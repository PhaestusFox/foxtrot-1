use crate::player_control::actions::CameraActions;
use crate::player_control::camera::util::clamp_pitch;
use crate::util::trait_extension::Vec3Ext;
use bevy::prelude::*;
use bevy_rapier3d::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

const MIN_DISTANCE: f32 = 1e-2;
const MAX_DISTANCE: f32 = 10.0;

#[derive(Debug, Clone, PartialEq, Reflect, FromReflect, Serialize, Deserialize)]
#[reflect(Serialize, Deserialize)]
pub struct ThirdPersonCamera {
    pub eye: Transform,
    pub target: Vec3,
    pub up: Vec3,
    pub secondary_target: Option<Vec3>,
    pub distance: f32,
    last_eye: Transform,
    last_target: Vec3,
}

impl Default for ThirdPersonCamera {
    fn default() -> Self {
        Self {
            up: Vec3::Y,
            eye: default(),
            distance: MAX_DISTANCE / 2.,
            target: default(),
            last_eye: default(),
            last_target: default(),
            secondary_target: default(),
        }
    }
}

impl ThirdPersonCamera {
    pub fn forward(&self) -> Vec3 {
        self.eye.forward()
    }

    fn rotate_around_target(&mut self, yaw: f32, pitch: f32) {
        let yaw_rotation = Quat::from_axis_angle(self.up, yaw);
        let pitch_rotation = Quat::from_axis_angle(self.eye.local_x(), pitch);

        let pivot = self.target;
        let rotation = yaw_rotation * pitch_rotation;
        self.eye.rotate_around(pivot, rotation);
    }

    pub fn init_transform(&mut self, transform: Transform) {
        self.last_eye = transform;
    }

    pub fn update_transform(
        &mut self,
        dt: f32,
        camera_actions: CameraActions,
        rapier_context: &RapierContext,
        transform: Transform,
    ) -> Transform {
        self.follow_target();

        if let Some(secondary_target) = self.secondary_target {
            self.move_eye_to_align_target_with(secondary_target);
        }
        if let Some(camera_movement) = camera_actions.movement {
            let camera_movement = if self.secondary_target.is_some() {
                Vec2::new(0.0, camera_movement.y)
            } else {
                camera_movement
            };
            self.handle_camera_controls(camera_movement);
        }
        if let Some(zoom) = camera_actions.zoom {
            self.zoom(zoom);
        }
        let los_correction = self.place_eye_in_valid_position(rapier_context);
        self.get_camera_transform(dt, transform, los_correction)
    }

    fn follow_target(&mut self) {
        let target_movement = (self.target - self.last_target).collapse_approx_zero();
        self.eye.translation = self.last_eye.translation + target_movement;

        let new_target = self.target;
        if !(new_target - self.eye.translation).is_approx_zero() {
            let up = self.up;
            self.eye.look_at(new_target, up);
        }
    }

    fn handle_camera_controls(&mut self, camera_movement: Vec2) {
        let mouse_sensitivity = 3e-2;
        let camera_movement = camera_movement * mouse_sensitivity;

        let yaw = -camera_movement.x.clamp(-PI, PI);
        let pitch = self.clamp_pitch(-camera_movement.y);
        self.rotate_around_target(yaw, pitch);
    }

    fn clamp_pitch(&self, angle: f32) -> f32 {
        clamp_pitch(self.up, self.forward(), angle)
    }

    fn zoom(&mut self, zoom: f32) {
        let zoom_speed = 0.1;
        let zoom = zoom * zoom_speed;
        self.distance = (self.distance + zoom).clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    fn move_eye_to_align_target_with(&mut self, secondary_target: Vec3) {
        let target_to_secondary_target = (secondary_target - self.target).split(self.up).horizontal;
        if target_to_secondary_target.is_approx_zero() {
            return;
        }
        let target_to_secondary_target = target_to_secondary_target.normalize();
        let eye_to_target = (self.target - self.eye.translation)
            .split(self.up)
            .horizontal
            .normalize();
        let rotation = Quat::from_rotation_arc(eye_to_target, target_to_secondary_target);
        let pivot = self.target;
        self.eye.rotate_around(pivot, rotation);
    }

    fn place_eye_in_valid_position(
        &mut self,
        rapier_context: &RapierContext,
    ) -> LineOfSightCorrection {
        let line_of_sight_result = self.keep_line_of_sight(rapier_context);
        self.eye.translation = line_of_sight_result.location;
        self.last_eye = self.eye;
        self.last_target = self.target;
        line_of_sight_result.correction
    }

    fn get_camera_transform(
        &self,
        dt: f32,
        mut transform: Transform,
        line_of_sight_correction: LineOfSightCorrection,
    ) -> Transform {
        let translation_smoothing = if line_of_sight_correction == LineOfSightCorrection::Further {
            50.
        } else {
            100.
        };

        let scale = (translation_smoothing * dt).min(1.);
        transform.translation = transform.translation.lerp(self.eye.translation, scale);

        let rotation_smoothing = 45.;
        let scale = (rotation_smoothing * dt).min(1.);
        transform.rotation = transform.rotation.slerp(self.eye.rotation, scale);

        transform
    }

    pub fn keep_line_of_sight(&self, rapier_context: &RapierContext) -> LineOfSightResult {
        let origin = self.target;
        let desired_direction = self.eye.translation - self.target;
        let norm_direction = desired_direction.try_normalize().unwrap_or(Vec3::Z);

        let distance = get_raycast_distance(origin, norm_direction, rapier_context, self.distance);
        let location = origin + norm_direction * distance;
        let correction = if distance * distance < desired_direction.length_squared() - 1e-3 {
            LineOfSightCorrection::Closer
        } else {
            LineOfSightCorrection::Further
        };
        LineOfSightResult {
            location,
            correction,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineOfSightResult {
    pub location: Vec3,
    pub correction: LineOfSightCorrection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOfSightCorrection {
    Closer,
    Further,
}

pub fn get_raycast_distance(
    origin: Vec3,
    direction: Vec3,
    rapier_context: &RapierContext,
    max_distance: f32,
) -> f32 {
    let max_toi = max_distance;
    let solid = true;
    let mut filter = QueryFilter::only_fixed();
    filter.flags |= QueryFilterFlags::EXCLUDE_SENSORS;

    let min_distance_to_objects = 0.01;
    rapier_context
        .cast_ray(origin, direction, max_toi, solid, filter)
        .map(|(_entity, toi)| toi - min_distance_to_objects)
        .unwrap_or(max_distance)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn facing_secondary_target_that_is_primary_changes_nothing() {
        let mut camera = ThirdPersonCamera::default();
        let camera_transform = Transform::from_xyz(2., 0., 0.);
        let primary_target = Vec3::new(-2., 0., 0.);
        let secondary_target = Vec3::new(-2., 0., 0.);

        camera.init_transform(camera_transform);
        camera.follow_target();
        camera.target = primary_target;
        camera.move_eye_to_align_target_with(secondary_target);

        assert_nearly_eq(camera.eye.translation, camera_transform.translation);
    }

    #[test]
    fn facing_secondary_target_that_is_aligned_with_primary_changes_nothing() {
        let mut camera = ThirdPersonCamera::default();
        let camera_transform = Transform::from_xyz(2., 0., 0.);
        let primary_target = Vec3::new(-2., 0., 0.);
        let secondary_target = Vec3::new(-3., 0., 0.);

        camera.init_transform(camera_transform);
        camera.follow_target();
        camera.target = primary_target;
        camera.move_eye_to_align_target_with(secondary_target);

        assert_nearly_eq(camera.eye.translation, camera_transform.translation);
    }

    #[test]
    fn faces_secondary_target_that_is_at_right_angle_with_primary() {
        let mut camera = ThirdPersonCamera::default();
        let camera_transform = Transform::from_xyz(2., 0., 0.);
        let primary_target = Vec3::new(-2., 0., 0.);
        let secondary_target = Vec3::new(-2., 0., -2.);

        camera.init_transform(camera_transform);
        camera.follow_target();
        camera.target = primary_target;
        camera.move_eye_to_align_target_with(secondary_target);

        let expected_position = Vec3::new(-2., 0., 4.);
        assert_nearly_eq(camera.eye.translation, expected_position);
    }

    #[test]
    fn faces_secondary_target_that_is_at_right_angle_with_primary_ignoring_y() {
        let mut camera = ThirdPersonCamera::default();
        let camera_transform = Transform::from_xyz(2., 2., 0.);
        let primary_target = Vec3::new(-2., -3., 0.);
        let secondary_target = Vec3::new(-2., -1., -2.);

        camera.init_transform(camera_transform);
        camera.follow_target();
        camera.target = primary_target;
        camera.move_eye_to_align_target_with(secondary_target);

        let expected_position = Vec3::new(-2., 2., 4.);
        assert_nearly_eq(camera.eye.translation, expected_position);
    }

    fn assert_nearly_eq(actual: Vec3, expected: Vec3) {
        assert!(
            (actual - expected).length_squared() < 1e-5,
            "expected: {:?}, actual: {:?}",
            expected,
            actual
        );
    }
}
