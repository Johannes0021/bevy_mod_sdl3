use std::{
    mem, thread,
    time::{Duration, Instant},
};

use bevy_app::{App, AppExit, PluginsState};
use bevy_ecs::{
    change_detection::Res,
    system::{Query, SystemState},
    world::FromWorld,
};
use bevy_window::{Window, WindowDestroyed};

use sdl3::event::{Event as SdlEvent, WindowEvent as SdlWindowEvent};

#[cfg(target_os = "android")]
use crate::android;
use crate::{
    config::{FrameRate, SdlSettings},
    context::{CreateWindowParams, SdlContext, create_windows},
    event::{RawSdlEvent, forward_bevy_window_events, handle_sdl_event},
    monitors::{SyncMonitorsParams, sync_monitors},
};

const SUSPENDED_FRAME_RATE: FrameRate = FrameRate::Limited {
    frame_time: Duration::from_millis(100),
};

pub(crate) struct RequestAppLoopBreak(pub bool);

pub(crate) fn app_loop(mut app: App) -> AppExit {
    if app.plugins_state() == PluginsState::Ready {
        app.finish();
        app.cleanup();
    }

    let mut event_pump = {
        let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
        sdl_context.event_pump.take().unwrap()
    };

    let mut startup_forced_updates = 5;
    let mut break_app_loop = false;
    let mut init_monitor_sync = false;
    let mut suspended = false;

    'app_loop: loop {
        let frame_start = Instant::now();

        if app.plugins_state() != PluginsState::Cleaned {
            app.finish();
            app.cleanup();
            continue;
        }

        if !init_monitor_sync {
            init_monitor_sync = true;

            let mut sync_monitors_state =
                SystemState::<SyncMonitorsParams>::from_world(app.world_mut());
            sync_monitors(sync_monitors_state.get_mut(app.world_mut()).unwrap());
            sync_monitors_state.apply(app.world_mut());
        }

        {
            let needs_to_create_sdl_windows = mem::replace(
                &mut app
                    .world_mut()
                    .non_send_mut::<SdlContext>()
                    .needs_to_create_sdl_windows,
                false,
            );

            if needs_to_create_sdl_windows {
                let mut create_windows_state =
                    SystemState::<CreateWindowParams>::from_world(app.world_mut());
                create_windows(create_windows_state.get_mut(app.world_mut()).unwrap());
                create_windows_state.apply(app.world_mut());
            }
        }

        let (break_app_loop_next_iter, do_app_update) = {
            let mut break_app_loop_next_iter = false;
            let mut do_app_update = !suspended || (startup_forced_updates > 0);
            let mut bevy_window_events = Vec::new();

            if !suspended || (startup_forced_updates == 0) {
                let mut sdl_events = Vec::new();
                'sdl_event_pump_loop: for sdl_event in event_pump.poll_iter() {
                    let RequestAppLoopBreak(request_app_loop_break) =
                        handle_sdl_event(app.world_mut(), &sdl_event, &mut bevy_window_events);

                    break_app_loop_next_iter |= request_app_loop_break;

                    if let SdlEvent::Window {
                        timestamp: _,
                        window_id: _,
                        win_event,
                    } = &sdl_event
                    {
                        if suspended && matches!(win_event, SdlWindowEvent::FocusGained) {
                            #[cfg(target_os = "android")]
                            {
                                let mut ensure_surface_exists_state =
                                    SystemState::<android::EnsureSurfaceExistsParams>::from_world(
                                        app.world_mut(),
                                    );
                                android::ensure_surface_exists(
                                    ensure_surface_exists_state
                                        .get_mut(app.world_mut())
                                        .unwrap(),
                                );
                                ensure_surface_exists_state.apply(app.world_mut());
                            }

                            suspended = false;
                            do_app_update = true;
                        } else if matches!(win_event, SdlWindowEvent::FocusLost) {
                            #[cfg(target_os = "android")]
                            android::trigger_surface_destruction(app.world_mut());

                            suspended = true;
                            do_app_update = true;

                            // Break and process the events because the loop might stall and die
                            // after this event, leaving the app with no chance to react.
                            break 'sdl_event_pump_loop;
                        }
                    }

                    sdl_events.push(sdl_event);
                }

                if !sdl_events.is_empty() {
                    app.world_mut()
                        .write_message_batch(sdl_events.into_iter().map(RawSdlEvent));
                }
            }

            let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
            for window in sdl_context.destroyed_windows.drain(..) {
                bevy_window_events.push(WindowDestroyed { window }.into());
            }

            if !bevy_window_events.is_empty() {
                forward_bevy_window_events(app.world_mut(), bevy_window_events);
            }

            (break_app_loop_next_iter, do_app_update)
        };

        if do_app_update {
            app.update();

            if startup_forced_updates > 0 {
                startup_forced_updates -= 1;
            }
        }

        if startup_forced_updates == 0 {
            if break_app_loop {
                break 'app_loop;
            } else if break_app_loop_next_iter || app.should_exit().is_some() {
                if app.should_exit().is_none() {
                    app.world_mut().write_message(AppExit::Success);
                }

                break_app_loop = true;
            } else {
                let frame_rate = if suspended {
                    SUSPENDED_FRAME_RATE
                } else {
                    let mut focused_windows_state: SystemState<(Res<SdlSettings>, Query<&Window>)> =
                        SystemState::new(app.world_mut());
                    let (settings, windows) = focused_windows_state.get(app.world()).unwrap();
                    let focused = windows.iter().any(|window| window.focused);

                    if focused {
                        settings.focused
                    } else {
                        settings.unfocused
                    }
                };
                match frame_rate {
                    FrameRate::Uncapped => {}

                    FrameRate::Limited { frame_time } => {
                        let elapsed = frame_start.elapsed();

                        if elapsed < frame_time {
                            let remaining = frame_time - elapsed;
                            thread::sleep(remaining);
                        }
                    }
                }
            }
        }
    }

    AppExit::Success
}
