/*
 * TODO:
 *  - Bevy relies on AndroidApp from android-activity crate to access the AssetManager.
 *  - Frame pacing option (without VSync (Impl bevy_winit/src/winit_config.rs))
 *  - Do we need to release input when focus is lost?
 *  - Does sdl3 support SdlEvent::MouseWheel MouseScrollUnit::Pixel?
 *      - In bevy_winit/src/state.rs look at: WindowEvent::MouseWheel
 *  - I couldn't find SDL_EVENT_FINGER_CANCELED in sdl3-rs Event (TouchPhase::Canceled);
 *      - In bevy_winit/src/state.rs look at: WindowEvent::Touch
 *  - I couldn't find SDL_EVENT_SYSTEM_THEME_CHANGED in sdl3-rs Event (ThemeChanged);
 *      - In bevy_winit/src/state.rs look at: WindowEvent::ThemeChanged
 *  - In bevy_winit/src/state.rs look at:
 *      - fn window_event
 *          - PinchGesture
 *          - RotationGesture
 *          - DoubleTapGesture
 *          - PanGesture
 *          - WindowEvent::Ime
 * - in bevy_winit/src/system.rs look at:
 *     - fn create_window (incomplete look at crate::windows::SdlWindows::create_window)
 *     - fn changed_windows
 *     - fn changed_cursor_options
 *     - Understand why ... exists:
 *          - CachedWindow
 *          - CachedCursorOptions
 * - Impl bevy_winit/src/cursor/mod.rs
 * - Impl bevy_winit/src/accessibility.rs
 */
pub use context::*;
pub use event::RawSdlEvent;
pub use monitors::*;
pub use windows::*;

pub use sdl3;

use bevy_app::{App, Last, Plugin};
use bevy_ecs::{
    change_detection::NonSendMut, lifecycle::Add, observer::On, schedule::IntoScheduleConfigs,
};
use bevy_window::{ExitSystems, Window};

#[cfg(target_os = "android")]
mod android;
mod context;
mod converters;
mod event;
mod monitors;
mod runner;
mod windows;

pub struct Sdl3Plugin;

impl Plugin for Sdl3Plugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(SdlContext::init())
            .init_resource::<SdlMonitors>()
            .add_message::<RawSdlEvent>()
            .set_runner(runner::app_loop)
            .add_systems(
                Last,
                (
                    //changed_windows,
                    //changed_cursor_options,
                    despawn_windows.after(ExitSystems), /* TODO: .after(OnAppExitSystems), */
                )
                    .chain(),
            )
            .add_observer(
                |_window: On<Add, Window>, mut sdl_context: NonSendMut<SdlContext>| {
                    sdl_context.needs_to_spawn_sdl_windows = true;
                },
            );
    }
}
