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

/// Stores the most recent traversable vehicle state for fall recovery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleRecovery {
    last_safe_state: VehicleState,
}

impl Default for VehicleRecovery {
    fn default() -> Self {
        Self::new(VehicleState::default())
    }
}

impl VehicleRecovery {
    /// Creates recovery state from the vehicle's initial spawn.
    #[must_use]
    pub const fn new(initial_state: VehicleState) -> Self {
        Self {
            last_safe_state: initial_state,
        }
    }

    /// Records a state only when it is within the allowed clearance above ground.
    pub fn record_safe_state(
        &mut self,
        state: VehicleState,
        ground_height: f32,
        max_clearance: f32,
    ) -> bool {
        let clearance = state.position.y - ground_height;
        if !state.position.x.is_finite()
            || !state.position.y.is_finite()
            || !state.position.z.is_finite()
            || !state.heading.is_finite()
            || !state.speed.is_finite()
            || !ground_height.is_finite()
            || !max_clearance.is_finite()
            || max_clearance < 0.0
            || clearance < 0.0
            || clearance > max_clearance
        {
            return false;
        }
        self.last_safe_state = state;
        true
    }

    /// Restores the last safe location below the recovery plane or on invalid height.
    pub fn recover_if_fallen(&self, state: &mut VehicleState, minimum_height: f32) -> bool {
        if !minimum_height.is_finite()
            || (state.position.y.is_finite() && state.position.y >= minimum_height)
        {
            return false;
        }
        *state = self.last_safe_state;
        state.speed = 0.0;
        true
    }

    /// Returns the last accepted safe state.
    #[must_use]
    pub const fn last_safe_state(&self) -> VehicleState {
        self.last_safe_state
    }
}

/// Tunable four-corner spring approximation for visual chassis response.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SuspensionConfig {
    /// Maximum spring compression in world units.
    pub max_travel: f32,
    /// Spring acceleration per unit of compression error.
    pub spring_strength: f32,
    /// Velocity damping applied to each spring.
    pub damping: f32,
    /// Rest distance between chassis reference and wheel contact.
    pub rest_length: f32,
    /// Distance between left and right wheel contact points.
    pub track_width: f32,
    /// Distance between front and rear wheel contact points.
    pub wheelbase: f32,
}

impl Default for SuspensionConfig {
    fn default() -> Self {
        Self {
            max_travel: 0.42,
            spring_strength: 34.0,
            damping: 8.5,
            rest_length: 0.58,
            track_width: 1.55,
            wheelbase: 2.65,
        }
    }
}

/// Chassis response derived from the four suspension corners.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SuspensionPose {
    /// Mean spring compression across all wheels.
    pub average_compression: f32,
    /// Front-to-rear chassis angle in radians.
    pub pitch: f32,
    /// Left-to-right chassis angle in radians.
    pub roll: f32,
}

/// Allocation-free suspension state; corner order is front-left, front-right,
/// rear-left, rear-right.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SuspensionState {
    compression: [f32; 4],
    compression_velocity: [f32; 4],
}

impl SuspensionState {
    /// Advances the spring approximation from four sampled ground heights.
    /// Invalid inputs are ignored and the timestep is capped at 100 ms.
    pub fn update(
        &mut self,
        wheel_ground_heights: [f32; 4],
        chassis_height: f32,
        config: SuspensionConfig,
        delta_seconds: f32,
    ) -> SuspensionPose {
        if !delta_seconds.is_finite()
            || delta_seconds <= 0.0
            || !chassis_height.is_finite()
            || wheel_ground_heights
                .iter()
                .any(|height| !height.is_finite())
            || !config.max_travel.is_finite()
            || !config.spring_strength.is_finite()
            || !config.damping.is_finite()
            || !config.rest_length.is_finite()
            || !config.track_width.is_finite()
            || !config.wheelbase.is_finite()
            || config.max_travel <= 0.0
            || config.spring_strength < 0.0
            || config.damping < 0.0
            || config.rest_length < 0.0
            || config.track_width <= 0.0
            || config.wheelbase <= 0.0
        {
            return self.pose(config);
        }

        let dt = delta_seconds.min(0.1);
        for (index, ground_height) in wheel_ground_heights.iter().enumerate() {
            let target =
                (ground_height + config.rest_length - chassis_height).clamp(0.0, config.max_travel);
            let acceleration = (target - self.compression[index]) * config.spring_strength
                - self.compression_velocity[index] * config.damping;
            self.compression_velocity[index] += acceleration * dt;
            self.compression[index] = (self.compression[index]
                + self.compression_velocity[index] * dt)
                .clamp(0.0, config.max_travel);
            if self.compression[index] <= f32::EPSILON
                || self.compression[index] >= config.max_travel - f32::EPSILON
            {
                self.compression_velocity[index] = 0.0;
            }
        }
        self.pose(config)
    }

    /// Returns the current visual chassis response without advancing simulation.
    #[must_use]
    pub fn pose(&self, config: SuspensionConfig) -> SuspensionPose {
        let front = f32::midpoint(self.compression[0], self.compression[1]);
        let rear = f32::midpoint(self.compression[2], self.compression[3]);
        let left = f32::midpoint(self.compression[0], self.compression[2]);
        let right = f32::midpoint(self.compression[1], self.compression[3]);
        SuspensionPose {
            average_compression: self.compression.iter().sum::<f32>() * 0.25,
            pitch: ((front - rear) / config.wheelbase.max(f32::EPSILON)).atan(),
            roll: ((left - right) / config.track_width.max(f32::EPSILON)).atan(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vehicle_recovery_restores_last_safe_state_and_resets_speed() {
        let initial = VehicleState {
            position: Vec3::new(1.0, 0.6, 2.0),
            heading: 0.4,
            speed: 5.0,
        };
        let mut recovery = VehicleRecovery::new(initial);
        let safe = VehicleState {
            position: Vec3::new(4.0, 0.8, 6.0),
            heading: 1.2,
            speed: 9.0,
        };
        assert!(recovery.record_safe_state(safe, 0.0, 1.0));
        let mut fallen = VehicleState {
            position: Vec3::new(4.0, -5.0, 6.0),
            heading: 0.0,
            speed: 12.0,
        };
        assert!(recovery.recover_if_fallen(&mut fallen, -3.0));
        assert_eq!(fallen.position, safe.position);
        assert_eq!(fallen.heading, safe.heading);
        assert_eq!(fallen.speed, 0.0);
    }

    #[test]
    fn vehicle_recovery_rejects_unsafe_samples_and_preserves_valid_state() {
        let initial = VehicleState::default();
        let mut recovery = VehicleRecovery::new(initial);
        let unsafe_state = VehicleState {
            position: Vec3::new(0.0, 8.0, 0.0),
            ..VehicleState::default()
        };
        assert!(!recovery.record_safe_state(unsafe_state, 0.0, 2.0));
        let mut valid = VehicleState {
            position: Vec3::new(0.0, 0.5, 0.0),
            speed: 3.0,
            ..VehicleState::default()
        };
        assert!(!recovery.recover_if_fallen(&mut valid, -3.0));
        assert_eq!(valid.speed, 3.0);
        assert_eq!(recovery.last_safe_state(), initial);
    }


    #[test]
    fn suspension_compresses_over_a_bump_and_returns_a_finite_pose() {
        let mut suspension = SuspensionState::default();
        let config = SuspensionConfig::default();
        let mut pose = SuspensionPose::default();
        for _ in 0..20 {
            pose = suspension.update([0.2, 0.0, 0.0, 0.0], 0.58, config, 0.05);
        }
        assert!(pose.average_compression > 0.0);
        assert!(pose.roll.is_finite() && pose.pitch.is_finite());
    }

    #[test]
    fn suspension_ignores_invalid_samples() {
        let mut suspension = SuspensionState::default();
        let before = suspension;
        let pose = suspension.update(
            [f32::NAN, 0.0, 0.0, 0.0],
            0.58,
            SuspensionConfig::default(),
            0.05,
        );
        assert_eq!(suspension, before);
        assert_eq!(pose, SuspensionPose::default());
    }

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
        let config = VehicleConfig::default();
        let expected =
            -config.steering_rate * (vehicle.speed.abs() / config.max_reverse_speed) * 0.1;
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
