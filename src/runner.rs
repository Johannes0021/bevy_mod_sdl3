use std::mem;

use bevy_app::{App, AppExit, PluginsState};
use bevy_ecs::{system::SystemState, world::FromWorld};
use bevy_window::WindowDestroyed;

#[cfg(target_os = "android")]
use crate::android;
use crate::{
    context::{self, SdlContext, SpawnWindowParams},
    event::{RawSdlEvent, forward_bevy_window_events, handle_sdl_event},
    monitors::{self, SyncMonitorsParams},
};

const SUSPENDED_DELAY_MS: u32 = 100;

pub(crate) enum RequestAppLoopState {
    Continue,
    SuspendAndContinue,
    ResumeAndContinue,
    Break,
}

pub(crate) fn app_loop(mut app: App) -> AppExit {
    if app.plugins_state() == PluginsState::Ready {
        app.finish();
        app.cleanup();
    }

    let mut break_after_next_app_loop = false;
    let mut init_monitor_sync = false;
    let mut suspended = false;

    'app_loop: loop {
        if app.plugins_state() != PluginsState::Cleaned {
            app.finish();
            app.cleanup();
            continue;
        }

        if !init_monitor_sync {
            init_monitor_sync = true;

            let mut sync_monitors = SystemState::<SyncMonitorsParams>::from_world(app.world_mut());
            monitors::sync_monitors(sync_monitors.get_mut(app.world_mut()).unwrap());
            sync_monitors.apply(app.world_mut());
        }

        {
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

        #[derive(Default)]
        struct IterState {
            break_app_loop: bool,
            suspend: bool,
            #[cfg(target_os = "android")]
            trigger_surface_destruction: bool,
        }

        let iter_state = {
            let mut iter_state = IterState::default();
            let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();

            let mut bevy_window_events = Vec::new();
            for window in sdl_context.destroyed_windows.drain(..) {
                bevy_window_events.push(WindowDestroyed { window }.into());
            }

            // While testing, I noticed that on iOS the application lifecycle events are only
            // delivered through the sdl event watch and are not received via
            // EventPump::poll_iter(). Therefore, lifecycle events are forwarded to the loop thread
            // through the event channel. There may be a better design, or I may be missing
            // something.
            for _ in sdl_context.event_pump.poll_iter() {}
            let sdl_events: Vec<_> = sdl_context.event_rx.try_iter().collect();

            for event in &sdl_events {
                match handle_sdl_event(app.world_mut(), event, &mut bevy_window_events) {
                    RequestAppLoopState::Continue => (),
                    RequestAppLoopState::SuspendAndContinue => {
                        #[cfg(target_os = "android")]
                        {
                            iter_state.trigger_surface_destruction = true;
                        }
                        iter_state.suspend = true;
                    }
                    RequestAppLoopState::ResumeAndContinue => iter_state.suspend = false,
                    RequestAppLoopState::Break => iter_state.break_app_loop = true,
                }
            }

            if !sdl_events.is_empty() {
                app.world_mut()
                    .write_message_batch(sdl_events.into_iter().map(RawSdlEvent));
            }

            if !bevy_window_events.is_empty() {
                forward_bevy_window_events(app.world_mut(), bevy_window_events);
            }

            #[cfg(target_os = "android")]
            {
                iter_state.trigger_surface_destruction |= iter_state.break_app_loop;
            }
            iter_state.suspend |= iter_state.break_app_loop;

            iter_state
        };

        #[cfg(target_os = "android")]
        {
            if iter_state.trigger_surface_destruction {
                android::trigger_surface_destruction(app.world_mut());
            }

            if suspended && !iter_state.suspend {
                let mut ensure_surface_exists =
                    SystemState::<android::EnsureSurfaceExistsParams>::from_world(app.world_mut());
                android::ensure_surface_exists(
                    ensure_surface_exists.get_mut(app.world_mut()).unwrap(),
                );
                ensure_surface_exists.apply(app.world_mut());
            }
        }

        if !suspended {
            app.update();
        }

        suspended = iter_state.suspend;

        if break_after_next_app_loop {
            break 'app_loop;
        } else if iter_state.break_app_loop || app.should_exit().is_some() {
            if app.should_exit().is_none() {
                app.world_mut().write_message(AppExit::Success);
            }

            break_after_next_app_loop = true;
        }

        if suspended {
            sdl3::timer::delay(SUSPENDED_DELAY_MS);
        }
    }

    AppExit::Success
}
