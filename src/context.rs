use std::collections::HashMap;

use bevy_app::AppExit;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::{
    change_detection::{NonSendMut, Res},
    component::Component,
    entity::Entity,
    lifecycle::RemovedComponents,
    message::{MessageReader, MessageWriter},
    query::{Added, With},
    system::{Commands, Local, Query, SystemParamItem},
};
use bevy_input::keyboard::{Key, KeyCode};
use bevy_log::info;
use bevy_window::{
    ClosingWindow, CursorOptions, RawHandleWrapper, RawHandleWrapperHolder, Window, WindowClosed,
    WindowClosing, WindowCreated, WindowWrapper,
};

use sdl3::{EventPump as SdlEventPump, Sdl, VideoSubsystem as SdlVideoSubsystem};

use crate::{
    converters::theme_from_sdl,
    monitors::SdlMonitors,
    windows::{SdlWindowWrapper, SdlWindows, WindowId},
};

//==================================================================================================
// SdlContext
//==================================================================================================

pub struct SdlContext {
    pub sdl: Sdl,
    pub video: SdlVideoSubsystem,
    windows: SdlWindows,
    pub(crate) event_pump: SdlEventPump,
    pub(crate) needs_to_spawn_sdl_windows: bool,
}

impl SdlContext {
    pub fn init() -> Self {
        let sdl = sdl3::init().unwrap();
        let event_pump = sdl.event_pump().unwrap();
        let video = sdl.video().unwrap();

        Self {
            sdl,
            video,
            windows: SdlWindows::new(),
            event_pump,
            needs_to_spawn_sdl_windows: true,
        }
    }

    fn create_window(
        &mut self,
        entity: Entity,
        bevy_window: &Window,
        sdl_monitors: &SdlMonitors,
    ) -> &WindowWrapper<SdlWindowWrapper> {
        self.windows
            .create(&self.video, entity, bevy_window, sdl_monitors)
    }

    fn destroy_window(&mut self, entity: Entity) -> Option<WindowWrapper<SdlWindowWrapper>> {
        self.windows.destroy(entity)
    }

    fn get_window(&self, entity: Entity) -> Option<&WindowWrapper<SdlWindowWrapper>> {
        self.windows.get(entity)
    }

    fn get_window_entity(&self, window_id: WindowId) -> Option<Entity> {
        self.windows.get_entity(window_id)
    }
}

//==================================================================================================
// SdlWindowPressedKeys
//==================================================================================================

/// This keeps track of which keys are pressed on each window.
/// When a window is unfocused, this is used to send key release events for all the currently held
/// keys.
#[derive(Default, Component)]
pub struct SdlWindowPressedKeys(pub(crate) HashMap<KeyCode, Key>);

//==================================================================================================
// CachedWindow
//==================================================================================================

/// The cached state of the window so we can check which properties were changed from within the
/// app.
#[derive(Debug, Clone, Component, Deref, DerefMut)]
pub(crate) struct CachedWindow(Window);

//==================================================================================================
// CachedCursorOptions
//==================================================================================================

/// The cached state of the window so we can check which properties were changed from within the
/// app.
#[derive(Debug, Clone, Component, Deref, DerefMut)]
pub(crate) struct CachedCursorOptions(CursorOptions);

//==================================================================================================
// systems
//==================================================================================================

pub type SpawnWindowParams<'w, 's> = (
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

pub fn spawn_windows(
    (
        mut commands,
        mut sdl_context,
        sdl_monitors,
        mut window_created_events,
        mut created_windows,
    ): SystemParamItem<SpawnWindowParams>,
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
            SdlWindowPressedKeys::default(),
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
pub(crate) fn despawn_windows(
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
    // - take the `WindowWrapper`s out of `WINIT_WINDOWS` and into the local `windows_to_drop`
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
