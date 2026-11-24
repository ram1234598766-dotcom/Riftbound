//! Deterministic, frame-rate-independent character movement.

use crate::{Button, InputState, Transform3, Vec3};

/// Tunable horizontal movement parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementConfig {
    /// Maximum horizontal speed in world units per second.
    pub max_speed: f32,
    /// Horizontal acceleration in world units per second squared.
    pub acceleration: f32,
    /// Horizontal deceleration when no direction is held.
    pub friction: f32,
}

impl Default for MovementConfig {
    fn default() -> Self {
        Self {
            max_speed: 4.5,
            acceleration: 18.0,
            friction: 14.0,
        }
    }
}

/// Tunable vertical movement parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerticalMotionConfig {
    /// Downward acceleration in world units per second squared.
    pub gravity: f32,
    /// Initial upward velocity when a grounded jump begins.
    pub jump_speed: f32,
}

impl Default for VerticalMotionConfig {
    fn default() -> Self {
        Self {
            gravity: 22.0,
            jump_speed: 7.2,
        }
    }
}

/// High-level locomotion state suitable for animation selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterMovementState {
    /// No meaningful horizontal movement while grounded.
    Idle,
    /// Horizontal movement while grounded.
    Moving,
    /// Airborne and travelling upward.
    Jumping,
    /// Airborne and travelling downward.
    Falling,
}

/// Converts logical input into smooth horizontal and vertical movement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterController {
    velocity: Vec3,
    vertical_velocity: f32,
    grounded: bool,
    config: MovementConfig,
    vertical_config: VerticalMotionConfig,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self::new(MovementConfig::default())
    }
}

impl CharacterController {
    /// Creates a controller with the supplied movement configuration.
    #[must_use]
    pub const fn new(config: MovementConfig) -> Self {
        Self {
            velocity: Vec3::new(0.0, 0.0, 0.0),
            vertical_velocity: 0.0,
            grounded: false,
            config,
            vertical_config: VerticalMotionConfig {
                gravity: 22.0,
                jump_speed: 7.2,
            },
        }
    }

    /// Creates a controller with explicit horizontal and vertical movement settings.
    #[must_use]
    pub const fn with_vertical_config(
        config: MovementConfig,
        vertical_config: VerticalMotionConfig,
    ) -> Self {
        Self {
            velocity: Vec3::new(0.0, 0.0, 0.0),
            vertical_velocity: 0.0,
            grounded: false,
            config,
            vertical_config,
        }
    }

    /// Current vertical velocity.
    #[must_use]
    pub const fn vertical_velocity(&self) -> f32 {
        self.vertical_velocity
    }

    /// Whether the character is standing on the configured ground plane.
    #[must_use]
    pub const fn is_grounded(&self) -> bool {
        self.grounded
    }

    /// Advances jumping and gravity relative to the character's ground-center height.
    pub fn update_vertical(
        &mut self,
        transform: &mut Transform3,
        input: &InputState,
        delta_seconds: f32,
        ground_height: f32,
    ) {
        if !delta_seconds.is_finite()
            || delta_seconds <= 0.0
            || !ground_height.is_finite()
            || !self.vertical_config.gravity.is_finite()
            || self.vertical_config.gravity < 0.0
            || !self.vertical_config.jump_speed.is_finite()
            || self.vertical_config.jump_speed <= 0.0
        {
            return;
        }

        let delta = delta_seconds.min(0.1);
        if transform.translation.y <= ground_height + 0.0001 && self.vertical_velocity <= 0.0 {
            transform.translation.y = ground_height;
            self.vertical_velocity = 0.0;
            self.grounded = true;
        } else {
            self.grounded = false;
        }

        if self.grounded && input.was_pressed(Button::Jump) {
            self.vertical_velocity = self.vertical_config.jump_speed;
            self.grounded = false;
        }

        if !self.grounded {
            self.vertical_velocity -= self.vertical_config.gravity * delta;
            transform.translation.y += self.vertical_velocity * delta;
            if transform.translation.y <= ground_height {
                transform.translation.y = ground_height;
                self.vertical_velocity = 0.0;
                self.grounded = true;
            }
        }
    }

    /// Returns the current locomotion state for animation and gameplay systems.
    #[must_use]
    pub fn movement_state(&self) -> CharacterMovementState {
        if !self.grounded {
            if self.vertical_velocity > 0.0 {
                CharacterMovementState::Jumping
            } else {
                CharacterMovementState::Falling
            }
        } else if self.velocity.x * self.velocity.x + self.velocity.z * self.velocity.z > 0.01 {
            CharacterMovementState::Moving
        } else {
            CharacterMovementState::Idle
        }
    }

    /// Current horizontal velocity.
    #[must_use]
    pub const fn velocity(&self) -> Vec3 {
        self.velocity
    }

    /// Clears velocity along axes blocked by the collision solver.
    pub fn resolve_collision(&mut self, blocked_x: bool, blocked_z: bool) {
        if blocked_x {
            self.velocity.x = 0.0;
        }
        if blocked_z {
            self.velocity.z = 0.0;
        }
    }

    /// Advances movement relative to world north (the legacy, fixed-axis control mode).
    pub fn update(&mut self, transform: &mut Transform3, input: &InputState, delta_seconds: f32) {
        self.update_camera_relative(transform, input, delta_seconds, 0.0);
    }

    /// Advances movement relative to the horizontal direction the camera faces.
    ///
    /// A yaw of zero preserves the legacy mapping: forward moves along negative Z.
    /// Non-finite yaw values are ignored without changing controller or transform state.
    pub fn update_camera_relative(
        &mut self,
        transform: &mut Transform3,
        input: &InputState,
        delta_seconds: f32,
        camera_yaw: f32,
    ) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 || !camera_yaw.is_finite() {
            return;
        }
        let delta = delta_seconds.min(0.1);
        let mut x = axis(input, Button::MoveRight, Button::MoveLeft);
        let mut z = axis(input, Button::MoveBackward, Button::MoveForward);
        let magnitude_squared = x * x + z * z;
        if magnitude_squared > 1.0 {
            let inverse_magnitude = magnitude_squared.sqrt().recip();
            x *= inverse_magnitude;
            z *= inverse_magnitude;
        }

        let sin_yaw = camera_yaw.sin();
        let cos_yaw = camera_yaw.cos();
        let world_x = x * cos_yaw + z * sin_yaw;
        let world_z = -x * sin_yaw + z * cos_yaw;
        let has_input = magnitude_squared > f32::EPSILON;
        let speed_multiplier = if input.is_held(Button::Sprint) {
            1.6
        } else {
            1.0
        };
        let target_x = world_x * self.config.max_speed * speed_multiplier;
        let target_z = world_z * self.config.max_speed * speed_multiplier;
        let rate = if has_input {
            self.config.acceleration
        } else {
            self.config.friction
        };
        self.velocity.x = approach(self.velocity.x, target_x, rate * delta);
        self.velocity.z = approach(self.velocity.z, target_z, rate * delta);
        transform.translation.x += self.velocity.x * delta;
        transform.translation.z += self.velocity.z * delta;
    }
}

fn axis(input: &InputState, positive: Button, negative: Button) -> f32 {
    let positive = if input.is_held(positive) { 1.0 } else { 0.0 };
    let negative = if input.is_held(negative) { 1.0 } else { 0.0 };
    positive - negative
}

fn approach(current: f32, target: f32, max_delta: f32) -> f32 {
    let difference = target - current;
    if difference.abs() <= max_delta {
        target
    } else {
        current + difference.signum() * max_delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_state_tracks_idle_move_jump_and_fall() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        assert_eq!(controller.movement_state(), CharacterMovementState::Idle);

        input.set_button(Button::MoveForward, true);
        controller.update(&mut transform, &input, 1.0 / 60.0);
        assert_eq!(controller.movement_state(), CharacterMovementState::Moving);

        input.set_button(Button::MoveForward, false);
        for _ in 0..60 {
            controller.update(&mut transform, &input, 1.0 / 60.0);
        }
        input.set_button(Button::Jump, true);
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        assert_eq!(controller.movement_state(), CharacterMovementState::Jumping);
        input.begin_frame();
        for _ in 0..25 {
            controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        }
        assert_eq!(controller.movement_state(), CharacterMovementState::Falling);
    }

    #[test]
    fn camera_relative_forward_tracks_camera_yaw() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        input.set_button(Button::MoveForward, true);
        for _ in 0..30 {
            controller.update_camera_relative(
                &mut transform,
                &input,
                1.0 / 60.0,
                std::f32::consts::FRAC_PI_2,
            );
        }
        assert!(controller.velocity().x < -4.49);
        assert!(controller.velocity().z.abs() < 0.001);
        assert!(transform.translation.x < 0.0);
        assert!(transform.translation.z.abs() < 0.001);
    }

    #[test]
    fn camera_relative_diagonal_input_remains_normalized() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        input.set_button(Button::MoveForward, true);
        input.set_button(Button::MoveRight, true);
        for _ in 0..120 {
            controller.update_camera_relative(&mut transform, &input, 1.0 / 60.0, 0.73);
        }
        let velocity = controller.velocity();
        let speed = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
        assert!((speed - 4.5).abs() < 0.001);
    }

    #[test]
    fn non_finite_camera_yaw_does_not_change_movement() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let input = InputState::default();
        controller.update_camera_relative(&mut transform, &input, 1.0 / 60.0, f32::NAN);
        assert_eq!(transform, Transform3::IDENTITY);
        assert_eq!(controller.velocity(), Vec3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn movement_accelerates_and_respects_speed_limit() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        input.set_button(Button::MoveForward, true);
        for _ in 0..120 {
            controller.update(&mut transform, &input, 1.0 / 60.0);
        }
        assert!((controller.velocity().z + 4.5).abs() < 0.001);
        assert!(transform.translation.z < 0.0);
    }

    #[test]
    fn diagonal_input_is_normalized() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        input.set_button(Button::MoveForward, true);
        input.set_button(Button::MoveRight, true);
        for _ in 0..120 {
            controller.update(&mut transform, &input, 1.0 / 60.0);
        }
        let velocity = controller.velocity();
        let speed = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
        assert!((speed - 4.5).abs() < 0.001);
    }

    #[test]
    fn sprint_increases_speed_and_releasing_it_returns_to_normal_speed() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        input.set_button(Button::MoveForward, true);
        input.set_button(Button::Sprint, true);
        for _ in 0..120 {
            controller.update(&mut transform, &input, 1.0 / 60.0);
        }
        let sprint_speed = (controller.velocity().x.powi(2)
            + controller.velocity().z.powi(2))
        .sqrt();
        assert!((sprint_speed - 7.2).abs() < 0.001);

        input.set_button(Button::Sprint, false);
        for _ in 0..120 {
            controller.update(&mut transform, &input, 1.0 / 60.0);
        }
        let normal_speed = (controller.velocity().x.powi(2)
            + controller.velocity().z.powi(2))
        .sqrt();
        assert!((normal_speed - 4.5).abs() < 0.001);
    }

    #[test]
    fn releasing_input_applies_friction() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        input.set_button(Button::MoveForward, true);
        for _ in 0..30 {
            controller.update(&mut transform, &input, 1.0 / 60.0);
        }
        input.set_button(Button::MoveForward, false);
        let previous_speed = controller.velocity().z.abs();
        for _ in 0..10 {
            controller.update(&mut transform, &input, 1.0 / 60.0);
        }
        assert!(controller.velocity().z.abs() < previous_speed);
    }

    #[test]
    fn invalid_delta_does_not_change_state() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let input = InputState::default();
        controller.update(&mut transform, &input, f32::NAN);
        assert_eq!(transform, Transform3::IDENTITY);
        assert_eq!(controller.velocity(), Vec3::new(0.0, 0.0, 0.0));
    }

    #[test]
    fn jump_applies_upward_velocity_only_while_grounded() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        assert!(controller.is_grounded());

        input.set_button(Button::Jump, true);
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        assert!(!controller.is_grounded());
        assert!(controller.vertical_velocity() > 0.0);
        assert!(transform.translation.y > 0.2);

        input.begin_frame();
        let velocity = controller.vertical_velocity();
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        assert!(controller.vertical_velocity() < velocity);
    }

    #[test]
    fn jump_lands_on_ground_and_resets_vertical_velocity() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        input.set_button(Button::Jump, true);
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        input.begin_frame();

        for _ in 0..120 {
            controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        }

        assert!(controller.is_grounded());
        assert!((transform.translation.y - 0.2).abs() < 0.0001);
        assert_eq!(controller.vertical_velocity(), 0.0);
    }

    #[test]
    fn airborne_character_cannot_double_jump() {
        let mut controller = CharacterController::default();
        let mut transform = Transform3::IDENTITY;
        let mut input = InputState::default();
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        input.set_button(Button::Jump, true);
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        input.begin_frame();

        for _ in 0..5 {
            controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);
        }
        let velocity = controller.vertical_velocity();
        input.set_button(Button::Jump, false);
        input.set_button(Button::Jump, true);
        controller.update_vertical(&mut transform, &input, 1.0 / 60.0, 0.2);

        assert!(controller.vertical_velocity() < velocity);
    }
}
