pub use context::*;
pub use monitors::*;
pub use windows::*;

pub use sdl3;

use bevy_app::{App, Last, /*OnAppExitSystems,*/ Plugin};
use bevy_ecs::{
    change_detection::NonSendMut, lifecycle::Add, observer::On, schedule::IntoScheduleConfigs,
};
use bevy_window::{ExitSystems, Window};

mod context;
mod converters;
mod monitors;
mod runner;
mod windows;

pub struct Sdl3Plugin;

impl Plugin for Sdl3Plugin {
    fn build(&self, app: &mut App) {
        app.insert_non_send(SdlContext::init())
            .init_resource::<SdlMonitors>()
            .set_runner(runner::app_loop)
            .add_systems(
                Last,
                (
                    //changed_windows,
                    //changed_cursor_options,
                    despawn_windows.after(ExitSystems), //.after(OnAppExitSystems),
                                                        //check_keyboard_focus_lost,
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
