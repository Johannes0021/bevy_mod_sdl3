use std::{any::Any, sync::mpsc};

use bevy_app::AppExit;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    change_detection::{DetectChangesMut, NonSendMut, Res},
    component::Component,
    entity::Entity,
    lifecycle::RemovedComponents,
    message::{MessageReader, MessageWriter},
    query::{Added, Changed, With},
    system::{Commands, Local, Query, SystemParamItem},
};
use bevy_log::{error, info};
use bevy_window::{
    ClosingWindow, CursorOptions, OnMonitor, RawHandleWrapper, RawHandleWrapperHolder, Window,
    WindowClosed, WindowClosing, WindowCreated, WindowEvent, WindowMode, WindowPosition,
    WindowResized, WindowScaleFactorChanged, WindowWrapper,
};

use sdl3::{
    EventPump as SdlEventPump, EventSubsystem as SdlEventSubsystem, Sdl,
    VideoSubsystem as SdlVideoSubsystem, event::Event as SdlEvent,
    mouse::MouseUtil as SdlMouseUtil, video::WindowPos as SdlWindowPos,
};

use crate::{
    converters::theme_from_sdl,
    monitors::{SdlDisplayModeExt, SdlMonitors},
    windows::{SdlWindowExt, SdlWindowWrapper, SdlWindows, WindowId},
};

//==================================================================================================
// SdlContext
//==================================================================================================

pub struct SdlContext {
    pub sdl: Sdl,
    pub event: SdlEventSubsystem,
    pub video: SdlVideoSubsystem,
    pub mouse: SdlMouseUtil,
    windows: SdlWindows,
    pub(crate) event_rx: mpsc::Receiver<SdlEvent>,
    pub(crate) _event_watch: Box<dyn Any + Send>,
    pub(crate) event_pump: SdlEventPump,
    pub(crate) needs_to_create_sdl_windows: bool,
    pub(crate) destroyed_windows: Vec<Entity>,
}

impl SdlContext {
    pub fn init() -> Self {
        let sdl = sdl3::init().unwrap();
        let event = sdl.event().unwrap();
        let (event_tx, event_rx) = mpsc::channel();
        let event_watch = Box::new(event.add_event_watch(move |event| {
            let _ = event_tx.send(event);
        }));
        let event_pump = sdl.event_pump().unwrap();
        let video = sdl.video().unwrap();
        let mouse = sdl.mouse();

        Self {
            sdl,
            event,
            video,
            mouse,
            windows: Default::default(),
            event_rx,
            _event_watch: event_watch,
            event_pump,
            needs_to_create_sdl_windows: true,
            destroyed_windows: Default::default(),
        }
    }

    pub(crate) fn create_window(
        &mut self,
        entity: Entity,
        bevy_window: &Window,
        sdl_monitors: &SdlMonitors,
    ) -> &WindowWrapper<SdlWindowWrapper> {
        self.windows
            .create(&self.video, entity, bevy_window, sdl_monitors)
    }

    pub(crate) fn destroy_window(
        &mut self,
        entity: Entity,
    ) -> Option<WindowWrapper<SdlWindowWrapper>> {
        let window = self.windows.destroy(entity);
        if window.is_some() {
            self.destroyed_windows.push(entity);
        }

        window
    }

    pub(crate) fn get_window(&self, entity: Entity) -> Option<&WindowWrapper<SdlWindowWrapper>> {
        self.windows.get(entity)
    }

    pub(crate) fn get_window_entity(&self, window_id: WindowId) -> Option<Entity> {
        self.windows.get_entity(window_id)
    }
}

//==================================================================================================
// CachedWindow
//==================================================================================================

#[derive(Debug, Clone, Component, Deref, DerefMut)]
pub(crate) struct CachedWindow(Window);

//==================================================================================================
// CachedCursorOptions
//==================================================================================================

#[derive(Debug, Clone, Component, Deref, DerefMut)]
pub(crate) struct CachedCursorOptions(CursorOptions);

//==================================================================================================
// Systems
//==================================================================================================

pub type CreateWindowParams<'w, 's> = (
    Commands<'w, 's>,
    NonSendMut<'w, SdlContext>,
    Res<'w, SdlMonitors>,
    MessageWriter<'w, WindowCreated>,
    Query<
        'w,
        's,
        (
            Entity,
            &'static mut Window,
            &'static CursorOptions,
            Option<&'static RawHandleWrapperHolder>,
        ),
        Added<Window>,
    >,
);

pub fn create_windows(
    (
        mut commands,
        mut sdl_context,
        sdl_monitors,
        mut window_created_events,
        mut created_windows,
    ): SystemParamItem<CreateWindowParams>,
) {
    for (entity, mut window, cursor_options, handle_holder) in &mut created_windows {
        if sdl_context.get_window(entity).is_some() {
            continue;
        }

        info!("Creating new window {} ({})", window.title.as_str(), entity);

        let sdl_window = sdl_context.create_window(entity, &window, &sdl_monitors);

        if let Some(theme) = theme_from_sdl(SdlVideoSubsystem::get_system_theme()) {
            window.window_theme = Some(theme);
        }

        window
            .resolution
            .set_scale_factor_and_apply_to_physical_size(sdl_window.display_scale());

        commands.entity(entity).insert((
            CachedWindow(window.clone()),
            CachedCursorOptions(cursor_options.clone()),
        ));

        if let Ok(handle_wrapper) = RawHandleWrapper::new(sdl_window) {
            commands.entity(entity).insert(handle_wrapper.clone());
            if let Some(handle_holder) = handle_holder {
                *handle_holder.0.lock().unwrap() = Some(handle_wrapper);
            }
        }

        #[cfg(target_os = "macos")]
        {
            // Request app activation via `raise()` if the window should start focused.
            if window.focused {
                let mut sdl_window = (*sdl_window).clone();
                sdl_window.raise();
            }
        }

        window_created_events.write(WindowCreated { window: entity });
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn destroy_windows(
    mut sdl_context: NonSendMut<SdlContext>,
    closing: Query<Entity, With<ClosingWindow>>,
    mut closed: RemovedComponents<Window>,
    window_entities: Query<Entity, With<Window>>,
    mut closing_event_writer: MessageWriter<WindowClosing>,
    mut closed_event_writer: MessageWriter<WindowClosed>,
    mut windows_to_drop: Local<Vec<WindowWrapper<SdlWindowWrapper>>>,
    mut exit_event_reader: MessageReader<AppExit>,
) {
    // Drop all the windows that are waiting to be closed.
    windows_to_drop.clear();

    for window in closing.iter() {
        closing_event_writer.write(WindowClosing { window });
    }

    for window in closed.read() {
        info!("Closing window {}", window);
        // Guard to verify that the window is in fact actually gone,
        // rather than having the component added and removed in the same frame.
        if !window_entities.contains(window) {
            if let Some(window) = sdl_context.destroy_window(window) {
                // Keeping WindowWrapper that are dropped for one frame.
                // Otherwise the last `Arc` of the window could be in the rendering thread,
                // and dropped there This would hang on macOS
                // Keeping the wrapper and dropping it next frame in this system ensure its dropped
                // in the main thread.
                windows_to_drop.push(window);
            }
            closed_event_writer.write(WindowClosed { window });
        }
    }

    // On macOS, many things need to be dropped on the main thread, or the app will hang:
    // - notify the rendering thread the windows are about to close
    // - take the `WindowWrapper`s out of `SdlWindows` and into the local `windows_to_drop`
    if !exit_event_reader.is_empty() {
        exit_event_reader.clear();

        for window in window_entities.iter() {
            closing_event_writer.write(WindowClosing { window });
            if let Some(wrapper) = sdl_context.destroy_window(window) {
                windows_to_drop.push(wrapper);
            }
        }
    }
}

pub(crate) fn changed_windows(
    mut commands: Commands,
    sdl_context: NonSendMut<SdlContext>,
    mut changed_windows: Query<
        (Entity, &mut Window, &mut CachedWindow, Option<&OnMonitor>),
        Changed<Window>,
    >,
    monitors: Res<SdlMonitors>,
    mut window_resized: MessageWriter<WindowResized>,
    mut window_event: MessageWriter<WindowEvent>,
    mut window_rescaled: MessageWriter<WindowScaleFactorChanged>,
) {
    for (entity, mut window, mut cache, monitor_relationship) in &mut changed_windows {
        let Some(mut sdl_window) = sdl_context.get_window(entity).map(|w| (*w).clone()) else {
            continue;
        };

        if window.title != cache.title
            && let Err(e) = sdl_window.set_title(window.title.as_str())
        {
            error!("Failed to set window title: {e}");
        }

        if window.mode != cache.mode {
            let (display_mode, fullscreen) = match window.mode {
                WindowMode::Windowed => (None, false),

                WindowMode::BorderlessFullscreen(monitor_selection) => {
                    let display_mode = sdl_window
                        .display_mode()
                        .ok_or_else(|| {
                            "Failed to update window mode: Couldn't get the display mode"
                                .to_string()
                        })
                        .and_then(|mut dm| {
                            dm.select_monitor(&sdl_context.video, &monitors, &monitor_selection)?;
                            Ok(dm)
                        });

                    (Some(display_mode), true)
                }

                WindowMode::Fullscreen(monitor_selection, video_mode_selection) => {
                    let display_mode = sdl_window
                        .display_mode()
                        .ok_or_else(|| {
                            "Failed to update window mode: Couldn't get the display mode"
                                .to_string()
                        })
                        .and_then(|mut dm| {
                            dm.select_monitor(&sdl_context.video, &monitors, &monitor_selection)?;
                            Ok(dm)
                        })
                        .and_then(|mut dm| {
                            dm.select_video_mode(&video_mode_selection)?;
                            Ok(dm)
                        });

                    (Some(display_mode), true)
                }
            };

            if let Some(display_mode) = display_mode
                && let Err(e) = display_mode.map(|dm| sdl_window.set_display_mode(dm))
            {
                error!(
                    "Failed to update display mode for window {}: {e}",
                    window.title
                );
            }

            if let Err(e) = sdl_window.set_fullscreen(fullscreen) {
                error!(
                    "Failed to set fullscreen for window {} ({fullscreen}): {e}",
                    window.title
                );
            }
        }

        // Set position before size so the window is on the correct monitor
        // (and thus using the correct scale factor) when size is applied.
        if window.position != cache.position {
            let (display_mode, pos_x, pos_y) = match window.position {
                WindowPosition::Automatic => {
                    (None, SdlWindowPos::Undefined, SdlWindowPos::Undefined)
                }

                WindowPosition::Centered(monitor_selection) => {
                    let display_mode = sdl_window
                        .display_mode()
                        .ok_or_else(|| {
                            "Failed to update window mode: Couldn't get the display mode"
                                .to_string()
                        })
                        .and_then(|mut dm| {
                            dm.select_monitor(&sdl_context.video, &monitors, &monitor_selection)?;
                            Ok(dm)
                        });

                    (
                        Some(display_mode),
                        SdlWindowPos::Centered,
                        SdlWindowPos::Centered,
                    )
                }

                WindowPosition::At(position) => (
                    None,
                    SdlWindowPos::Positioned(position.x),
                    SdlWindowPos::Positioned(position.y),
                ),
            };

            if let Some(display_mode) = display_mode
                && let Err(e) = display_mode.map(|dm| sdl_window.set_display_mode(dm))
            {
                error!(
                    "Failed to update display mode for window {}: {e}",
                    window.title
                );
            }

            sdl_window.set_position(pos_x, pos_y);
        }

        if window.resolution != cache.resolution {
            let cache_size = cache.resolution.size();
            let requested_size = window.resolution.size();

            if cache_size != requested_size {
                match sdl_window.set_size(requested_size.x as u32, requested_size.y as u32) {
                    Ok(()) => {
                        let event = WindowResized {
                            window: entity,
                            width: requested_size.x,
                            height: requested_size.y,
                        };
                        // Need to send two very similar events because different systems rely on
                        // those.
                        window_resized.write(event.clone());
                        window_event.write(event.into());
                    }
                    Err(e) => error!(
                        "Failed to set window size for window {} ({requested_size}): {e}",
                        window.title,
                    ),
                }
            }

            let cache_scale_factor = cache.scale_factor();
            let requested_scale_factor = window.scale_factor();

            if cache_scale_factor != requested_scale_factor {
                // If the scale factor has changed we don't query anything from sdl, but send events
                // for camera system to handle.
                let event = WindowScaleFactorChanged {
                    scale_factor: requested_scale_factor as f64,
                    window: entity,
                };
                // Need to send two very similar events because different systems rely on those.
                window_rescaled.write(event.clone());
                window_event.write(event.into());
            }
        }

        if window.cursor_position() != cache.cursor_position()
            && let Some(cursor_position) = window.cursor_position()
        {
            sdl_context.mouse.warp_mouse_in_window(
                &sdl_window,
                cursor_position.x,
                cursor_position.y,
            );
        }

        if window.decorations != cache.decorations {
            sdl_window.set_bordered(window.decorations);
        }

        if window.resizable != cache.resizable {
            // TODO: I couldn't find a way to change an sdl window's resizability after it has been
            // created.
            error!(
                "Unable to change window resizability after creation for window {}. \
                Attempted value: {}.",
                window.title, window.resizable,
            );
        }

        if window.enabled_buttons != cache.enabled_buttons {
            // TODO: I don't know how to replicate winit's `enabled_buttons` behavior in sdl.
            // I think sdl doesn't currently provide an equivalent api for enabling/disabling
            // individual window controls.
            error!(
                "Setting `enabled_buttons` is not supported right now. Window title: {}. \
                Attempted value: {:?}.",
                window.title, window.enabled_buttons
            );
        }

        if window.resize_constraints != cache.resize_constraints {
            let constraints = window.resize_constraints.check_constraints();

            if let Err(e) = sdl_window
                .set_minimum_size(constraints.min_width as u32, constraints.min_height as u32)
            {
                error!(
                    "Failed to set minimum size for window {} (min_width: {}, min_height: {}): {e}",
                    window.title, constraints.min_width as u32, constraints.min_height as u32,
                );
            }

            if constraints.max_width.is_finite() && constraints.max_height.is_finite() {
                if let Err(e) = sdl_window
                    .set_maximum_size(constraints.max_width as u32, constraints.max_height as u32)
                {
                    error!(
                        "Failed to set maximum size for window {} \
                        (max_width: {}, max_height: {}): {e}",
                        window.title, constraints.max_width as u32, constraints.max_height as u32,
                    );
                }
            } else {
                error!(
                    "Failed to set maximum size for window {} \
                    (max_width: {}, max_height: {}): not finite",
                    window.title, constraints.max_width, constraints.max_height,
                );
            }
        }

        if let Some(monitor_link) = monitor_relationship {
            if let Some(sdl_monitor) = sdl_window.display_mode().map(|dm| dm.display) {
                if let Some(linked_monitor) = monitors.find(monitor_link.0)
                    && &sdl_monitor != linked_monitor
                    && let Some(sdl_monitor_entity) = monitors.find_entity(&sdl_monitor)
                {
                    commands
                        .entity(entity)
                        .insert(OnMonitor(sdl_monitor_entity.to_owned()));
                }
            } else {
                commands.entity(entity).remove::<OnMonitor>();
            }
        } else if let Some(sdl_monitor) = sdl_window.display_mode().map(|dm| dm.display)
            && let Some(sdl_monitor_entity) = monitors.find_entity(&sdl_monitor)
        {
            commands
                .entity(entity)
                .insert(OnMonitor(sdl_monitor_entity.to_owned()));
        }

        if let Some(maximized) = window.internal.take_maximize_request() {
            if maximized {
                sdl_window.maximize();
            } else {
                sdl_window.restore();
            }
        }

        if let Some(minimized) = window.internal.take_minimize_request() {
            if minimized {
                sdl_window.minimize();
            } else {
                sdl_window.restore();
            }
        }

        if window.internal.take_move_request() {
            // TODO: I don't know how to handle `take_move_request()` in sdl.
            error!(
                "Window move request is not supported right now. Window title: {}.",
                window.title,
            );
        }

        if let Some(resize_direction) = window.internal.take_resize_request() {
            // TODO: I don't know how to handle `take_resize_request()` in sdl.
            error!(
                "Window resize request is not supported right now. Window title: {}. \
                Attempted value: {:?}.",
                window.title, resize_direction,
            );
        }

        if window.focused != cache.focused && window.focused && !sdl_window.raise() {
            error!("Failed to raise the window {}", window.title);
        }

        if window.window_level != cache.window_level {
            // TODO: I don't know how to handle `window_level` in sdl.
            error!(
                "Window level is not supported right now. Window title: {}. Attempted value: {:?}.",
                window.title, window.window_level,
            );
        }

        if window.transparent != cache.transparent {
            // TODO: I don't know how to handle `transparent` in sdl.
            error!(
                "Window transparent is not supported right now. Window title: {}. \
                Attempted value: {}.",
                window.title, window.transparent,
            );
        }

        #[cfg(target_arch = "wasm32")]
        if window.canvas != cache.canvas {
            window.canvas.clone_from(&cache.canvas);
            error!(
                "Bevy currently doesn't support modifying the window canvas after initialization. \
                Window title: {}",
                window.title,
            );
        }

        if window.ime_enabled != cache.ime_enabled {
            // TODO: I don't know how to handle `ime_enabled` in sdl.
            error!(
                "Window ime_enabled is not supported right now. Window title: {}. \
                Attempted value: {}.",
                window.title, window.ime_enabled,
            );
        }

        if window.ime_position != cache.ime_position {
            // TODO: I don't know how to handle `ime_enabled` in sdl.
            error!(
                "Window ime_position is not supported right now. Window title: {}. \
                Attempted value: {}.",
                window.title, window.ime_position,
            );
        }

        if window.window_theme != cache.window_theme {
            // TODO: I don't know how to handle `window_theme` in sdl.
            error!(
                "Window window_theme is not supported right now. Window title: {}. \
                Attempted value: {:?}.",
                window.title, window.window_theme,
            );
        }

        if window.visible != cache.visible {
            if window.visible {
                sdl_window.show();
            } else {
                sdl_window.hide();
            };
        }

        #[cfg(target_os = "ios")]
        {
            if window.recognize_pinch_gesture != cache.recognize_pinch_gesture {
                // TODO: I don't know how to handle `recognize_pinch_gesture` in sdl.
                error!(
                    "Window recognize_pinch_gesture is not supported right now. Window title: {}. \
                    Attempted value: {}.",
                    window.title, window.recognize_pinch_gesture,
                );
            }

            if window.recognize_rotation_gesture != cache.recognize_rotation_gesture {
                // TODO: I don't know how to handle `recognize_rotation_gesture` in sdl.
                error!(
                    "Window recognize_rotation_gesture is not supported right now. \
                    Window title: {}. Attempted value: {}.",
                    window.title, window.recognize_rotation_gesture,
                );
            }

            if window.recognize_doubletap_gesture != cache.recognize_doubletap_gesture {
                // TODO: I don't know how to handle `recognize_doubletap_gesture` in sdl.
                error!(
                    "Window recognize_doubletap_gesture is not supported right now. \
                    Window title: {}. Attempted value: {}.",
                    window.title, window.recognize_doubletap_gesture,
                );
            }

            if window.recognize_pan_gesture != cache.recognize_pan_gesture {
                // TODO: I don't know how to handle `recognize_pan_gesture` in sdl.
                error!(
                    "Window recognize_pan_gesture is not supported right now. Window title: {}. \
                    Attempted value: {:?}.",
                    window.title, window.recognize_pan_gesture,
                );
            }

            if window.prefers_home_indicator_hidden != cache.prefers_home_indicator_hidden {
                // TODO: I don't know how to handle `prefers_home_indicator_hidden` in sdl.
                error!(
                    "Window prefers_home_indicator_hidden is not supported right now. \
                    Window title: {}. Attempted value: {}.",
                    window.title, window.prefers_home_indicator_hidden,
                );
            }

            if window.prefers_status_bar_hidden != cache.prefers_status_bar_hidden {
                // TODO: I don't know how to handle `prefers_status_bar_hidden` in sdl.
                error!(
                    "Window prefers_status_bar_hidden is not supported right now. \
                    Window title: {}. Attempted value: {}.",
                    window.title, window.prefers_status_bar_hidden,
                );
            }

            if window.preferred_screen_edges_deferring_system_gestures
                != cache.preferred_screen_edges_deferring_system_gestures
            {
                // TODO: `preferred_screen_edges_deferring_system_gestures`
                // I don't know how to handle this in sdl.
                error!(
                    "Window preferred_screen_edges_deferring_system_gestures is not supported \
                    right now. Window title: {}. Attempted value: {:?}.",
                    window.title, window.preferred_screen_edges_deferring_system_gestures,
                );
            }
        }

        **cache = window.clone();
    }
}

pub(crate) fn changed_cursor_options(
    sdl_context: NonSendMut<SdlContext>,
    mut changed_windows: Query<
        (
            Entity,
            &Window,
            &mut CursorOptions,
            &mut CachedCursorOptions,
        ),
        // TODO: Changed<CursorOptions>, Look at how CursorOptions::visible is handled in this fn.
    >,
) {
    for (entity, window, mut cursor_options, mut cache) in &mut changed_windows {
        // This system already only runs when the cursor options change, so we need to bypass
        // change detection or the next frame will also run this system
        let cursor_options = cursor_options.bypass_change_detection();
        let Some(mut sdl_window) = sdl_context.get_window(entity).map(|w| (*w).clone()) else {
            continue;
        };

        // Don't check the cache for the grab mode.
        // It can change through external means, leaving the cache outdated.
        if let Err(e) = sdl_window.attempt_grab(cursor_options.grab_mode) {
            error!(
                "Could not set cursor grab mode for window {}: {}",
                window.title, e
            );
            cursor_options.grab_mode = cache.grab_mode;
        } else {
            cache.grab_mode = cursor_options.grab_mode;
        }

        if sdl_window.has_mouse_focus() {
            if cursor_options.visible != sdl_context.mouse.is_cursor_showing() {
                sdl_context.mouse.show_cursor(cursor_options.visible);
            }
            cache.visible = cursor_options.visible;
        }

        if cursor_options.hit_test != cache.hit_test {
            // TODO: I don't know how to handle `hit_test` in sdl.
            error!(
                "Window hit_test is not supported right now. Windowt title: {}. \
                Attempted value: {}.",
                window.title, cursor_options.hit_test,
            );
            cache.hit_test = cursor_options.hit_test;
        }
    }
}
