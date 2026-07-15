use bevy_ecs::{
    change_detection::{NonSend, ResMut},
    entity::Entity,
    resource::Resource,
    system::{Commands, Query, SystemParamItem},
};
use bevy_log::{error_once, info};
use bevy_math::{IVec2, UVec2};
use bevy_window::{Monitor, PrimaryMonitor, VideoMode};

use sdl3::video::{Display, DisplayMode};

use crate::context::SdlContext;

#[derive(Resource, Debug, Default)]
pub struct SdlMonitors {
    displays: Vec<(Display, Entity)>,
}

impl SdlMonitors {
    pub fn find_entity(&self, entity: Entity) -> Option<&Display> {
        self.displays
            .iter()
            .find(|(_, e)| *e == entity)
            .map(|(displays, _)| displays)
    }
}

pub fn get_refresh_rate_millihertz(mode: &DisplayMode) -> Option<u32> {
    if mode.refresh_rate_numerator > 0 && mode.refresh_rate_denominator > 0 {
        let numerator = mode.refresh_rate_numerator as u128;
        let denominator = mode.refresh_rate_denominator as u128;

        u32::try_from((numerator * 1000) / denominator).ok()
    } else if mode.refresh_rate.is_finite() && mode.refresh_rate > 0.0 {
        u32::try_from((mode.refresh_rate * 1000.0) as u64).ok()
    } else {
        None
    }
}

pub type SyncMonitorsParams<'w, 's> = (
    Commands<'w, 's>,
    NonSend<'w, SdlContext>,
    ResMut<'w, SdlMonitors>,
    Query<'w, 's, (Entity, &'static PrimaryMonitor)>,
);

pub(crate) fn sync_monitors(
    (mut commands, sdl_context, mut sdl_monitors, old_primary_monitors): SystemParamItem<
        SyncMonitorsParams,
    >,
) {
    let primary_display = sdl_context.video.get_primary_display();
    let mut seen_displays = vec![false; sdl_monitors.displays.len()];

    // Create
    match sdl_context.video.displays() {
        Ok(displays) => {
            'outer: for display in displays {
                for (idx, (d, _)) in sdl_monitors.displays.iter().enumerate() {
                    if &display == d {
                        seen_displays[idx] = true;
                        continue 'outer;
                    }
                }

                let name = display.get_name().ok();
                let Ok(bounds) = display.get_bounds() else {
                    continue 'outer;
                };
                let scale_factor = display.get_content_scale().unwrap_or(1.0) as f64;
                let video_modes = display
                    .get_fullscreen_modes()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|mode| {
                        let refresh_rate_millihertz = get_refresh_rate_millihertz(&mode)?;

                        Some(VideoMode {
                            physical_size: UVec2::new(
                                (mode.w as f32 * mode.pixel_density) as u32,
                                (mode.h as f32 * mode.pixel_density) as u32,
                            ),
                            bit_depth: mode.format.bits_per_pixel() as u16,
                            refresh_rate_millihertz,
                        })
                    })
                    .collect();
                let refresh_rate_millihertz = display
                    .get_mode()
                    .ok()
                    .and_then(|mode| get_refresh_rate_millihertz(&mode));

                let entity = commands
                    .spawn(Monitor {
                        name,
                        physical_height: bounds.height(),
                        physical_width: bounds.width(),
                        physical_position: IVec2::new(bounds.x(), bounds.y()),
                        scale_factor,
                        video_modes,
                        refresh_rate_millihertz,
                    })
                    .id();

                if primary_display.as_ref() == Ok(&display) {
                    commands.entity(entity).insert(PrimaryMonitor);
                }

                seen_displays.push(true);
                sdl_monitors.displays.push((display, entity));
            }
        }

        Err(e) => {
            error_once!("Failed to get displays: {}", e);
        }
    }

    // Filter
    let mut idx = 0;
    sdl_monitors.displays.retain(|(_, entity)| {
        if seen_displays[idx] {
            idx += 1;
            true
        } else {
            info!("Monitor removed {}", entity);
            commands.entity(*entity).despawn();
            idx += 1;
            false
        }
    });

    // Cleanup
    let mut remove_markers = Vec::new();

    for (entity, _) in old_primary_monitors.iter() {
        let mut remove_marker = true;

        if let Some(display) = sdl_monitors.find_entity(entity)
            && primary_display.as_ref() == Ok(display)
        {
            remove_marker = false;
        }

        if remove_marker {
            remove_markers.push(entity);
        }
    }

    for entity in remove_markers {
        commands.entity(entity).remove::<PrimaryMonitor>();
    }
}
