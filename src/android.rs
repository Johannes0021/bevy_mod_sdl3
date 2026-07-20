use bevy_ecs::{
    change_detection::NonSend,
    entity::Entity,
    query::{With, Without},
    system::{Commands, Single, SystemParamItem},
    world::World,
};
use bevy_window::{PrimaryWindow, RawHandleWrapper, RawHandleWrapperHolder};

use crate::context::{CachedWindow, SdlContext};

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
    NonSend<'w, SdlContext>,
    Single<
        'w,
        's,
        (Entity, Option<&'static RawHandleWrapperHolder>),
        (With<CachedWindow>, Without<RawHandleWrapper>),
    >,
);

pub(crate) fn ensure_surface_exists(
    (mut commands, sdl_context, window): SystemParamItem<EnsureSurfaceExistsParams>,
) {
    // Get windows that are cached but without raw handles.
    // Those window were already created, but got their handle wrapper removed when the app was
    // suspended.

    let (window, handle_holder) = *window;

    if let Some(sdl_window) = sdl_context.get_window(window)
        && let Ok(handle_wrapper) = RawHandleWrapper::new(sdl_window)
    {
        commands.entity(window).insert(handle_wrapper.clone());
        if let Some(handle_holder) = handle_holder {
            *handle_holder.0.lock().unwrap() = Some(handle_wrapper);
        }
    }
}
