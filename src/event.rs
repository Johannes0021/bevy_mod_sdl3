use bevy_ecs::{entity::Entity, message::Message, world::World};
use bevy_input::mouse::MouseMotion;
use bevy_math::{DVec2, IVec2, Vec2};
use bevy_window::{
    CursorEntered, CursorLeft, CursorMoved, Window, WindowCloseRequested, WindowEvent,
    WindowFocused, WindowMoved, WindowOccluded, WindowResized,
};

use sdl3::event::{Event as SdlEvent, WindowEvent as SdlWindowEvent};

use crate::{context::SdlContext, runner::RequestBreakAppLoop};

//==================================================================================================
// RawSdlEvent
//==================================================================================================

#[derive(Debug, Clone, PartialEq, Message)]
pub struct RawSdlEvent(pub SdlEvent);

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

        SdlEvent::AppLowMemory { timestamp: _ } => {}

        SdlEvent::AppWillEnterBackground { timestamp: _ } => (),

        SdlEvent::AppDidEnterBackground { timestamp: _ } => (),

        SdlEvent::AppWillEnterForeground { timestamp: _ } => (),

        SdlEvent::AppDidEnterForeground { timestamp: _ } => (),

        SdlEvent::Window {
            timestamp: _,
            window_id,
            win_event,
        } => {
            let sdl_context = world.non_send_mut::<SdlContext>();
            if let Some(entity) = sdl_context.get_window_entity((*window_id).into()) {
                handle_sdl_window_event(world, entity, *win_event, bevy_window_events);
            }
        }

        SdlEvent::KeyDown {
            timestamp: _,
            window_id,
            keycode,
            scancode,
            keymod,
            repeat,
            which,
            raw,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let entity = windows
                    .get_window_entity(window_id)
                    .expect("Window entity not found");
                let Some(key_code) = scancode.and_then(convert_sdl_scancode) else {
                    return;
                };
                let Some(logical_key) = keycode.and_then(convert_sdl_keycode) else {
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::KeyboardInput(
                    bevy_input::keyboard::KeyboardInput {
                        key_code,
                        logical_key,
                        state: bevy_input::ButtonState::Pressed,
                        text: None,
                        repeat,
                        window: entity,
                    },
                ));
            });
            */
        }

        SdlEvent::KeyUp {
            timestamp: _,
            window_id,
            keycode,
            scancode,
            keymod,
            repeat,
            which,
            raw,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let entity = windows
                    .get_window_entity(window_id)
                    .expect("Window entity not found");
                let Some(key_code) = scancode.and_then(convert_sdl_scancode) else {
                    return;
                };
                let Some(logical_key) = keycode.and_then(convert_sdl_keycode) else {
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::KeyboardInput(
                    bevy_input::keyboard::KeyboardInput {
                        key_code,
                        logical_key,
                        state: bevy_input::ButtonState::Released,
                        text: None,
                        repeat,
                        window: entity,
                    },
                ));
            });
            */
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
            let delta = Vec2::new(*xrel as f32, *yrel as f32);
            bevy_window_events.push(MouseMotion { delta }.into());

            let sdl_context = world.non_send_mut::<SdlContext>();
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
            which,
            mouse_btn,
            clicks,
            x,
            y,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let entity = windows
                    .get_window_entity(window_id)
                    .expect("Window entity not found");
                let Some(button) = convert_sdl_mouse_btn(mouse_btn) else {
                    error!("Unknown mouse button: {:?}", mouse_btn);
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::MouseButtonInput(
                    bevy_input::mouse::MouseButtonInput {
                        button,
                        state: bevy_input::ButtonState::Pressed,
                        window: entity,
                    },
                ));
            });
            */
        }

        SdlEvent::MouseButtonUp {
            timestamp: _,
            window_id,
            which,
            mouse_btn,
            clicks,
            x,
            y,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let entity = windows
                    .get_window_entity(window_id)
                    .expect("Window entity not found");
                let Some(button) = convert_sdl_mouse_btn(mouse_btn) else {
                    error!("Unknown mouse button: {:?}", mouse_btn);
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::MouseButtonInput(
                    bevy_input::mouse::MouseButtonInput {
                        button,
                        state: bevy_input::ButtonState::Released,
                        window: entity,
                    },
                ));
            });
            */
        }

        SdlEvent::MouseWheel {
            timestamp: _,
            window_id,
            which,
            x,
            y,
            direction,
            mouse_x,
            mouse_y,
            integer_x,
            integer_y,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let entity = windows
                    .get_window_entity(window_id)
                    .expect("Window entity not found");
                bevy_window_events.push(bevy_window::WindowEvent::MouseWheel(
                    bevy_input::mouse::MouseWheel {
                        unit: bevy_input::mouse::MouseScrollUnit::Line,
                        x: precise_x,
                        y: precise_y,
                        window: entity,
                    },
                ));
            });
            */
        }

        SdlEvent::JoyAxisMotion {
            timestamp,
            which,
            axis_idx,
            value,
        } => (),

        SdlEvent::JoyHatMotion {
            timestamp,
            which,
            hat_idx,
            state,
        } => (),

        SdlEvent::JoyButtonDown {
            timestamp,
            which,
            button_idx,
        } => (),

        SdlEvent::JoyButtonUp {
            timestamp,
            which,
            button_idx,
        } => (),

        SdlEvent::JoyDeviceAdded { timestamp, which } => (),

        SdlEvent::JoyDeviceRemoved { timestamp, which } => (),

        SdlEvent::ControllerAxisMotion {
            timestamp,
            which,
            axis,
            value,
        } => (),

        SdlEvent::ControllerButtonDown {
            timestamp,
            which,
            button,
        } => (),

        SdlEvent::ControllerButtonUp {
            timestamp,
            which,
            button,
        } => (),

        SdlEvent::ControllerDeviceAdded { timestamp, which } => (),

        SdlEvent::ControllerDeviceRemoved { timestamp, which } => (),

        SdlEvent::ControllerDeviceRemapped { timestamp, which } => (),

        SdlEvent::ControllerTouchpadDown {
            timestamp,
            which,
            touchpad,
            finger,
            x,
            y,
            pressure,
        } => (),

        SdlEvent::ControllerTouchpadMotion {
            timestamp,
            which,
            touchpad,
            finger,
            x,
            y,
            pressure,
        } => (),

        SdlEvent::ControllerTouchpadUp {
            timestamp,
            which,
            touchpad,
            finger,
            x,
            y,
            pressure,
        } => (),

        SdlEvent::FingerDown {
            timestamp,
            touch_id,
            finger_id,
            x,
            y,
            dx,
            dy,
            pressure,
            window_id,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let Some(entity) = windows.get_window_entity(0) else {
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::TouchInput(
                    convert_sdl_touch_event(
                        bevy_input::touch::TouchPhase::Started,
                        finger_id,
                        x,
                        y,
                        pressure,
                        entity,
                    ),
                ));
            });
            */
        }

        SdlEvent::FingerUp {
            timestamp,
            touch_id,
            finger_id,
            x,
            y,
            dx,
            dy,
            pressure,
            window_id,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let Some(entity) = windows.get_window_entity(0) else {
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::TouchInput(
                    convert_sdl_touch_event(
                        bevy_input::touch::TouchPhase::Ended,
                        finger_id,
                        x,
                        y,
                        pressure,
                        entity,
                    ),
                ));
            });
            */
        }

        SdlEvent::FingerMotion {
            timestamp,
            touch_id,
            finger_id,
            x,
            y,
            dx,
            dy,
            pressure,
            window_id,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let Some(entity) = windows.get_window_entity(0) else {
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::TouchInput(
                    convert_sdl_touch_event(
                        bevy_input::touch::TouchPhase::Moved,
                        finger_id,
                        x,
                        y,
                        pressure,
                        entity,
                    ),
                ));
            });
            */
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
            timestamp,
            window_id,
            filename,
        } => {
            /*
            SDL_WINDOWS.with_borrow(|windows| {
                let Some(entity) = windows.get_window_entity(window_id) else {
                    return;
                };
                bevy_window_events.push(bevy_window::WindowEvent::FileDragAndDrop(
                    bevy_window::FileDragAndDrop::DroppedFile {
                        window: entity,
                        path_buf: std::path::PathBuf::from(filename),
                    },
                ));
            });
            */
        }

        SdlEvent::DropText {
            timestamp,
            window_id,
            filename,
        } => (),

        SdlEvent::DropBegin {
            timestamp,
            window_id,
        } => (),

        SdlEvent::DropComplete {
            timestamp,
            window_id,
        } => (),

        SdlEvent::AudioDeviceAdded {
            timestamp,
            which,
            iscapture,
        } => (),

        SdlEvent::AudioDeviceRemoved {
            timestamp,
            which,
            iscapture,
        } => (),

        SdlEvent::PenProximityIn {
            timestamp,
            which,
            window,
        } => (),

        SdlEvent::PenProximityOut {
            timestamp,
            which,
            window,
        } => (),

        SdlEvent::PenDown {
            timestamp,
            which,
            window,
            x,
            y,
            eraser,
        } => (),

        SdlEvent::PenUp {
            timestamp,
            which,
            window,
            x,
            y,
            eraser,
        } => (),

        SdlEvent::PenMotion {
            timestamp,
            which,
            window,
            x,
            y,
        } => (),

        SdlEvent::PenButtonUp {
            timestamp,
            which,
            window,
            x,
            y,
            button,
        } => (),

        SdlEvent::PenButtonDown {
            timestamp,
            which,
            window,
            x,
            y,
            button,
        } => (),

        SdlEvent::PenAxis {
            timestamp,
            which,
            window,
            x,
            y,
            axis,
            value,
        } => (),

        SdlEvent::RenderTargetsReset { timestamp } => (),

        SdlEvent::RenderDeviceReset { timestamp } => (),

        SdlEvent::User {
            timestamp,
            window_id,
            type_,
            code,
            data1,
            data2,
        } => (),

        SdlEvent::Unknown { timestamp, type_ } => (),

        SdlEvent::Display {
            timestamp,
            display,
            display_event,
        } => (),
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
