use bevy_ecs::{
    change_detection::{NonSendMut, Res},
    entity::Entity,
    query::{With, Without},
    system::{Commands, Single, SystemParamItem},
    world::World,
};
use bevy_window::{PrimaryWindow, RawHandleWrapper, Window};

use crate::{context::SdlContext, monitors::SdlMonitors};

pub(crate) fn trigger_surface_destruction(world: &mut World) {
    // Remove the `RawHandleWrapper` from the primary window.
    // This will trigger the surface destruction.

    let mut primary_window = world.query_filtered::<Entity, With<PrimaryWindow>>();
    if let Ok(primary_window_entity) = primary_window.single(world) {
        world
            .entity_mut(primary_window_entity)
            .remove::<RawHandleWrapper>();
    }
}

pub(crate) type EnsureSurfaceExistsParams<'w, 's> = (
    Commands<'w, 's>,
    NonSendMut<'w, SdlContext>,
    Res<'w, SdlMonitors>,
    Single<
        'w,
        's,
        (
            Entity,
            &'static Window,
            //&'static CursorOptions,
        ),
        (
            //With<CachedWindow>,
            Without<RawHandleWrapper>,
        ),
    >,
);

pub(crate) fn ensure_surface_exists(
    (mut commands, mut sdl_context, sdl_monitors, window): SystemParamItem<
        EnsureSurfaceExistsParams,
    >,
) {
    // Get windows that are cached but without raw handles.
    // Those window were already created, but got their handle wrapper removed when the app was
    // suspended.

    let (entity, window) = *window;
    let sdl_window = sdl_context.create_window(entity, window, &sdl_monitors);
    if let Ok(handle_wrapper) = RawHandleWrapper::new(sdl_window) {
        commands.entity(entity).insert(handle_wrapper.clone());
    }
}
