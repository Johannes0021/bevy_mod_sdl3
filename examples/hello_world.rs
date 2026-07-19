use bevy::{prelude::*, winit::WinitPlugin};
use bevy_mod_sdl3::Sdl3Plugin;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .build()
                .add_after::<WinitPlugin>(Sdl3Plugin)
                .disable::<WinitPlugin>(),
        )
        .add_systems(Startup, setup)
        .add_systems(Update, (sprite_rotate_system, move_quad_to_input))
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn((Camera2d, Msaa::Off));

    commands.spawn(Sprite {
        color: Color::srgb(0.0, 1.0, 0.0),
        custom_size: Some(Vec2 { x: 21.0, y: 21.0 }),
        ..default()
    });
}

fn sprite_rotate_system(
    time: Res<Time>,
    mut sprite_transforms: Query<&mut Transform, With<Sprite>>,
) {
    for mut transform in &mut sprite_transforms {
        let rotation_speed = -1.0;
        transform.rotation *= Quat::from_rotation_z(rotation_speed * time.delta_secs());
    }
}

fn move_quad_to_input(
    touches: Res<Touches>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform)>,
    mut sprite_transform: Single<&mut Transform, With<Sprite>>,
) {
    let (camera, camera_transform) = *camera;

    if let Some(touch) = touches.iter().next()
        && let Ok(world_position) = camera.viewport_to_world_2d(camera_transform, touch.position())
    {
        sprite_transform.translation = world_position.extend(0.0);
    } else if let Some(cursor_pos) = window.cursor_position()
        && let Ok(world_position) = camera.viewport_to_world_2d(camera_transform, cursor_pos)
    {
        sprite_transform.translation = world_position.extend(0.0);
    }
}
