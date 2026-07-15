use std::{collections::HashMap, marker::PhantomData};

use bevy_derive::{Deref, DerefMut};
use bevy_ecs::entity::{Entity, EntityHashMap};
use bevy_window::{Window, WindowWrapper};
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};

use sdl3::{VideoSubsystem as SdlVideoSubsystem, video::Window as SdlWindow};

use crate::monitors::SdlMonitors;

//==================================================================================================
// WindowId
//==================================================================================================

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct WindowId(pub u32);

impl From<u32> for WindowId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

//==================================================================================================
// SdlWindow
//==================================================================================================

#[derive(Deref, DerefMut)]
pub(crate) struct SdlWindowWrapper(pub(crate) SdlWindow);

// TODO: I don't know if this is safe...
//
// We need this because WindowWrapper<W> expects W to be Send + Sync.
// I think Bevy needs this for rendering and to satisfy its lifetime requirements.
//
// Access to the wrapped sdl window must remain private to this crate to ensure that the window is
// only accessed from the thread on which it was created.
unsafe impl Send for SdlWindowWrapper {}
unsafe impl Sync for SdlWindowWrapper {}

impl HasWindowHandle for SdlWindowWrapper {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl HasDisplayHandle for SdlWindowWrapper {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

//==================================================================================================
// SdlWindows
//==================================================================================================

#[derive(Default)]
pub(crate) struct SdlWindows {
    windows: HashMap<WindowId, WindowWrapper<SdlWindowWrapper>>,
    entity_to_sdl: EntityHashMap<WindowId>,
    sdl_to_entity: HashMap<WindowId, Entity>,
    _not_send_sync: PhantomData<*const ()>,
}

impl SdlWindows {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create(
        &mut self,
        video: &SdlVideoSubsystem,
        entity: Entity,
        bevy_window: &Window,
        sdl_monitors: &SdlMonitors,
    ) -> &WindowWrapper<SdlWindowWrapper> {
        let sdl_window = video
            .window(
                bevy_window.name.as_ref().unwrap_or(&"Bevy".to_string()),
                bevy_window.width() as u32,
                bevy_window.height() as u32,
            )
            .position_centered()
            .resizable()
            .metal_view()
            .build()
            .map_err(|e| e.to_string())
            .unwrap();

        let id = WindowId(sdl_window.id());

        self.windows
            .insert(id, WindowWrapper::new(SdlWindowWrapper(sdl_window)));
        self.entity_to_sdl.insert(entity, id);
        self.sdl_to_entity.insert(id, entity);

        self.windows.get(&id).unwrap()
    }

    pub fn destroy(&mut self, entity: Entity) -> Option<WindowWrapper<SdlWindowWrapper>> {
        let id = self.entity_to_sdl.remove(&entity)?;
        self.sdl_to_entity.remove(&id);
        self.windows.remove(&id)
    }

    pub fn get(&self, entity: Entity) -> Option<&WindowWrapper<SdlWindowWrapper>> {
        let id = self.entity_to_sdl.get(&entity)?;
        self.windows.get(id)
    }

    pub fn get_entity(&self, window_id: WindowId) -> Option<Entity> {
        self.sdl_to_entity.get(&window_id).copied()
    }
}
