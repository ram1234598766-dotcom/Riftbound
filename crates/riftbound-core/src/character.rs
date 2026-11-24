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

/// Converts logical input into smooth horizontal movement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterController {
    velocity: Vec3,
    config: MovementConfig,
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
            config,
        }
    }

    /// Current horizontal velocity.
    #[must_use]
    pub const fn velocity(&self) -> Vec3 {
        self.velocity
    }

    /// Advances movement by a bounded time step and updates the transform.
    pub fn update(
        &mut self,
        transform: &mut Transform3,
        input: &InputState,
        delta_seconds: f32,
    ) {
        if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
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

        let has_input = magnitude_squared > f32::EPSILON;
        let target_x = x * self.config.max_speed;
        let target_z = z * self.config.max_speed;
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
}
