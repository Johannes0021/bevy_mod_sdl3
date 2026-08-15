#[cfg(target_os = "android")]
use crate::android;
use crate::{
    config::{FrameRate, SdlSettings},
    context::{CreateWindowParams, SdlContext, create_windows},
    event::{RawSdlEvent, forward_bevy_window_events, handle_sdl_event},
    monitors::{SyncMonitorsParams, sync_monitors},
};
use bevy_app::{App, AppExit, PluginsState};
use bevy_ecs::{
    change_detection::Res,
    entity::Entity,
    system::{Query, SystemState},
    world::FromWorld,
};
use bevy_log::error;
use bevy_window::{AppLifecycle, Window, WindowDestroyed, WindowEvent};
use sdl3::event::Event as SdlEvent;
use std::{cell::RefCell, mem, num::NonZeroU8, thread, time::Instant};

const EXIT_FAILURE: NonZeroU8 = NonZeroU8::new(1).unwrap();

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
    raw_sdl_events: Vec<RawSdlEvent>,
    bevy_window_events: Vec<WindowEvent>,
    pub destroyed_windows: Vec<Entity>,
    lifecycle: AppLifecycle,
    last_app_update_lifecycle: AppLifecycle,
    pub needs_to_create_sdl_windows: bool,
    exit: bool,
}

impl Default for AppLoopState {
    fn default() -> Self {
        Self {
            raw_sdl_events: Vec::default(),
            bevy_window_events: vec![WindowEvent::AppLifecycle(AppLifecycle::Idle)],
            destroyed_windows: Default::default(),
            lifecycle: AppLifecycle::Idle,
            last_app_update_lifecycle: AppLifecycle::Idle,
            needs_to_create_sdl_windows: true,
            exit: false,
        }
    }
}

impl AppLoopState {
    fn apply_lifecycle(app: &mut App, lifecycle: AppLifecycle) {
        let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
        let this = &mut sdl_context.app_loop_state;

        this.bevy_window_events
            .push(WindowEvent::AppLifecycle(lifecycle));
        this.lifecycle = lifecycle;

        if this.last_app_update_lifecycle != this.lifecycle {
            AppLoopState::force_update_app(app);
        }
    }

    fn try_update_app(app: &mut App) {
        let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
        let this = &mut sdl_context.app_loop_state;
        if this.lifecycle.is_active() {
            AppLoopState::force_update_app(app);
        }
    }

    fn force_update_app(app: &mut App) {
        let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
        let this = &mut sdl_context.app_loop_state;

        this.last_app_update_lifecycle = this.lifecycle;

        let raw_sdl_events = mem::take(&mut this.raw_sdl_events);
        let bevy_window_events = mem::take(&mut this.bevy_window_events);

        if !raw_sdl_events.is_empty() {
            app.world_mut().write_message_batch(raw_sdl_events);
        }

        forward_bevy_window_events(app.world_mut(), bevy_window_events);

        app.update();

        run_create_windows_system_if_needed(app);
    }
}

pub(crate) fn app_loop(app: App) -> AppExit {
    APP.with_borrow_mut(|thread_local_app| *thread_local_app = Some(app));

    let result = app_loop_impl();

    APP.with_borrow_mut(|thread_local_app| *thread_local_app = None);

    match result {
        Ok(()) => AppExit::Success,
        Err(error) => {
            error!("Application loop failed: {error}");
            AppExit::Error(EXIT_FAILURE)
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

        with_app_mut(|app| {
            let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
            let app_loop_state = &mut sdl_context.app_loop_state;
            if app_loop_state.lifecycle == AppLifecycle::Idle {
                AppLoopState::apply_lifecycle(app, AppLifecycle::Running);
            }
        })?;

        if !did_init_monitor_sync {
            did_init_monitor_sync = true;
            with_app_mut(run_sync_monitors_system)?;
        }

        if !last_iter {
            for sdl_event in event_pump.poll_iter() {
                with_app_mut(|app| {
                    let mut bevy_window_events = Vec::new();
                    let RequestAppLoopExit(request_app_loop_exit) =
                        handle_sdl_event(app.world_mut(), &sdl_event, &mut bevy_window_events);

                    let mut sdl_context = app.world_mut().non_send_mut::<SdlContext>();
                    let app_loop_state = &mut sdl_context.app_loop_state;

                    app_loop_state.exit |= request_app_loop_exit;

                    app_loop_state.raw_sdl_events.push(RawSdlEvent(sdl_event));

                    for window in app_loop_state.destroyed_windows.drain(..) {
                        bevy_window_events.push(WindowDestroyed { window }.into());
                    }
                    app_loop_state.bevy_window_events.extend(bevy_window_events);
                })?;
            }
        }

        if last_iter {
            with_app_mut(|app| {
                if app.should_exit().is_none() {
                    app.world_mut().write_message(AppExit::Success);
                }

                AppLoopState::force_update_app(app)
            })?;

            break 'app_loop;
        } else {
            with_app_mut(AppLoopState::try_update_app)?;
        }

        last_iter |= with_app_mut(should_exit)?;
        if !last_iter {
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

fn should_exit(app: &mut App) -> bool {
    if app.should_exit().is_some() {
        return true;
    }

    app.world().non_send::<SdlContext>().app_loop_state.exit
}

fn apply_frame_pacing(app: &mut App, frame_start: Instant) {
    let is_active = app
        .world()
        .non_send::<SdlContext>()
        .app_loop_state
        .lifecycle
        .is_active();

    let mut focused_windows_state: SystemState<(Res<SdlSettings>, Query<&Window>)> =
        SystemState::new(app.world_mut());
    let (settings, windows) = focused_windows_state.get(app.world()).unwrap();
    let frame_rate = if is_active {
        let focused = windows.iter().any(|window| window.focused);

        if focused {
            settings.focused
        } else {
            settings.unfocused
        }
    } else {
        settings.suspended
    };

    match frame_rate {
        FrameRate::Uncapped => (),

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

            AppLoopState::apply_lifecycle(app, AppLifecycle::WillSuspend);
        }),

        SdlEvent::AppDidEnterBackground { timestamp: _ } => with_app_mut(|app| {
            AppLoopState::apply_lifecycle(app, AppLifecycle::Suspended);
        }),

        SdlEvent::AppWillEnterForeground { timestamp: _ } => with_app_mut(|app| {
            AppLoopState::apply_lifecycle(app, AppLifecycle::WillResume);
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

            AppLoopState::apply_lifecycle(app, AppLifecycle::Running);
        }),

        _ => Ok(()),
    };

    if let Err(err) = result {
        error!("Failed to handle SDL application lifecycle event: {err}");
    }
}
