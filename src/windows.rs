use std::{collections::HashMap, marker::PhantomData};

use bevy_derive::{Deref, DerefMut};
use bevy_ecs::entity::{Entity, EntityHashMap};
use bevy_log::{debug, error};
use bevy_window::{
    CursorGrabMode, CursorOptions, Window, WindowMode, WindowPosition, WindowWrapper,
};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

use sdl3::{
    VideoSubsystem as SdlVideoSubsystem, mouse::MouseUtil as SdlMouseUtil,
    video::Window as SdlWindow,
};

use crate::monitors::{SdlDisplayModeExt, SdlMonitors, get_refresh_rate_millihertz};

//==================================================================================================
// WindowId
//==================================================================================================

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct WindowId(pub u32);

impl From<u32> for WindowId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

//==================================================================================================
// SdlWindow
//==================================================================================================

#[derive(Deref, DerefMut)]
pub(crate) struct SdlWindowWrapper(pub(crate) SdlWindow);

// TODO: I don't know if this is safe...
//
// We need this because WindowWrapper<W> expects W to be Send + Sync.
// I think Bevy needs this for rendering and to satisfy its lifetime requirements.
//
// Access to the wrapped sdl window must remain private to this crate to ensure that the window is
// only accessed from the thread on which it was created.
unsafe impl Send for SdlWindowWrapper {}
unsafe impl Sync for SdlWindowWrapper {}

impl HasWindowHandle for SdlWindowWrapper {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for SdlWindowWrapper {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

//==================================================================================================
// SdlWindows
//==================================================================================================

#[derive(Default)]
pub(crate) struct SdlWindows {
    windows: HashMap<WindowId, WindowWrapper<SdlWindowWrapper>>,
    entity_to_sdl: EntityHashMap<WindowId>,
    sdl_to_entity: HashMap<WindowId, Entity>,
    _not_send_sync: PhantomData<*const ()>,
}

impl SdlWindows {
    pub fn create(
        &mut self,
        video: &SdlVideoSubsystem,
        mouse: &SdlMouseUtil,
        entity: Entity,
        window: &Window,
        cursor_options: &CursorOptions,
        sdl_monitors: &SdlMonitors,
    ) -> &WindowWrapper<SdlWindowWrapper> {
        let mut sdl_window_builder = video.window(
            window.name.as_ref().unwrap_or(&"Bevy".to_string()),
            window.width() as u32,
            window.height() as u32,
        );

        let (monitor_selection, video_mode_selection, fullscreen) = match window.mode {
            WindowMode::Windowed => {
                let monitor_selection = match window.position {
                    WindowPosition::Automatic => {
                        sdl_window_builder.position_centered();
                        None
                    }

                    WindowPosition::Centered(monitor_selection) => {
                        sdl_window_builder.position_centered();
                        Some(monitor_selection)
                    }

                    WindowPosition::At(position) => {
                        sdl_window_builder.position(position.x, position.y);
                        None
                    }
                };

                (monitor_selection, None, false)
            }

            WindowMode::BorderlessFullscreen(monitor_selection) => {
                (Some(monitor_selection), None, true)
            }

            WindowMode::Fullscreen(monitor_selection, video_mode_selection) => {
                (Some(monitor_selection), Some(video_mode_selection), true)
            }
        };

        if fullscreen {
            sdl_window_builder.fullscreen();
        }

        if window.resizable {
            sdl_window_builder.resizable();
        }

        if !window.decorations {
            sdl_window_builder.borderless();
        }

        let mut sdl_window = sdl_window_builder
            .high_pixel_density()
            .metal_view()
            .build()
            .inspect_err(|error| error!("Failed to build SDL window: {error}"))
            .expect("Failed to build SDL window");

        let constraints = window.resize_constraints.check_constraints();
        if let Err(e) =
            sdl_window.set_minimum_size(constraints.min_width as u32, constraints.min_height as u32)
        {
            error!(
                "Failed to set minimum size for window {} (min_width: {}, min_height: {}): {e}",
                window.title, constraints.min_width as u32, constraints.min_height as u32,
            );
        }

        if constraints.max_width.is_finite()
            && constraints.max_height.is_finite()
            && let Err(e) = sdl_window
                .set_maximum_size(constraints.max_width as u32, constraints.max_height as u32)
        {
            error!(
                "Failed to set maximum size for window {} \
                (max_width: {}, max_height: {}): {e}",
                window.title, constraints.max_width as u32, constraints.max_height as u32,
            );
        }

        if monitor_selection.is_some() || video_mode_selection.is_some() {
            let display_mode = sdl_window
                .display_mode()
                .ok_or_else(|| {
                    "Failed to update window mode: Couldn't get the display mode".to_string()
                })
                .and_then(|mut dm| {
                    if let Some(monitor_selection) = monitor_selection {
                        dm.select_monitor(video, sdl_monitors, &monitor_selection)?;
                    }
                    Ok(dm)
                })
                .and_then(|mut dm| {
                    if let Some(video_mode_selection) = video_mode_selection {
                        dm.select_video_mode(&video_mode_selection)?;
                    }
                    Ok(dm)
                });

            if let Err(e) = display_mode.map(|dm| sdl_window.set_display_mode(dm)) {
                error!(
                    "Failed to update display mode for window {}: {e}",
                    window.title
                );
            }
        }

        if window.focused && !sdl_window.raise() {
            error!("Failed to raise the window {}", window.title);
        }

        // Do not set the grab mode on window creation if it's none. It can fail on mobile.
        if cursor_options.grab_mode != CursorGrabMode::None
            && let Err(e) = sdl_window.attempt_grab(cursor_options.grab_mode)
        {
            error!(
                "Could not set cursor grab mode for window {}: {}",
                window.title, e
            );
        }

        if sdl_window.has_mouse_focus() && cursor_options.visible != mouse.is_cursor_showing() {
            mouse.show_cursor(cursor_options.visible);
        }

        if let Some(display_mode) = sdl_window.display_mode() {
            let display_info = DisplayInfo {
                window_physical_resolution: (
                    window.resolution.physical_width(),
                    window.resolution.physical_height(),
                ),
                window_logical_resolution: (window.resolution.width(), window.resolution.height()),
                monitor_name: display_mode.display.get_name().ok(),
                scale_factor: Some(sdl_window.display_scale() as f64),
                refresh_rate_millihertz: get_refresh_rate_millihertz(&display_mode),
            };
            debug!("{display_info}");
        } else {
            match sdl_window.get_display() {
                Ok(display) => {
                    let display_info = DisplayInfo {
                        window_physical_resolution: (
                            window.resolution.physical_width(),
                            window.resolution.physical_height(),
                        ),
                        window_logical_resolution: (
                            window.resolution.width(),
                            window.resolution.height(),
                        ),
                        monitor_name: display.get_name().ok(),
                        scale_factor: Some(sdl_window.display_scale() as f64),
                        refresh_rate_millihertz: None,
                    };
                    debug!("{display_info}");
                }

                Err(e) => error!("Failed to get display from window {}: {e}", window.title),
            }
        }

        let id = WindowId(sdl_window.id());

        self.windows
            .insert(id, WindowWrapper::new(SdlWindowWrapper(sdl_window)));
        self.entity_to_sdl.insert(entity, id);
        self.sdl_to_entity.insert(id, entity);

        self.windows.get(&id).unwrap()
    }

    pub fn destroy(&mut self, entity: Entity) -> Option<WindowWrapper<SdlWindowWrapper>> {
        let id = self.entity_to_sdl.remove(&entity)?;
        self.sdl_to_entity.remove(&id);
        self.windows.remove(&id)
    }

    pub fn get(&self, entity: Entity) -> Option<&WindowWrapper<SdlWindowWrapper>> {
        let id = self.entity_to_sdl.get(&entity)?;
        self.windows.get(id)
    }

    pub fn get_entity(&self, window_id: WindowId) -> Option<Entity> {
        self.sdl_to_entity.get(&window_id).copied()
    }
}

//==================================================================================================
// SdlWindowExt
//==================================================================================================

pub(crate) trait SdlWindowExt {
    fn attempt_grab(&mut self, grab_mode: CursorGrabMode) -> Result<(), String>;
}

impl SdlWindowExt for SdlWindow {
    fn attempt_grab(&mut self, grab_mode: CursorGrabMode) -> Result<(), String> {
        match grab_mode {
            CursorGrabMode::None => {
                if !self.set_mouse_grab(false) {
                    return Err("Failed to release mouse grab".to_string());
                }
            }
            CursorGrabMode::Confined => {
                if !self.set_mouse_grab(true) {
                    return Err("Failed to grab mouse".to_string());
                }
            }
            CursorGrabMode::Locked => {
                return Err("CursorGrabMode::Locked is not directly supported by sdl".to_string());
            }
        }

        Ok(())
    }
}

//==================================================================================================
// SdlWindowExt
//==================================================================================================

struct DisplayInfo {
    window_physical_resolution: (u32, u32),
    window_logical_resolution: (f32, f32),
    monitor_name: Option<String>,
    scale_factor: Option<f64>,
    refresh_rate_millihertz: Option<u32>,
}

impl core::fmt::Display for DisplayInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Display information:")?;
        write!(
            f,
            "  Window physical resolution: {}x{}",
            self.window_physical_resolution.0, self.window_physical_resolution.1
        )?;
        write!(
            f,
            "  Window logical resolution: {}x{}",
            self.window_logical_resolution.0, self.window_logical_resolution.1
        )?;
        write!(
            f,
            "  Monitor name: {}",
            self.monitor_name.as_deref().unwrap_or("")
        )?;
        write!(f, "  Scale factor: {}", self.scale_factor.unwrap_or(0.))?;
        let millihertz = self.refresh_rate_millihertz.unwrap_or(0);
        let hertz = millihertz / 1000;
        let extra_millihertz = millihertz % 1000;
        write!(f, "  Refresh rate (Hz): {hertz}.{extra_millihertz:03}")?;
        Ok(())
    }
}
