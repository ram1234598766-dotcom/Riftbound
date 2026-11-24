//! Deterministic lightweight ground-vehicle handling for the open-world driving RPG.

use crate::Vec3;

/// Surface response used by the vehicle simulation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VehicleSurface {
    /// High grip and low rolling resistance.
    Asphalt,
    /// Loose aggregate with reduced grip.
    Gravel,
    /// Deep, energy-absorbing surface.
    Mud,
    /// Loose granular surface.
    Sand,
    /// Very low grip surface.
    Snow,
    /// Uneven hard surface with moderate grip.
    Rock,
}

impl VehicleSurface {
    fn grip(self) -> f32 {
        match self {
            Self::Asphalt => 1.0,
            Self::Gravel => 0.78,
            Self::Mud => 0.48,
            Self::Sand => 0.62,
            Self::Snow => 0.38,
            Self::Rock => 0.72,
        }
    }

    fn speed_factor(self) -> f32 {
        match self {
            Self::Asphalt => 1.0,
            Self::Gravel => 0.88,
            Self::Mud => 0.58,
            Self::Sand => 0.7,
            Self::Snow => 0.66,
            Self::Rock => 0.76,
        }
    }

    fn rolling_resistance(self) -> f32 {
        match self {
            Self::Asphalt => 0.8,
            Self::Gravel => 1.5,
            Self::Mud => 3.2,
            Self::Sand => 2.7,
            Self::Snow => 2.2,
            Self::Rock => 1.8,
        }
    }
}

/// Tunable, unit-agnostic vehicle handling values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleConfig {
    /// Forward speed limit in world units per second.
    pub max_forward_speed: f32,
    /// Reverse speed limit in world units per second.
    pub max_reverse_speed: f32,
    /// Longitudinal acceleration in world units per second squared.
    pub acceleration: f32,
    /// Braking deceleration in world units per second squared.
    pub braking: f32,
    /// Steering yaw rate at full speed, in radians per second.
    pub steering_rate: f32,
    /// Baseline passive drag.
    pub drag: f32,
}

impl Default for VehicleConfig {
    fn default() -> Self {
        Self {
            max_forward_speed: 24.0,
            max_reverse_speed: 7.0,
            acceleration: 10.0,
            braking: 18.0,
            steering_rate: 1.8,
            drag: 0.16,
        }
    }
}

/// Normalized player input for a vehicle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleInput {
    /// -1 reverse, 0 coast, +1 accelerate.
    pub throttle: f32,
    /// 0 released, 1 fully applied.
    pub brake: f32,
    /// -1 left, 0 straight, +1 right.
    pub steering: f32,
}

/// Vehicle state retained between fixed simulation steps.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleState {
    /// World-space vehicle origin.
    pub position: Vec3,
    /// Yaw in radians; zero faces toward negative Z.
    pub heading: f32,
    /// Signed forward speed in world units per second.
    pub speed: f32,
}

impl VehicleState {
    /// Advances the vehicle without allocating. Delta time is bounded to avoid
    /// large jumps after stalls or application suspension.
    pub fn update(
        &mut self,
        input: VehicleInput,
        surface: VehicleSurface,
        config: VehicleConfig,
        delta_seconds: f32,
    ) {
        if !delta_seconds.is_finite()
            || delta_seconds <= 0.0
            || !input.throttle.is_finite()
            || !input.brake.is_finite()
            || !input.steering.is_finite()
            || !self.position.x.is_finite()
            || !self.position.y.is_finite()
            || !self.position.z.is_finite()
            || !self.heading.is_finite()
            || !self.speed.is_finite()
            || !config.max_forward_speed.is_finite()
            || !config.max_reverse_speed.is_finite()
            || !config.acceleration.is_finite()
            || !config.braking.is_finite()
            || !config.steering_rate.is_finite()
            || !config.drag.is_finite()
            || config.max_forward_speed <= 0.0
            || config.max_reverse_speed <= 0.0
            || config.acceleration < 0.0
            || config.braking < 0.0
            || config.steering_rate < 0.0
            || config.drag < 0.0
        {
            return;
        }

        let dt = delta_seconds.min(0.1);
        let throttle = input.throttle.clamp(-1.0, 1.0);
        let brake = input.brake.clamp(0.0, 1.0);
        let steering = input.steering.clamp(-1.0, 1.0);
        let grip = surface.grip();
        self.speed += throttle * config.acceleration * grip * dt;
        if brake > 0.0 {
            let reduction = config.braking * brake * dt;
            self.speed = if self.speed.abs() <= reduction {
                0.0
            } else {
                self.speed - self.speed.signum() * reduction
            };
        }
        let drag = (config.drag + surface.rolling_resistance() * 0.025) * dt;
        self.speed *= (1.0 - drag).max(0.0);
        self.speed = self.speed.clamp(
            -config.max_reverse_speed * surface.speed_factor(),
            config.max_forward_speed * surface.speed_factor(),
        );

        let speed_limit = if self.speed < 0.0 {
            config.max_reverse_speed
        } else {
            config.max_forward_speed
        };
        let speed_fraction = (self.speed.abs() / speed_limit).clamp(0.0, 1.0);
        self.heading +=
            steering * config.steering_rate * grip * speed_fraction * self.speed.signum() * dt;
        self.position.x += self.heading.sin() * self.speed * dt;
        self.position.z -= self.heading.cos() * self.speed * dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_accelerates_and_moves_forward() {
        let mut vehicle = VehicleState::default();
        let input = VehicleInput {
            throttle: 1.0,
            ..VehicleInput::default()
        };
        for _ in 0..20 {
            vehicle.update(
                input,
                VehicleSurface::Asphalt,
                VehicleConfig::default(),
                0.05,
            );
        }
        assert!(vehicle.speed > 0.0);
        assert!(vehicle.position.z < 0.0);
    }

    #[test]
    fn braking_reduces_speed_without_reversing() {
        let mut vehicle = VehicleState {
            speed: 8.0,
            ..VehicleState::default()
        };
        vehicle.update(
            VehicleInput {
                brake: 1.0,
                ..VehicleInput::default()
            },
            VehicleSurface::Asphalt,
            VehicleConfig::default(),
            0.1,
        );
        assert!(vehicle.speed >= 0.0 && vehicle.speed < 8.0);
    }

    #[test]
    fn loose_surfaces_reduce_acceleration_and_top_speed() {
        let mut road = VehicleState::default();
        let mut mud = VehicleState::default();
        let input = VehicleInput {
            throttle: 1.0,
            ..VehicleInput::default()
        };
        for _ in 0..20 {
            road.update(
                input,
                VehicleSurface::Asphalt,
                VehicleConfig::default(),
                0.05,
            );
            mud.update(input, VehicleSurface::Mud, VehicleConfig::default(), 0.05);
        }
        assert!(road.speed > mud.speed);
    }

    #[test]
    fn invalid_delta_and_non_finite_input_are_ignored() {
        let mut vehicle = VehicleState::default();
        vehicle.update(
            VehicleInput {
                throttle: f32::NAN,
                ..VehicleInput::default()
            },
            VehicleSurface::Asphalt,
            VehicleConfig::default(),
            0.1,
        );
        assert_eq!(vehicle, VehicleState::default());
        vehicle.update(
            VehicleInput::default(),
            VehicleSurface::Asphalt,
            VehicleConfig::default(),
            f32::NAN,
        );
        assert_eq!(vehicle, VehicleState::default());
    }


    #[test]
    fn reverse_steering_uses_reverse_speed_scale() {
        let mut vehicle = VehicleState {
            speed: -7.0,
            ..VehicleState::default()
        };
        vehicle.update(
            VehicleInput {
                steering: 1.0,
                ..VehicleInput::default()
            },
            VehicleSurface::Asphalt,
            VehicleConfig::default(),
            0.1,
        );
        let expected = VehicleConfig::default().steering_rate * 0.1;
        assert!((vehicle.heading - expected).abs() < 0.0001);
    }

    #[test]
    fn reverse_speed_is_limited_independently_from_forward_speed() {
        let mut vehicle = VehicleState::default();
        for _ in 0..200 {
            vehicle.update(
                VehicleInput {
                    throttle: -1.0,
                    ..VehicleInput::default()
                },
                VehicleSurface::Asphalt,
                VehicleConfig::default(),
                0.1,
            );
        }
        assert!(vehicle.speed >= -VehicleConfig::default().max_reverse_speed);
        assert!(vehicle.speed < 0.0);
    }

    #[test]
    fn steering_changes_heading_when_moving() {
        let mut vehicle = VehicleState {
            speed: 10.0,
            ..VehicleState::default()
        };
        vehicle.update(
            VehicleInput {
                steering: 1.0,
                ..VehicleInput::default()
            },
            VehicleSurface::Asphalt,
            VehicleConfig::default(),
            0.1,
        );
        assert!(vehicle.heading > 0.0);
    }
}
