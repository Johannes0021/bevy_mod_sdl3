use std::mem;

use bevy_app::{App, AppExit, PluginsState};
use bevy_ecs::{system::SystemState, world::FromWorld};

use sdl3::event::Event as SdlEvent;

use crate::{
    context::{self, SdlContext, SpawnWindowParams},
    monitors::{self, SyncMonitorsParams},
};

pub fn app_loop(mut app: App) -> AppExit {
    if app.plugins_state() == PluginsState::Ready {
        app.finish();
        app.cleanup();
    }

    'app_loop: loop {
        if app.plugins_state() != PluginsState::Cleaned {
            app.finish();
            app.cleanup();
        }

        {
            let mut sync_monitors = SystemState::<SyncMonitorsParams>::from_world(app.world_mut());
            monitors::sync_monitors(sync_monitors.get_mut(app.world_mut()).unwrap());
            sync_monitors.apply(app.world_mut());

            let needs_to_spawn_sdl_windows = mem::replace(
                &mut app
                    .world_mut()
                    .non_send_mut::<SdlContext>()
                    .needs_to_spawn_sdl_windows,
                false,
            );

            if needs_to_spawn_sdl_windows {
                let mut spawn_windows =
                    SystemState::<SpawnWindowParams>::from_world(app.world_mut());
                context::spawn_windows(spawn_windows.get_mut(app.world_mut()).unwrap());
                spawn_windows.apply(app.world_mut());
            }
        }

        let quit = {
            let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
            let mut quit = false;

            for event in sdl_context.event_pump.poll_iter() {
                match event {
                    SdlEvent::Quit { timestamp: _ } | SdlEvent::AppTerminating { timestamp: _ } => {
                        quit = true
                    }
                    _ => (),
                }
            }

            quit
        };

        if app.plugins_state() == PluginsState::Cleaned {
            app.update();

            if quit || app.should_exit().is_some() {
                break 'app_loop;
            }
        }
    }

    AppExit::Success
}
