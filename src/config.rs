use bevy_ecs::resource::Resource;
use std::time::Duration;

//==================================================================================================
// SdlSettings
//==================================================================================================

#[derive(Debug, Resource, Clone)]
pub struct SdlSettings {
    pub focused: FrameRate,
    pub unfocused: FrameRate,
    pub suspended: FrameRate,
}

impl Default for SdlSettings {
    fn default() -> Self {
        Self {
            focused: FrameRate::Uncapped,
            unfocused: FrameRate::Limited {
                frame_time: Duration::from_secs_f64(1.0 / 60.0),
            },
            suspended: FrameRate::Limited {
                frame_time: Duration::from_millis(320),
            },
        }
    }
}

//==================================================================================================
// FrameRate
//==================================================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRate {
    Uncapped,
    Limited { frame_time: Duration },
}
