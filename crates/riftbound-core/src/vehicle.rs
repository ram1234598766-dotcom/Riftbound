//! Deterministic lightweight ground-vehicle handling for the open-world driving RPG.

use crate::{Inventory, ItemKind, Vec3};

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

/// Filters brief surface-sampling changes so a vehicle does not flicker between
/// terrain profiles when its wheel contacts straddle a biome boundary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VehicleSurfaceTracker {
    current: VehicleSurface,
    candidate: Option<VehicleSurface>,
    candidate_frames: u8,
}

impl Default for VehicleSurfaceTracker {
    fn default() -> Self {
        Self::new(VehicleSurface::Asphalt)
    }
}

impl VehicleSurfaceTracker {
    /// Number of consecutive fixed updates required before accepting a new surface.
    pub const CONFIRMATION_FRAMES: u8 = 4;

    /// Creates a tracker using the surface at the vehicle's current location.
    #[must_use]
    pub const fn new(initial: VehicleSurface) -> Self {
        Self {
            current: initial,
            candidate: None,
            candidate_frames: 0,
        }
    }

    /// Returns the stable surface, accepting changes after repeated matching samples.
    pub fn update(&mut self, sampled: VehicleSurface) -> VehicleSurface {
        if sampled == self.current {
            self.candidate = None;
            self.candidate_frames = 0;
            return self.current;
        }

        if self.candidate == Some(sampled) {
            self.candidate_frames = self.candidate_frames.saturating_add(1);
        } else {
            self.candidate = Some(sampled);
            self.candidate_frames = 1;
        }

        if self.candidate_frames >= Self::CONFIRMATION_FRAMES {
            self.current = sampled;
            self.candidate = None;
            self.candidate_frames = 0;
        }
        self.current
    }

    /// Returns the currently accepted terrain surface.
    #[must_use]
    pub const fn current(self) -> VehicleSurface {
        self.current
    }
}

impl VehicleSurface {
    /// Returns the normalized tire-grip multiplier used for acceleration and steering.
    #[must_use]
    pub const fn traction(self) -> f32 {
        match self {
            Self::Asphalt => 1.0,
            Self::Gravel => 0.78,
            Self::Mud => 0.48,
            Self::Sand => 0.62,
            Self::Snow => 0.38,
            Self::Rock => 0.72,
        }
    }

    /// Returns the normalized maximum-speed multiplier for this surface.
    #[must_use]
    pub const fn speed_multiplier(self) -> f32 {
        match self {
            Self::Asphalt => 1.0,
            Self::Gravel => 0.88,
            Self::Mud => 0.58,
            Self::Sand => 0.7,
            Self::Snow => 0.66,
            Self::Rock => 0.76,
        }
    }

    /// Returns the rolling-resistance coefficient used by passive vehicle drag.
    #[must_use]
    pub const fn rolling_resistance(self) -> f32 {
        match self {
            Self::Asphalt => 0.8,
            Self::Gravel => 1.5,
            Self::Mud => 3.2,
            Self::Sand => 2.7,
            Self::Snow => 2.2,
            Self::Rock => 1.8,
        }
    }

    fn grip(self) -> f32 {
        self.traction()
    }

    fn speed_factor(self) -> f32 {
        self.speed_multiplier()
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

/** Persistent engine tuning purchased with recovered salvage. */
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct VehicleUpgrades {
    engine_level: u8,
    handling_level: u8,
}

impl VehicleUpgrades {
    /// Restores validated upgrade tiers from a save file.
    ///
    /// Returns None when either tier exceeds the supported maximum.
    #[must_use]
    pub const fn from_levels(engine_level: u8, handling_level: u8) -> Option<Self> {
        if engine_level > Self::MAX_ENGINE_LEVEL || handling_level > Self::MAX_ENGINE_LEVEL {
            None
        } else {
            Some(Self {
                engine_level,
                handling_level,
            })
        }
    }

    /// Maximum engine tuning tier.
    pub const MAX_ENGINE_LEVEL: u8 = 3;
    /// Salvage required for each successive engine tier.
    pub const ENGINE_UPGRADE_COSTS: [u16; 3] = [3, 5, 8];
    /// Salvage required for successive handling and tire upgrades.
    pub const HANDLING_UPGRADE_COSTS: [u16; 3] = [2, 4, 6];

    /// Current engine tuning tier.
    #[must_use]
    pub const fn engine_level(self) -> u8 {
        self.engine_level
    }

    /// Salvage required for the next engine tier, or None at maximum level.
    #[must_use]
    pub const fn next_engine_cost(self) -> Option<u16> {
        if self.engine_level >= Self::MAX_ENGINE_LEVEL {
            None
        } else {
            Some(Self::ENGINE_UPGRADE_COSTS[self.engine_level as usize])
        }
    }

    /// Purchases the next engine tier without changing inventory on failure.
    pub fn purchase_engine_upgrade(&mut self, inventory: &mut Inventory) -> bool {
        let Some(cost) = self.next_engine_cost() else {
            return false;
        };
        if inventory.count(ItemKind::Salvage) < u32::from(cost) {
            return false;
        }
        let removed = inventory.remove(ItemKind::Salvage, cost);
        if removed != cost {
            return false;
        }
        self.engine_level += 1;
        true
    }

    /// Current handling and tire upgrade tier.
    #[must_use]
    pub const fn handling_level(self) -> u8 {
        self.handling_level
    }

    /// Salvage required for the next handling upgrade.
    #[must_use]
    pub const fn next_handling_cost(self) -> Option<u16> {
        if self.handling_level >= Self::MAX_ENGINE_LEVEL {
            None
        } else {
            Some(Self::HANDLING_UPGRADE_COSTS[self.handling_level as usize])
        }
    }

    /// Purchases handling and tire upgrades with Salvage.
    pub fn purchase_handling_upgrade(&mut self, inventory: &mut Inventory) -> bool {
        let Some(cost) = self.next_handling_cost() else {
            return false;
        };
        if inventory.count(ItemKind::Salvage) < u32::from(cost) {
            return false;
        }
        if inventory.remove(ItemKind::Salvage, cost) != cost {
            return false;
        }
        self.handling_level += 1;
        true
    }

    /// Applies installed engine and handling tiers to a base handling configuration.
    #[must_use]
    pub fn apply_to(self, mut config: VehicleConfig) -> VehicleConfig {
        let engine = self.engine_level.min(Self::MAX_ENGINE_LEVEL);
        let handling = self.handling_level.min(Self::MAX_ENGINE_LEVEL);
        config.acceleration += f32::from(engine) * 2.0;
        config.max_forward_speed += f32::from(engine) * 2.5;
        config.steering_rate += f32::from(handling) * 0.12;
        config.drag = (config.drag - f32::from(handling) * 0.025).max(0.02);
        config
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
        self.update_with_surface_response(
            input,
            surface.traction(),
            surface.speed_multiplier(),
            surface.rolling_resistance(),
            config,
            delta_seconds,
        );
    }

    /// Advances handling with blended terrain coefficients for smooth surface transitions.
    pub fn update_with_surface_response(
        &mut self,
        input: VehicleInput,
        traction: f32,
        speed_multiplier: f32,
        rolling_resistance: f32,
        config: VehicleConfig,
        delta_seconds: f32,
    ) {
        if !traction.is_finite()
            || !speed_multiplier.is_finite()
            || !rolling_resistance.is_finite()
            || traction <= 0.0
            || traction > 1.0
            || speed_multiplier <= 0.0
            || speed_multiplier > 1.0
            || rolling_resistance < 0.0
            || !delta_seconds.is_finite()
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
        let grip = traction;
        self.speed += throttle * config.acceleration * grip * dt;
        if brake > 0.0 {
            // Available tire grip limits braking force on loose and slippery terrain.
            let reduction = config.braking * brake * grip * dt;
            self.speed = if self.speed.abs() <= reduction {
                0.0
            } else {
                self.speed - self.speed.signum() * reduction
            };
        }
        let drag = (config.drag + rolling_resistance * 0.025) * dt;
        self.speed *= (1.0 - drag).max(0.0);
        self.speed = self.speed.clamp(
            -config.max_reverse_speed * speed_multiplier,
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

        // Smaller internal steps keep the spring integration stable when a frame stalls.
        let dt = delta_seconds.min(0.1);
        let substeps = (dt / (1.0 / 60.0)).ceil() as u32;
        let step_dt = dt / substeps as f32;
        for _ in 0..substeps {
            for (index, ground_height) in wheel_ground_heights.iter().enumerate() {
                let target = (ground_height + config.rest_length - chassis_height)
                    .clamp(0.0, config.max_travel);
                let acceleration = (target - self.compression[index]) * config.spring_strength
                    - self.compression_velocity[index] * config.damping;
                self.compression_velocity[index] += acceleration * step_dt;
                self.compression[index] = (self.compression[index]
                    + self.compression_velocity[index] * step_dt)
                    .clamp(0.0, config.max_travel);
                if self.compression[index] <= f32::EPSILON
                    || self.compression[index] >= config.max_travel - f32::EPSILON
                {
                    self.compression_velocity[index] = 0.0;
                }
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
    fn vehicle_surface_tracker_filters_single_frame_boundary_noise() {
        let mut tracker = VehicleSurfaceTracker::new(VehicleSurface::Asphalt);
        assert_eq!(
            tracker.update(VehicleSurface::Gravel),
            VehicleSurface::Asphalt
        );
        assert_eq!(
            tracker.update(VehicleSurface::Asphalt),
            VehicleSurface::Asphalt
        );
        assert_eq!(
            tracker.update(VehicleSurface::Gravel),
            VehicleSurface::Asphalt
        );
        assert_eq!(
            tracker.update(VehicleSurface::Gravel),
            VehicleSurface::Asphalt
        );
        assert_eq!(
            tracker.update(VehicleSurface::Gravel),
            VehicleSurface::Asphalt
        );
        assert_eq!(
            tracker.update(VehicleSurface::Gravel),
            VehicleSurface::Gravel
        );
        assert_eq!(tracker.current(), VehicleSurface::Gravel);
        assert_eq!(
            tracker.update(VehicleSurface::Asphalt),
            VehicleSurface::Gravel
        );
        assert_eq!(
            tracker.update(VehicleSurface::Gravel),
            VehicleSurface::Gravel
        );
    }
    #[test]
    fn terrain_surface_profiles_expose_distinct_driving_characteristics() {
        assert_eq!(VehicleSurface::Asphalt.traction(), 1.0);
        assert!(VehicleSurface::Asphalt.traction() > VehicleSurface::Gravel.traction());
        assert!(VehicleSurface::Gravel.traction() > VehicleSurface::Mud.traction());
        assert!(
            VehicleSurface::Mud.rolling_resistance() > VehicleSurface::Sand.rolling_resistance()
        );
        assert!(
            VehicleSurface::Snow.speed_multiplier() < VehicleSurface::Asphalt.speed_multiplier()
        );

        for surface in [
            VehicleSurface::Asphalt,
            VehicleSurface::Gravel,
            VehicleSurface::Mud,
            VehicleSurface::Sand,
            VehicleSurface::Snow,
            VehicleSurface::Rock,
        ] {
            assert!(surface.traction().is_finite());
            assert!(surface.speed_multiplier().is_finite());
            assert!(surface.rolling_resistance().is_finite());
            assert!(surface.traction() > 0.0 && surface.traction() <= 1.0);
            assert!(surface.speed_multiplier() > 0.0 && surface.speed_multiplier() <= 1.0);
            assert!(surface.rolling_resistance() >= 0.0);
        }
    }

    #[test]
    fn engine_upgrades_spend_salvage_and_improve_handling_with_a_level_cap() {
        let mut inventory = Inventory::default();
        let mut upgrades = VehicleUpgrades::default();
        assert!(!upgrades.purchase_engine_upgrade(&mut inventory));
        assert_eq!(upgrades.engine_level(), 0);
        inventory.add(ItemKind::Salvage, 16);
        assert!(upgrades.purchase_engine_upgrade(&mut inventory));
        assert_eq!(inventory.count(ItemKind::Salvage), 13);
        assert_eq!(upgrades.engine_level(), 1);
        let tuned = upgrades.apply_to(VehicleConfig::default());
        assert_eq!(tuned.acceleration, 12.0);
        assert_eq!(tuned.max_forward_speed, 26.5);
        assert!(upgrades.purchase_engine_upgrade(&mut inventory));
        assert_eq!(inventory.count(ItemKind::Salvage), 8);
        assert!(upgrades.purchase_engine_upgrade(&mut inventory));
        assert_eq!(inventory.count(ItemKind::Salvage), 0);
        assert_eq!(upgrades.engine_level(), VehicleUpgrades::MAX_ENGINE_LEVEL);
        assert_eq!(upgrades.next_engine_cost(), None);
        assert!(!upgrades.purchase_engine_upgrade(&mut inventory));
    }

    #[test]
    fn handling_upgrades_cost_salvage_and_improve_steering_response() {
        let mut inventory = Inventory::default();
        let mut upgrades = VehicleUpgrades::default();
        inventory.add(ItemKind::Salvage, 12);
        assert!(upgrades.purchase_handling_upgrade(&mut inventory));
        assert_eq!(upgrades.handling_level(), 1);
        assert_eq!(inventory.count(ItemKind::Salvage), 10);
        let tuned = upgrades.apply_to(VehicleConfig::default());
        assert!((tuned.steering_rate - 1.92).abs() < 0.0001);
        assert!((tuned.drag - 0.135).abs() < 0.0001);
        assert!(upgrades.purchase_handling_upgrade(&mut inventory));
        assert!(upgrades.purchase_handling_upgrade(&mut inventory));
        assert_eq!(upgrades.next_handling_cost(), None);
        assert!(!upgrades.purchase_handling_upgrade(&mut inventory));
    }

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
    fn suspension_pose_tracks_front_rear_pitch_and_left_right_roll() {
        let config = SuspensionConfig::default();
        let mut pitch_suspension = SuspensionState::default();
        let mut pitch_pose = SuspensionPose::default();
        let mut roll_suspension = SuspensionState::default();
        let mut roll_pose = SuspensionPose::default();

        for _ in 0..20 {
            pitch_pose = pitch_suspension.update([0.2, 0.2, 0.0, 0.0], 0.58, config, 0.05);
            roll_pose = roll_suspension.update([0.2, 0.0, 0.2, 0.0], 0.58, config, 0.05);
        }

        assert!(pitch_pose.pitch.abs() > 0.001);
        assert!(pitch_pose.roll.abs() < 0.001);
        assert!(roll_pose.roll.abs() > 0.001);
        assert!(roll_pose.pitch.abs() < 0.001);
        assert!(
            pitch_pose.pitch.is_finite()
                && pitch_pose.roll.is_finite()
                && roll_pose.pitch.is_finite()
                && roll_pose.roll.is_finite()
        );
    }

    #[test]
    fn suspension_settles_to_a_stable_flat_ground_compression() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();
        let mut pose = SuspensionPose::default();

        for _ in 0..360 {
            pose = suspension.update([0.2; 4], 0.58, config, 1.0 / 60.0);
        }

        assert!((pose.average_compression - 0.2).abs() < 0.01);
        assert!(pose.pitch.abs() < 0.001);
        assert!(pose.roll.abs() < 0.001);
        assert!(
            suspension
                .compression_velocity
                .iter()
                .all(|speed| speed.abs() < 0.01)
        );
    }

    #[test]
    fn suspension_recovers_smoothly_after_a_single_wheel_bump() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();
        for _ in 0..30 {
            suspension.update([0.0; 4], 0.58, config, 1.0 / 60.0);
        }

        let mut bump_pose = SuspensionPose::default();
        for _ in 0..6 {
            bump_pose = suspension.update([0.42, 0.0, 0.0, 0.0], 0.58, config, 1.0 / 60.0);
        }
        assert!(bump_pose.pitch.is_finite() && bump_pose.roll.is_finite());
        assert!(bump_pose.pitch.abs() > 0.001);

        let mut settled_pose = bump_pose;
        for _ in 0..360 {
            settled_pose = suspension.update([0.0; 4], 0.58, config, 1.0 / 60.0);
        }
        assert!(settled_pose.pitch.abs() < bump_pose.pitch.abs());
        assert!(settled_pose.roll.abs() < 0.001);
    }

    #[test]
    fn suspension_rolls_over_a_one_sided_terrain_ridge_and_recovers() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();

        for _ in 0..30 {
            suspension.update([0.0; 4], 0.58, config, 1.0 / 60.0);
        }

        let mut ridge_pose = SuspensionPose::default();
        for _ in 0..12 {
            ridge_pose = suspension.update([0.32, 0.0, 0.32, 0.0], 0.58, config, 1.0 / 60.0);
        }

        assert!(ridge_pose.roll.abs() > 0.001);
        assert!(ridge_pose.pitch.abs() < 0.001);
        assert!(ridge_pose.roll.is_finite());

        let mut recovered_pose = ridge_pose;
        for _ in 0..360 {
            recovered_pose = suspension.update([0.0; 4], 0.58, config, 1.0 / 60.0);
        }

        assert!(recovered_pose.roll.abs() < ridge_pose.roll.abs());
        assert!(recovered_pose.roll.abs() < 0.001);
    }

    #[test]
    fn suspension_pose_tracks_combined_terrain_pitch_and_roll() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();

        for _ in 0..120 {
            suspension.update([0.3, 0.1, 0.1, -0.1], 0.58, config, 1.0 / 60.0);
        }

        let pose = suspension.pose(config);
        assert!(pose.pitch > 0.0);
        assert!(pose.roll > 0.0);
        assert!(pose.pitch.is_finite() && pose.roll.is_finite());
    }

    #[test]
    fn suspension_compression_stays_within_travel_limits_on_extreme_terrain() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();

        for _ in 0..120 {
            suspension.update([100.0, -100.0, 100.0, -100.0], 0.58, config, 1.0 / 60.0);
        }

        assert!(
            suspension
                .compression
                .iter()
                .all(|compression| *compression >= 0.0 && *compression <= config.max_travel)
        );
        assert!(
            suspension
                .compression_velocity
                .iter()
                .all(|velocity| velocity.is_finite())
        );
        let pose = suspension.pose(config);
        assert!(pose.average_compression.is_finite());
        assert!(pose.pitch.is_finite() && pose.roll.is_finite());
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
    fn low_grip_terrain_increases_braking_distance() {
        let config = VehicleConfig::default();
        let input = VehicleInput {
            brake: 1.0,
            ..VehicleInput::default()
        };
        let mut asphalt = VehicleState {
            speed: 12.0,
            ..VehicleState::default()
        };
        let mut snow = asphalt;

        for _ in 0..10 {
            asphalt.update(input, VehicleSurface::Asphalt, config, 0.1);
            snow.update(input, VehicleSurface::Snow, config, 0.1);
        }

        assert!(asphalt.speed < snow.speed);
        assert!(snow.speed > 0.0);
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
    fn every_terrain_surface_enforces_its_own_forward_speed_limit() {
        let config = VehicleConfig {
            acceleration: 1_000.0,
            drag: 0.0,
            ..VehicleConfig::default()
        };
        let input = VehicleInput {
            throttle: 1.0,
            ..VehicleInput::default()
        };
        let surfaces = [
            VehicleSurface::Asphalt,
            VehicleSurface::Gravel,
            VehicleSurface::Rock,
            VehicleSurface::Sand,
            VehicleSurface::Snow,
            VehicleSurface::Mud,
        ];

        for surface in surfaces {
            let mut vehicle = VehicleState::default();
            for _ in 0..10 {
                vehicle.update(input, surface, config, 0.05);
            }
            let surface_limit = config.max_forward_speed * surface.speed_multiplier();
            assert!((vehicle.speed - surface_limit).abs() < 0.001);
        }
    }

    #[test]
    fn acceleration_response_decreases_with_surface_traction() {
        let config = VehicleConfig::default();
        let input = VehicleInput {
            throttle: 1.0,
            ..VehicleInput::default()
        };
        let mut speeds = Vec::new();
        for surface in [
            VehicleSurface::Asphalt,
            VehicleSurface::Gravel,
            VehicleSurface::Rock,
            VehicleSurface::Sand,
            VehicleSurface::Mud,
            VehicleSurface::Snow,
        ] {
            let mut vehicle = VehicleState::default();
            for _ in 0..20 {
                vehicle.update(input, surface, config, 0.05);
            }
            speeds.push(vehicle.speed);
        }

        assert!(speeds.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn steering_response_scales_consistently_across_all_terrain_surfaces() {
        let config = VehicleConfig::default();
        let input = VehicleInput {
            steering: 1.0,
            ..VehicleInput::default()
        };
        let surfaces = [
            VehicleSurface::Asphalt,
            VehicleSurface::Gravel,
            VehicleSurface::Rock,
            VehicleSurface::Sand,
            VehicleSurface::Mud,
            VehicleSurface::Snow,
        ];
        let mut headings = Vec::with_capacity(surfaces.len());

        for surface in surfaces {
            let mut vehicle = VehicleState {
                speed: 10.0,
                ..VehicleState::default()
            };
            vehicle.update(input, surface, config, 0.1);
            headings.push(vehicle.heading);
        }

        assert!(headings.iter().all(|heading| *heading > 0.0));
        assert!(headings.windows(2).all(|pair| pair[0] > pair[1]));
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

    #[test]
    fn every_terrain_surface_enforces_its_own_reverse_speed_limit() {
        let config = VehicleConfig {
            acceleration: 1_000.0,
            drag: 0.0,
            ..VehicleConfig::default()
        };
        let input = VehicleInput {
            throttle: -1.0,
            ..VehicleInput::default()
        };
        let surfaces = [
            VehicleSurface::Asphalt,
            VehicleSurface::Gravel,
            VehicleSurface::Rock,
            VehicleSurface::Sand,
            VehicleSurface::Snow,
            VehicleSurface::Mud,
        ];

        for surface in surfaces {
            let mut vehicle = VehicleState::default();
            for _ in 0..10 {
                vehicle.update(input, surface, config, 0.05);
            }
            let surface_limit = config.max_reverse_speed * surface.speed_multiplier();
            assert!((vehicle.speed + surface_limit).abs() < 0.001);
        }
    }

    #[test]
    fn reverse_steering_response_scales_with_terrain_traction() {
        let config = VehicleConfig::default();
        let input = VehicleInput {
            steering: 1.0,
            ..VehicleInput::default()
        };
        let surfaces = [
            VehicleSurface::Asphalt,
            VehicleSurface::Gravel,
            VehicleSurface::Rock,
            VehicleSurface::Sand,
            VehicleSurface::Mud,
            VehicleSurface::Snow,
        ];
        let mut heading_changes = Vec::with_capacity(surfaces.len());

        for surface in surfaces {
            let mut vehicle = VehicleState {
                speed: -10.0,
                ..VehicleState::default()
            };
            vehicle.update(input, surface, config, 0.1);
            heading_changes.push(vehicle.heading);
        }

        assert!(heading_changes.iter().all(|heading| *heading < 0.0));
        assert!(heading_changes.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn blended_terrain_response_produces_intermediate_acceleration() {
        let config = VehicleConfig::default();
        let input = VehicleInput {
            throttle: 1.0,
            ..VehicleInput::default()
        };
        let mut asphalt = VehicleState::default();
        let mut snow = VehicleState::default();
        let mut transition = VehicleState::default();

        asphalt.update(input, VehicleSurface::Asphalt, config, 0.1);
        snow.update(input, VehicleSurface::Snow, config, 0.1);
        transition.update_with_surface_response(
            input,
            f32::midpoint(VehicleSurface::Asphalt.traction(), VehicleSurface::Snow.traction()),
            f32::midpoint(
                VehicleSurface::Asphalt.speed_multiplier(),
                VehicleSurface::Snow.speed_multiplier(),
            ),
            f32::midpoint(
                VehicleSurface::Asphalt.rolling_resistance(),
                VehicleSurface::Snow.rolling_resistance(),
            ),
            config,
            0.1,
        );

        assert!(transition.speed > snow.speed);
        assert!(transition.speed < asphalt.speed);
    }
}
