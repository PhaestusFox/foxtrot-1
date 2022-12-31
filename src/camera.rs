use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy_rapier3d::na::{Matrix3, Vector3};

use crate::actions::Actions;
use crate::player::Player;
use crate::GameState;
use bevy_rapier3d::prelude::*;
use smooth_bevy_cameras::{LookTransform, LookTransformBundle, LookTransformPlugin, Smoother};

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_startup_system(setup_camera)
            // Enables the system that synchronizes your `Transform`s and `LookTransform`s.
            .add_plugin(LookTransformPlugin)
            .add_system_set(
                SystemSet::on_update(GameState::Playing)
                    .with_system(follow_player.label("follow_player"))
                    .with_system(handle_camera_controls.after("follow_player"))
                    .with_system(cursor_grab_system),
            );
    }
}

fn setup_camera(mut commands: Commands) {
    let eye = Vec3::default();
    let target = Vec3::default();
    commands.spawn((
        LookTransformBundle {
            transform: LookTransform::new(eye, target),
            smoother: Smoother::new(0.9), // Value between 0.0 and 1.0, higher is smoother.
        },
        Camera3dBundle::default(),
        Name::new("Camera"),
    ));
}

fn follow_player(
    player_query: Query<(&KinematicCharacterControllerOutput, &Transform), With<Player>>,
    mut camera_query: Query<&mut LookTransform>,
) {
    let (output, transform) = match player_query.iter().next() {
        Some(player) => player,
        None => return,
    };
    let mut camera = match camera_query.iter_mut().next() {
        Some(transform) => transform,
        None => return,
    };

    camera.eye += output.effective_translation;
    camera.target = transform.translation;
}

fn handle_camera_controls(mut camera_query: Query<&mut LookTransform>, actions: Res<Actions>) {
    let max_distance = 6.0;
    let mouse_sensitivity = 0.01;
    let mut camera = match camera_query.iter_mut().next() {
        Some(transform) => transform,
        None => return,
    };
    let camera_movement = match actions.camera_movement {
        Some(vector) => vector,
        None => return,
    };

    let mut direction = camera.look_direction().unwrap_or(Vect::Z);

    let x_angle = mouse_sensitivity * camera_movement.x;
    let y_angle = mouse_sensitivity * camera_movement.y;

    // See https://en.wikipedia.org/wiki/Rotation_matrix#Basic_rotations
    let y_axis_rotation_matrix = get_y_axis_rotation_matrix(x_angle);
    let x_axis_rotation_matrix = get_x_axis_rotation_matrix(y_angle);

    direction = (y_axis_rotation_matrix * x_axis_rotation_matrix * Vector3::from(direction)).into();
    camera.eye = camera.target - direction * max_distance;
}

fn get_x_axis_rotation_matrix(angle: f32) -> Matrix3<f32> {
    Matrix3::from_row_iterator(
        #[cfg_attr(rustfmt, rustfmt::skip)]
        [
            1., 0., 0.,
            0., angle.cos(), -angle.sin(),
            0., angle.sin(), angle.cos(),
        ].into_iter(),
    )
}

fn get_y_axis_rotation_matrix(angle: f32) -> Matrix3<f32> {
    Matrix3::from_row_iterator(
        #[cfg_attr(rustfmt, rustfmt::skip)]
        [
            angle.cos(), 0., -angle.sin(),
            0., 1., 0.,
            angle.sin(), 0., angle.cos(),
        ].into_iter(),
    )
}

fn cursor_grab_system(mut windows: ResMut<Windows>, key: Res<Input<KeyCode>>) {
    let window = windows.get_primary_mut().unwrap();

    if key.just_pressed(KeyCode::Escape) {
        if matches!(window.cursor_grab_mode(), CursorGrabMode::None) {
            // if you want to use the cursor, but not let it leave the window,
            // use `Confined` mode:
            window.set_cursor_grab_mode(CursorGrabMode::Confined);

            // for a game that doesn't use the cursor (like a shooter):
            // use `Locked` mode to keep the cursor in one place
            window.set_cursor_grab_mode(CursorGrabMode::Locked);
            // also hide the cursor
            window.set_cursor_visibility(false);
        } else {
            window.set_cursor_grab_mode(CursorGrabMode::None);
            window.set_cursor_visibility(true);
        }
    }
}
