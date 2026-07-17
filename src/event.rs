use std::path::PathBuf;

use approx::relative_eq;

use bevy_ecs::{
    change_detection::NonSendMut,
    component::Component,
    entity::Entity,
    message::Message,
    system::{Query, SystemParamItem, SystemState},
    world::{FromWorld, World},
};
use bevy_input::{
    ButtonState,
    keyboard::KeyboardInput,
    mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel},
    touch::TouchPhase,
};
use bevy_math::{DVec2, IVec2, Vec2};
use bevy_window::{
    CursorEntered, CursorLeft, CursorMoved, FileDragAndDrop, Window,
    WindowBackendScaleFactorChanged, WindowCloseRequested, WindowEvent, WindowFocused, WindowMoved,
    WindowOccluded, WindowResized, WindowScaleFactorChanged,
};

use sdl3::event::{Event as SdlEvent, WindowEvent as SdlWindowEvent};

use crate::{
    context::SdlContext,
    converters::{
        keycode_from_sdl, mouse_button_from_sdl, scancode_from_sdl, touch_event_from_sdl,
    },
    monitors::{self, SyncMonitorsParams},
    runner::RequestBreakAppLoop,
};

//==================================================================================================
// RawSdlEvent
//==================================================================================================

#[derive(Debug, Clone, PartialEq, Message)]
pub struct RawSdlEvent(pub SdlEvent);

//==================================================================================================
// FileDragAndDropSuccess
//==================================================================================================

#[derive(Component, Clone, Copy)]
struct FileDragAndDropSuccess(bool);

//==================================================================================================
// SdlEvent
//==================================================================================================

pub(crate) fn handle_sdl_event(
    world: &mut World,
    sdl_event: &SdlEvent,
    bevy_window_events: &mut Vec<WindowEvent>,
) -> RequestBreakAppLoop {
    match sdl_event {
        SdlEvent::Quit { timestamp: _ } | SdlEvent::AppTerminating { timestamp: _ } => {
            return RequestBreakAppLoop(true);
        }

        SdlEvent::AppLowMemory { timestamp: _ } => {} // TODO?

        SdlEvent::AppWillEnterBackground { timestamp: _ } => (), // TODO!!!

        SdlEvent::AppDidEnterBackground { timestamp: _ } => (), // TODO!!!

        SdlEvent::AppWillEnterForeground { timestamp: _ } => (), // TODO!!!

        SdlEvent::AppDidEnterForeground { timestamp: _ } => (), // TODO!!!

        SdlEvent::Window {
            timestamp: _,
            window_id,
            win_event,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some(entity) = sdl_context.get_window_entity((*window_id).into()) {
                handle_sdl_window_event(world, entity, *win_event, bevy_window_events);
            }
        }

        SdlEvent::KeyDown {
            timestamp: _,
            window_id,
            keycode,
            scancode,
            keymod: _,
            repeat,
            which: _,
            raw: _,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some((entity, key_code, logical_key)) = sdl_context
                .get_window_entity((*window_id).into())
                .and_then(|entity| {
                    let key_code = scancode.map(scancode_from_sdl)?;
                    let logical_key = keycode.map(keycode_from_sdl)?;
                    Some((entity, key_code, logical_key))
                })
            {
                bevy_window_events.push(
                    KeyboardInput {
                        key_code,
                        logical_key,
                        state: ButtonState::Pressed,
                        text: None, // TODO: Try to translate this event to text?
                        repeat: *repeat,
                        window: entity,
                    }
                    .into(),
                );
            }
        }

        SdlEvent::KeyUp {
            timestamp: _,
            window_id,
            keycode,
            scancode,
            keymod: _,
            repeat,
            which: _,
            raw: _,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some((entity, key_code, logical_key)) = sdl_context
                .get_window_entity((*window_id).into())
                .and_then(|entity| {
                    let key_code = scancode.map(scancode_from_sdl)?;
                    let logical_key = keycode.map(keycode_from_sdl)?;
                    Some((entity, key_code, logical_key))
                })
            {
                bevy_window_events.push(
                    KeyboardInput {
                        key_code,
                        logical_key,
                        state: ButtonState::Released,
                        text: None, // TODO: Try to translate this event to text?
                        repeat: *repeat,
                        window: entity,
                    }
                    .into(),
                );
            }
        }

        SdlEvent::TextEditing {
            timestamp: _,
            window_id,
            text,
            start,
            length,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let Some(entity) = windows.get_window_entity(window_id) else {
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::Ime(bevy_window::Ime::Preedit {
                    window: entity,
                    value: text.clone(),
                    cursor: Some((start as usize, (start + length) as usize)),
                }));
            });
            */
        }

        SdlEvent::TextInput {
            timestamp: _,
            window_id,
            text,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let Some(entity) = windows.get_window_entity(window_id) else {
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::Ime(bevy_window::Ime::Commit {
                    window: entity,
                    value: text.clone(),
                }));
            });
            */
        }

        SdlEvent::MouseMotion {
            timestamp: _,
            window_id,
            which: _,
            mousestate: _,
            x,
            y,
            xrel,
            yrel,
        } => {
            let delta = Vec2::new(*xrel, *yrel);
            bevy_window_events.push(MouseMotion { delta }.into());

            let sdl_context = world.non_send::<SdlContext>();
            if let Some((entity, cursor_position, cursor_delta)) = sdl_context
                .get_window_entity((*window_id).into())
                .and_then(|entity| {
                    try_with_window(world, entity, |window| {
                        let physical_position = DVec2::new(*x as f64, *y as f64);

                        let last_position = window.physical_cursor_position();
                        let delta = last_position.map(|last_pos| {
                            (physical_position.as_vec2() - last_pos)
                                / window.resolution.scale_factor()
                        });

                        window.set_physical_cursor_position(Some(physical_position));
                        let position =
                            (physical_position / window.resolution.scale_factor() as f64).as_vec2();

                        (entity, position, delta)
                    })
                })
            {
                bevy_window_events.push(
                    CursorMoved {
                        window: entity,
                        position: cursor_position,
                        delta: cursor_delta,
                    }
                    .into(),
                );
            }
        }

        SdlEvent::MouseButtonDown {
            timestamp: _,
            window_id,
            which: _,
            mouse_btn,
            clicks: _,
            x: _,
            y: _,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some((entity, button)) = sdl_context
                .get_window_entity((*window_id).into())
                .and_then(|entity| {
                    let button = mouse_button_from_sdl(*mouse_btn)?;
                    Some((entity, button))
                })
            {
                bevy_window_events.push(
                    MouseButtonInput {
                        button,
                        state: ButtonState::Pressed,
                        window: entity,
                    }
                    .into(),
                );
            }
        }

        SdlEvent::MouseButtonUp {
            timestamp: _,
            window_id,
            which: _,
            mouse_btn,
            clicks: _,
            x: _,
            y: _,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some((entity, button)) = sdl_context
                .get_window_entity((*window_id).into())
                .and_then(|entity| {
                    let button = mouse_button_from_sdl(*mouse_btn)?;
                    Some((entity, button))
                })
            {
                bevy_window_events.push(
                    MouseButtonInput {
                        button,
                        state: ButtonState::Released,
                        window: entity,
                    }
                    .into(),
                );
            }
        }

        SdlEvent::MouseWheel {
            timestamp: _,
            window_id,
            which: _,
            x,
            y,
            direction: _, // TODO: Do we have to take this into account?
            mouse_x: _,
            mouse_y: _,
            integer_x: _,
            integer_y: _,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some(entity) = sdl_context.get_window_entity((*window_id).into()) {
                bevy_window_events.push(
                    MouseWheel {
                        unit: MouseScrollUnit::Line,
                        x: *x,
                        y: *y,
                        window: entity,
                        phase: TouchPhase::Moved,
                    }
                    .into(),
                );
            }
        }

        SdlEvent::JoyAxisMotion { .. } => (), // TODO

        SdlEvent::JoyHatMotion { .. } => (), // TODO

        SdlEvent::JoyButtonDown { .. } => (), // TODO

        SdlEvent::JoyButtonUp { .. } => (), // TODO

        SdlEvent::JoyDeviceAdded { .. } => (), // TODO

        SdlEvent::JoyDeviceRemoved { .. } => (), // TODO

        SdlEvent::ControllerAxisMotion { .. } => (), // TODO

        SdlEvent::ControllerButtonDown { .. } => (), // TODO

        SdlEvent::ControllerButtonUp { .. } => (), // TODO

        SdlEvent::ControllerDeviceAdded { .. } => (), // TODO

        SdlEvent::ControllerDeviceRemoved { .. } => (), // TODO

        SdlEvent::ControllerDeviceRemapped { .. } => (), // TODO

        SdlEvent::ControllerTouchpadDown { .. } => (), // TODO

        SdlEvent::ControllerTouchpadMotion { .. } => (), // TODO

        SdlEvent::ControllerTouchpadUp { .. } => (), // TODO

        SdlEvent::FingerDown {
            timestamp: _,
            touch_id: _,
            finger_id,
            x,
            y,
            dx: _,
            dy: _,
            pressure,
            window_id,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some((entity, logical_position)) = sdl_context
                .get_window_entity((*window_id).into())
                .and_then(|entity| {
                    try_with_window(world, entity, |window| {
                        let logical_position = window.size() * Vec2::new(*x, *y);
                        (entity, logical_position)
                    })
                })
            {
                bevy_window_events.push(
                    touch_event_from_sdl(
                        TouchPhase::Started,
                        *finger_id as i64,
                        logical_position,
                        *pressure,
                        entity,
                    )
                    .into(),
                );
            }
        }

        SdlEvent::FingerUp {
            timestamp: _,
            touch_id: _,
            finger_id,
            x,
            y,
            dx: _,
            dy: _,
            pressure,
            window_id,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some((entity, logical_position)) = sdl_context
                .get_window_entity((*window_id).into())
                .and_then(|entity| {
                    try_with_window(world, entity, |window| {
                        let logical_position = window.size() * Vec2::new(*x, *y);
                        (entity, logical_position)
                    })
                })
            {
                bevy_window_events.push(
                    touch_event_from_sdl(
                        TouchPhase::Ended,
                        *finger_id as i64,
                        logical_position,
                        *pressure,
                        entity,
                    )
                    .into(),
                );
            }
        }

        SdlEvent::FingerMotion {
            timestamp: _,
            touch_id: _,
            finger_id,
            x,
            y,
            dx: _,
            dy: _,
            pressure,
            window_id,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some((entity, logical_position)) = sdl_context
                .get_window_entity((*window_id).into())
                .and_then(|entity| {
                    try_with_window(world, entity, |window| {
                        let logical_position = window.size() * Vec2::new(*x, *y);
                        (entity, logical_position)
                    })
                })
            {
                bevy_window_events.push(
                    touch_event_from_sdl(
                        TouchPhase::Moved,
                        *finger_id as i64,
                        logical_position,
                        *pressure,
                        entity,
                    )
                    .into(),
                );
            }
        }

        SdlEvent::DollarRecord {
            timestamp,
            touch_id,
            gesture_id,
            num_fingers,
            error,
            x,
            y,
        } => (),

        SdlEvent::MultiGesture {
            timestamp,
            touch_id,
            d_theta,
            d_dist,
            x,
            y,
            num_fingers,
        } => (),

        SdlEvent::ClipboardUpdate { timestamp: _ } => (),

        SdlEvent::DropFile {
            timestamp: _,
            window_id,
            filename,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some(entity) = sdl_context.get_window_entity((*window_id).into()) {
                world
                    .entity_mut(entity)
                    .insert(FileDragAndDropSuccess(true));
                // Workaround see: SdlEvent::DropBegin
                bevy_window_events.push(
                    FileDragAndDrop::HoveredFile {
                        window: entity,
                        path_buf: PathBuf::from(filename),
                    }
                    .into(),
                );
                bevy_window_events.push(
                    FileDragAndDrop::DroppedFile {
                        window: entity,
                        path_buf: PathBuf::from(filename),
                    }
                    .into(),
                );
            }
        }

        // TODO: I think bevy doesn't currently support this.
        SdlEvent::DropText {
            timestamp: _,
            window_id: _,
            filename: _, // Why is this field called `filename` instead of `text`?
        } => (),

        // TODO: `FileDragAndDrop::HoveredFile` should be sent here, but the filename is not
        // available. As a workaround, we emit `FileDragAndDrop::HoveredFile` and immediately
        // follow it with `FileDragAndDrop::DroppedFile` when handling `SdlEvent::DropFile`.
        SdlEvent::DropBegin {
            timestamp: _,
            window_id,
        } => {
            let sdl_context = world.non_send::<SdlContext>();
            if let Some(entity) = sdl_context.get_window_entity((*window_id).into()) {
                world
                    .entity_mut(entity)
                    .insert(FileDragAndDropSuccess(false));
            }
        }

        SdlEvent::DropComplete {
            timestamp: _,
            window_id,
        } => {
            let entity = {
                let sdl_context = world.non_send::<SdlContext>();
                sdl_context.get_window_entity((*window_id).into())
            };

            if let Some(entity) = entity {
                let mut entity_mut = world.entity_mut(entity);

                if let Some(FileDragAndDropSuccess(success)) =
                    entity_mut.get::<FileDragAndDropSuccess>().copied()
                {
                    entity_mut.remove::<FileDragAndDropSuccess>();

                    if !success {
                        bevy_window_events
                            .push(FileDragAndDrop::HoveredFileCanceled { window: entity }.into());
                    }
                }
            }
        }

        SdlEvent::AudioDeviceAdded { .. } => (), // TODO

        SdlEvent::AudioDeviceRemoved { .. } => (), // TODO

        SdlEvent::PenProximityIn { .. } => (), // TODO

        SdlEvent::PenProximityOut { .. } => (), // TODO

        // TODO: Consider handling this as pointer input?
        SdlEvent::PenDown { .. } => (),

        // TODO: Consider handling this as pointer input?
        SdlEvent::PenUp { .. } => (),

        // TODO: Consider handling this as pointer input?
        SdlEvent::PenMotion { .. } => (),

        // TODO: Consider handling this as pointer input?
        SdlEvent::PenButtonUp { .. } => (),

        // TODO: Consider handling this as pointer input?
        SdlEvent::PenButtonDown { .. } => (),

        SdlEvent::PenAxis { .. } => (), // TODO

        SdlEvent::RenderTargetsReset { .. } => (), // TODO?

        SdlEvent::RenderDeviceReset { .. } => (), // TODO?

        SdlEvent::User { .. } => (), // TODO?

        SdlEvent::Unknown {
            timestamp: _,
            type_: _,
        } => (),

        SdlEvent::Display {
            timestamp: _,
            display: _,
            display_event: _,
        } => {
            let mut sync_monitors = SystemState::<SyncMonitorsParams>::from_world(world);
            monitors::sync_monitors(sync_monitors.get_mut(world).unwrap());
            sync_monitors.apply(world);

            let mut sync_window_scale_factors_state =
                SystemState::<SyncWindowScaleFactorsParams>::from_world(world);
            sync_window_scale_factors(
                sync_window_scale_factors_state.get_mut(world).unwrap(),
                bevy_window_events,
            );
            sync_window_scale_factors_state.apply(world);
        }
    }

    RequestBreakAppLoop(false)
}

//==================================================================================================
// SdlWindowEvent
//==================================================================================================

pub fn handle_sdl_window_event(
    world: &mut World,
    entity: Entity,
    sdl_window_event: SdlWindowEvent,
    bevy_window_events: &mut Vec<WindowEvent>,
) {
    match sdl_window_event {
        SdlWindowEvent::None => (),

        SdlWindowEvent::Shown => {
            try_with_window(world, entity, |w| w.visible = true);
        }

        SdlWindowEvent::Hidden => {
            try_with_window(world, entity, |w| w.visible = false);
        }

        SdlWindowEvent::Exposed => bevy_window_events.push(
            WindowOccluded {
                window: entity,
                occluded: false,
            }
            .into(),
        ),

        SdlWindowEvent::Moved(x, y) => {
            let position = IVec2::new(x, y);
            try_with_window(world, entity, |w| w.position.set(position));
            bevy_window_events.push(
                WindowMoved {
                    window: entity,
                    position,
                }
                .into(),
            );
        }

        SdlWindowEvent::Resized(width, height) => {
            if let Some(size) = try_with_window(world, entity, |window| {
                window.resolution.set(width as f32, height as f32);
                window.size()
            }) {
                bevy_window_events.push(
                    WindowResized {
                        window: entity,
                        width: size.x,
                        height: size.y,
                    }
                    .into(),
                );
            }
        }

        SdlWindowEvent::PixelSizeChanged(width, height) => {
            if let Some(size) = try_with_window(world, entity, |window| {
                window
                    .resolution
                    .set_physical_resolution(width as u32, height as u32);
                window.size()
            }) {
                bevy_window_events.push(
                    WindowResized {
                        window: entity,
                        width: size.x,
                        height: size.y,
                    }
                    .into(),
                );
            }
        }

        SdlWindowEvent::Minimized => {
            try_with_window(world, entity, |w| w.set_minimized(true));
        }

        SdlWindowEvent::Maximized => {
            try_with_window(world, entity, |w| w.set_maximized(true));
        }

        SdlWindowEvent::Occluded => bevy_window_events.push(
            WindowOccluded {
                window: entity,
                occluded: true,
            }
            .into(),
        ),

        SdlWindowEvent::Restored => (),

        SdlWindowEvent::MouseEnter => {
            bevy_window_events.push(CursorEntered { window: entity }.into());
        }

        SdlWindowEvent::MouseLeave => {
            try_with_window(world, entity, |w| w.set_physical_cursor_position(None));
            bevy_window_events.push(CursorLeft { window: entity }.into());
        }

        SdlWindowEvent::FocusGained => {
            try_with_window(world, entity, |w| w.focused = true);
            bevy_window_events.push(
                WindowFocused {
                    window: entity,
                    focused: true,
                }
                .into(),
            );
        }

        SdlWindowEvent::FocusLost => {
            try_with_window(world, entity, |w| w.focused = false);
            bevy_window_events.push(
                WindowFocused {
                    window: entity,
                    focused: false,
                }
                .into(),
            );
        }

        SdlWindowEvent::CloseRequested => {
            bevy_window_events.push(WindowCloseRequested { window: entity }.into());
        }

        SdlWindowEvent::HitTest(_, _) => (),

        SdlWindowEvent::ICCProfChanged => (),

        SdlWindowEvent::DisplayChanged(_) => (),
    }

    // Just to make this sure...
    let mut sync_window_scale_factors_state =
        SystemState::<SyncWindowScaleFactorsParams>::from_world(world);
    sync_window_scale_factors(
        sync_window_scale_factors_state.get_mut(world).unwrap(),
        bevy_window_events,
    );
    sync_window_scale_factors_state.apply(world);
}

//==================================================================================================
// BevyWindowEvents
//==================================================================================================

pub(crate) fn forward_bevy_window_events(world: &mut World, window_events: Vec<WindowEvent>) {
    for window_event in window_events.iter() {
        match window_event.clone() {
            WindowEvent::AppLifecycle(e) => {
                world.write_message(e);
            }
            WindowEvent::CursorEntered(e) => {
                world.write_message(e);
            }
            WindowEvent::CursorLeft(e) => {
                world.write_message(e);
            }
            WindowEvent::CursorMoved(e) => {
                world.write_message(e);
            }
            WindowEvent::FileDragAndDrop(e) => {
                world.write_message(e);
            }
            WindowEvent::Ime(e) => {
                world.write_message(e);
            }
            WindowEvent::RequestRedraw(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowBackendScaleFactorChanged(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowCloseRequested(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowCreated(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowDestroyed(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowFocused(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowMoved(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowOccluded(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowResized(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowScaleFactorChanged(e) => {
                world.write_message(e);
            }
            WindowEvent::WindowThemeChanged(e) => {
                world.write_message(e);
            }
            WindowEvent::MouseButtonInput(e) => {
                world.write_message(e);
            }
            WindowEvent::MouseMotion(e) => {
                world.write_message(e);
            }
            WindowEvent::MouseWheel(e) => {
                world.write_message(e);
            }
            WindowEvent::PinchGesture(e) => {
                world.write_message(e);
            }
            WindowEvent::RotationGesture(e) => {
                world.write_message(e);
            }
            WindowEvent::DoubleTapGesture(e) => {
                world.write_message(e);
            }
            WindowEvent::PanGesture(e) => {
                world.write_message(e);
            }
            WindowEvent::TouchInput(e) => {
                world.write_message(e);
            }
            WindowEvent::KeyboardInput(e) => {
                world.write_message(e);
            }
            WindowEvent::KeyboardFocusLost(e) => {
                world.write_message(e);
            }
        }
    }

    if !window_events.is_empty() {
        world.write_message_batch(window_events);
    }
}

//==================================================================================================
// Helpers
//==================================================================================================

fn try_with_window<F, R>(world: &mut World, entity: Entity, f: F) -> Option<R>
where
    F: FnOnce(&mut Window) -> R,
{
    if let Ok(mut entity_mut) = world.get_entity_mut(entity)
        && let Some(mut window) = entity_mut.get_mut::<Window>()
    {
        Some(f(&mut window))
    } else {
        None
    }
}

pub type SyncWindowScaleFactorsParams<'w, 's> = (
    NonSendMut<'w, SdlContext>,
    Query<'w, 's, (Entity, &'static mut Window)>,
);

pub(crate) fn sync_window_scale_factors(
    (sdl_context, mut windows): SystemParamItem<SyncWindowScaleFactorsParams>,
    bevy_window_events: &mut Vec<WindowEvent>,
) {
    for (window_entity, mut window) in &mut windows {
        let Some(scale_factor) = sdl_context
            .get_window(window_entity)
            .map(|w| w.display_scale() as f64)
        else {
            continue;
        };

        let (window_backend_scale_factor_changed, window_scale_factor_changed) =
            react_to_scale_factor_change(window_entity, &mut window, scale_factor);

        bevy_window_events.push(window_backend_scale_factor_changed.into());
        if let Some(window_scale_factor_changed) = window_scale_factor_changed {
            bevy_window_events.push(window_scale_factor_changed.into());
        }
    }
}

fn react_to_scale_factor_change(
    window_entity: Entity,
    window: &mut Window,
    scale_factor: f64,
) -> (
    WindowBackendScaleFactorChanged,
    Option<WindowScaleFactorChanged>,
) {
    let prior_factor = window.resolution.scale_factor();
    window.resolution.set_scale_factor(scale_factor as f32);

    let window_backend_scale_factor_changed = WindowBackendScaleFactorChanged {
        window: window_entity,
        scale_factor,
    };

    let scale_factor_override = window.resolution.scale_factor_override();

    let window_scale_factor_changed =
        if scale_factor_override.is_none() && !relative_eq!(scale_factor as f32, prior_factor) {
            let window_scale_factor_changed = WindowScaleFactorChanged {
                window: window_entity,
                scale_factor,
            };
            Some(window_scale_factor_changed)
        } else {
            None
        };

    (
        window_backend_scale_factor_changed,
        window_scale_factor_changed,
    )
}
