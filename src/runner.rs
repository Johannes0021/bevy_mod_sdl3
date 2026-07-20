use std::{
    cell::RefCell,
    mem,
    num::NonZeroU8,
    thread,
    time::{Duration, Instant},
};

use bevy_app::{App, AppExit, PluginsState};
use bevy_ecs::{
    change_detection::Res,
    entity::Entity,
    system::{Query, SystemState},
    world::FromWorld,
};
use bevy_log::error;
use bevy_window::{Window, WindowDestroyed};

use sdl3::event::Event as SdlEvent;

#[cfg(target_os = "android")]
use crate::android;
use crate::{
    config::{FrameRate, SdlSettings},
    context::{CreateWindowParams, SdlContext, create_windows},
    event::{RawSdlEvent, forward_bevy_window_events, handle_sdl_event},
    monitors::{SyncMonitorsParams, sync_monitors},
};

const EXIT_FAILURE: NonZeroU8 = NonZeroU8::new(1).unwrap();
const SUSPENDED_FRAME_RATE: FrameRate = FrameRate::Limited {
    frame_time: Duration::from_millis(100),
};

pub(crate) struct RequestAppLoopExit(pub bool);

//==================================================================================================
// App
//==================================================================================================

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app_mut<R, F>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut App) -> R,
{
    APP.try_with(|app_cell| {
        let mut app_slot = app_cell
            .try_borrow_mut()
            .map_err(|_| "Failed to borrow Bevy App".to_string())?;

        let app = app_slot
            .as_mut()
            .ok_or_else(|| "Bevy App is not initialized".to_string())?;

        Ok(f(app))
    })
    .map_err(|_| "Failed to access thread-local Bevy App".to_string())?
}

//==================================================================================================
// AppLoop
//==================================================================================================

pub(crate) struct AppLoopState {
    pub suspended: bool,
    pub needs_to_create_sdl_windows: bool,
    pub destroyed_windows: Vec<Entity>,
    pub exit: bool,
}

pub(crate) fn app_loop(app: App) -> AppExit {
    APP.with_borrow_mut(|thread_local_app| *thread_local_app = Some(app));

    match app_loop_impl() {
        Ok(()) => AppExit::Success,

        Err(error) => {
            error!("Application loop failed: {error}");
            AppExit::Error(EXIT_FAILURE)
        }
    }
}

impl Default for AppLoopState {
    fn default() -> Self {
        Self {
            suspended: false,
            needs_to_create_sdl_windows: true,
            destroyed_windows: Default::default(),
            exit: false,
        }
    }
}

fn app_loop_impl() -> Result<(), String> {
    let (_event_watch, mut event_pump) = with_app_mut(|app| {
        if app.plugins_state() == PluginsState::Ready {
            app.finish();
            app.cleanup();
        }

        let sdl_context = app.world().non_send::<SdlContext>();

        let event_watch = sdl_context.event.add_event_watch(event_watch);

        let event_pump = sdl_context
            .sdl
            .event_pump()
            .inspect_err(|error| error!("Failed to create SDL event pump: {error}"))
            .expect("Failed to create SDL event pump");

        (event_watch, event_pump)
    })?;

    let mut did_init_monitor_sync = false;
    let mut last_iter = false;

    'app_loop: loop {
        let frame_start = Instant::now();

        if !with_app_mut(can_enter_app_loop)? {
            continue;
        }

        if !did_init_monitor_sync {
            did_init_monitor_sync = true;
            with_app_mut(run_sync_monitors_system)?;
        }

        with_app_mut(run_create_windows_system_if_needed)?;

        if !last_iter {
            'sdl_event_pump_loop: for sdl_event in event_pump.poll_iter() {
                let break_sdl_event_pump_loop = with_app_mut(|app| {
                    let mut bevy_window_events = Vec::new();
                    let RequestAppLoopExit(request_app_loop_exit) =
                        handle_sdl_event(app.world_mut(), &sdl_event, &mut bevy_window_events);

                    let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
                    sdl_context.app_loop_state.exit |= request_app_loop_exit;

                    app.world_mut().write_message(RawSdlEvent(sdl_event));

                    let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
                    for window in sdl_context.app_loop_state.destroyed_windows.drain(..) {
                        bevy_window_events.push(WindowDestroyed { window }.into());
                    }

                    if !bevy_window_events.is_empty() {
                        forward_bevy_window_events(app.world_mut(), bevy_window_events);
                    }

                    request_app_loop_exit
                })?;

                if break_sdl_event_pump_loop {
                    break 'sdl_event_pump_loop;
                }
            }
        }

        with_app_mut(try_update_app)?;

        if last_iter {
            break 'app_loop;
        }

        last_iter |= with_app_mut(should_exit)?;

        if last_iter {
            with_app_mut(|app| {
                if app.should_exit().is_none() {
                    app.world_mut().write_message(AppExit::Success);
                }
            })?;
        } else {
            with_app_mut(|app| apply_frame_pacing(app, frame_start))?;
        }
    }

    Ok(())
}

fn can_enter_app_loop(app: &mut App) -> bool {
    let plugins_cleaned = app.plugins_state() == PluginsState::Cleaned;

    if !plugins_cleaned {
        app.finish();
        app.cleanup();
    }

    plugins_cleaned
}

fn run_sync_monitors_system(app: &mut App) {
    let mut sync_monitors_state = SystemState::<SyncMonitorsParams>::from_world(app.world_mut());
    sync_monitors(sync_monitors_state.get_mut(app.world_mut()).unwrap());
    sync_monitors_state.apply(app.world_mut());
}

fn run_create_windows_system_if_needed(app: &mut App) {
    let needs_to_create_sdl_windows = mem::replace(
        &mut app
            .world_mut()
            .non_send_mut::<SdlContext>()
            .app_loop_state
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

fn try_update_app(app: &mut App) {
    let sdl_context = app.world().non_send::<SdlContext>();
    if !sdl_context.app_loop_state.suspended {
        app.update();
    }
}

fn should_exit(app: &mut App) -> bool {
    if app.should_exit().is_some() {
        return true;
    }

    let sdl_context = app.world().non_send::<SdlContext>();
    sdl_context.app_loop_state.exit
}

fn apply_frame_pacing(app: &mut App, frame_start: Instant) {
    let sdl_context = app.world().non_send::<SdlContext>();

    let frame_rate = if sdl_context.app_loop_state.suspended {
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

//==================================================================================================
// EventWatch
//==================================================================================================

/// Handles sdl lifecycle events that need direct access to the bevy app.
///
/// TODO: Verify with sdl documentation that these Android/iOS events always run on the sdl main
/// thread, which is also the thread that owns the bevy app. While testing, this appears to be the
/// case.
///
/// The events are handled immediately because Android and iOS may suspend or terminate the app
/// shortly afterwards. Handling them in the normal bevy update loop may be too late.
fn event_watch(event: SdlEvent) {
    let result = match event {
        SdlEvent::AppWillEnterBackground { timestamp: _ } => with_app_mut(|app| {
            #[cfg(target_os = "android")]
            android::trigger_surface_destruction(app.world_mut());

            let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
            sdl_context.app_loop_state.suspended = true;
        }),

        SdlEvent::AppDidEnterForeground { timestamp: _ } => with_app_mut(|app| {
            #[cfg(target_os = "android")]
            {
                let mut ensure_surface_exists_state =
                    SystemState::<android::EnsureSurfaceExistsParams>::from_world(app.world_mut());
                android::ensure_surface_exists(
                    ensure_surface_exists_state
                        .get_mut(app.world_mut())
                        .unwrap(),
                );
                ensure_surface_exists_state.apply(app.world_mut());
            }

            let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
            sdl_context.app_loop_state.suspended = false;
        }),

        _ => Ok(()),
    };

    if let Err(err) = result {
        error!("Failed to handle SDL application lifecycle event: {err}");
    }
}
