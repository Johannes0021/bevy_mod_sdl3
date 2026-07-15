use std::mem;

use bevy_app::{App, AppExit, PluginsState};
use bevy_ecs::{system::SystemState, world::FromWorld};
use bevy_log::info;

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

    let mut event_pump = app
        .world_mut()
        .non_send_mut::<SdlContext>()
        .sdl
        .event_pump()
        .unwrap();

    'outer: loop {
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

        for event in event_pump.poll_iter() {
            if let SdlEvent::Quit { .. } = event {
                break 'outer;
            }
        }

        if app.plugins_state() == PluginsState::Cleaned {
            app.update();
        }
    }

    AppExit::Success
}
