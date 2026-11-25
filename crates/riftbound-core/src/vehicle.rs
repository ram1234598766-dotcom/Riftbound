//! Deterministic lightweight ground-vehicle handling for the open-world driving RPG.

use crate::{
    Inventory, ItemKind, Vec3, world_terrain_vehicle_ground_heights,
    world_terrain_vehicle_surface, world_terrain_vehicle_surfaces,
};

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
    blend_from: [f32; 3],
    target_response: [f32; 3],
    blend_frames: u8,
    side_blend_from: [f32; 2],
    side_target: [f32; 2],
    side_blend_frames: u8,
    wheel_blend_from: [f32; 4],
    wheel_target: [f32; 4],
    wheel_blend_frames: u8,
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
            blend_from: [
                initial.traction(),
                initial.speed_multiplier(),
                initial.rolling_resistance(),
            ],
            target_response: [
                initial.traction(),
                initial.speed_multiplier(),
                initial.rolling_resistance(),
            ],
            blend_frames: Self::TRANSITION_FRAMES,
            side_blend_from: [initial.traction(); 2],
            side_target: [initial.traction(); 2],
            side_blend_frames: Self::TRANSITION_FRAMES,
            wheel_blend_from: [initial.traction(); 4],
            wheel_target: [initial.traction(); 4],
            wheel_blend_frames: Self::TRANSITION_FRAMES,
        }
    }

    /// Number of fixed updates used to blend accepted terrain changes.
    pub const TRANSITION_FRAMES: u8 = 18;

    /// Returns blended traction, speed multiplier, and rolling resistance.
    #[must_use]
    pub fn response(self) -> (f32, f32, f32) {
        let target = self.target_response;
        let amount = f32::from(self.blend_frames) / f32::from(Self::TRANSITION_FRAMES);
        (
            self.blend_from[0] + (target[0] - self.blend_from[0]) * amount,
            self.blend_from[1] + (target[1] - self.blend_from[1]) * amount,
            self.blend_from[2] + (target[2] - self.blend_from[2]) * amount,
        )
    }

    /// Returns blended grip for front-left, front-right, rear-left, rear-right.
    #[must_use]
    pub fn wheel_traction(self) -> [f32; 4] {
        let amount = f32::from(self.wheel_blend_frames) / f32::from(Self::TRANSITION_FRAMES);
        std::array::from_fn(|index| {
            self.wheel_blend_from[index]
                + (self.wheel_target[index] - self.wheel_blend_from[index]) * amount
        })
    }

    /// Returns blended left/right tyre grip, in vehicle-local side order.
    #[must_use]
    pub fn side_traction(self) -> (f32, f32) {
        let amount = f32::from(self.side_blend_frames) / f32::from(Self::TRANSITION_FRAMES);
        (
            self.side_blend_from[0] + (self.side_target[0] - self.side_blend_from[0]) * amount,
            self.side_blend_from[1] + (self.side_target[1] - self.side_blend_from[1]) * amount,
        )
    }

    /// Returns the stable surface, accepting changes after repeated matching samples.
    pub fn update(&mut self, sampled: VehicleSurface) -> VehicleSurface {
        let response_before_update = self.response();
        if sampled == self.current {
            self.blend_frames = self
                .blend_frames
                .saturating_add(1)
                .min(Self::TRANSITION_FRAMES);
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
            self.blend_from = [
                response_before_update.0,
                response_before_update.1,
                response_before_update.2,
            ];
            self.blend_frames = 0;
            self.current = sampled;
            self.target_response = [
                sampled.traction(),
                sampled.speed_multiplier(),
                sampled.rolling_resistance(),
            ];
            self.candidate = None;
            self.candidate_frames = 0;
        }
        self.current
    }

    /// Updates handling from four individual tyre contacts.
    ///
    /// The dominant contact still passes through the confirmation filter, while
    /// the response target is the mean of all four tyre profiles. A short blend
    /// each fixed update prevents abrupt changes as individual tyres cross edges.
    pub fn update_wheel_contacts(
        &mut self,
        dominant_surface: VehicleSurface,
        contacts: [VehicleSurface; 4],
    ) -> VehicleSurface {
        let response_before_update = self.response();
        let side_before_update = self.side_traction();
        let wheel_before_update = self.wheel_traction();
        self.update(dominant_surface);
        let side_target = [
            (contacts[0].traction() + contacts[2].traction()) * 0.5,
            (contacts[1].traction() + contacts[3].traction()) * 0.5,
        ];
        self.side_blend_frames = self
            .side_blend_frames
            .saturating_add(1)
            .min(Self::TRANSITION_FRAMES);
        if side_target != self.side_target {
            self.side_blend_from = [side_before_update.0, side_before_update.1];
            self.side_target = side_target;
            self.side_blend_frames = 0;
        }
        self.wheel_blend_frames = self
            .wheel_blend_frames
            .saturating_add(1)
            .min(Self::TRANSITION_FRAMES);
        let wheel_target = contacts.map(VehicleSurface::traction);
        if wheel_target != self.wheel_target {
            self.wheel_blend_from = wheel_before_update;
            self.wheel_target = wheel_target;
            self.wheel_blend_frames = 0;
        }
        let target = [
            contacts
                .iter()
                .map(|surface| surface.traction())
                .sum::<f32>()
                * 0.25,
            contacts
                .iter()
                .map(|surface| surface.speed_multiplier())
                .sum::<f32>()
                * 0.25,
            contacts
                .iter()
                .map(|surface| surface.rolling_resistance())
                .sum::<f32>()
                * 0.25,
        ];
        if target != self.target_response {
            self.blend_from = [
                response_before_update.0,
                response_before_update.1,
                response_before_update.2,
            ];
            self.target_response = target;
            self.blend_frames = 0;
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

/// Compact four-corner wheel rotational state for traction and slip feedback.
/// Corner order is front-left, front-right, rear-left, rear-right.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WheelSlipState {
    angular_velocity: [f32; 4],
    slip_ratio: [f32; 4],
}

impl WheelSlipState {
    /// Returns each wheel's signed slip ratio, clamped to the range -1..=1.
    #[must_use]
    pub const fn slip_ratios(self) -> [f32; 4] {
        self.slip_ratio
    }

    /// Returns each wheel's rotational speed in radians per second.
    #[must_use]
    pub const fn angular_velocities(self) -> [f32; 4] {
        self.angular_velocity
    }

    /// Advances a lightweight wheel-spin approximation without allocating.
    ///
    /// Drive torque spins the wheel, braking slows its rotation, and surface grip
    /// couples wheel speed toward ground speed. This is a rotational slip model,
    /// not a full tire-force or suspension-contact solver.
    pub fn update(
        &mut self,
        input: VehicleInput,
        ground_speed: f32,
        wheel_radius: f32,
        wheel_traction: [f32; 4],
        delta_seconds: f32,
    ) {
        if !ground_speed.is_finite()
            || !wheel_radius.is_finite()
            || wheel_radius <= 0.0
            || !delta_seconds.is_finite()
            || delta_seconds <= 0.0
            || !input.throttle.is_finite()
            || !input.brake.is_finite()
            || wheel_traction
                .iter()
                .any(|grip| !grip.is_finite() || *grip <= 0.0 || *grip > 1.0)
        {
            return;
        }

        let dt = delta_seconds.min(0.1);
        let throttle = input.throttle.clamp(-1.0, 1.0);
        let brake = input.brake.clamp(0.0, 1.0);
        let ground_angular_velocity = ground_speed / wheel_radius;
        for (index, grip) in wheel_traction.into_iter().enumerate() {
            let angular_velocity = self.angular_velocity[index];
            if !angular_velocity.is_finite() {
                self.angular_velocity[index] = 0.0;
                self.slip_ratio[index] = 0.0;
                continue;
            }
            let drive_acceleration = throttle * 150.0;
            let brake_acceleration = brake * 30.0 * angular_velocity.signum();
            let contact_coupling =
                (ground_angular_velocity - angular_velocity) * (2.0 + grip * 18.0);
            let next_angular_velocity = (angular_velocity
                + (drive_acceleration - brake_acceleration + contact_coupling) * dt)
                .clamp(-120.0, 120.0);
            self.angular_velocity[index] = next_angular_velocity;
            self.slip_ratio[index] = ((next_angular_velocity * wheel_radius - ground_speed)
                / ground_speed.abs().max(2.0))
                .clamp(-1.0, 1.0);
        }
    }
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
    /// Persistent rotational speed and slip for each wheel corner.
    pub wheel_slip: WheelSlipState,
}

impl VehicleState {
    /// Advances handling and four-wheel suspension using the shared procedural world.
    ///
    /// Returns None when the vehicle pose is outside the supported terrain sampling
    /// range; in that case neither handling nor suspension state is advanced.
    pub fn update_on_world_terrain(
        &mut self,
        input: VehicleInput,
        config: VehicleConfig,
        suspension: &mut SuspensionState,
        suspension_config: SuspensionConfig,
        surface_tracker: &mut VehicleSurfaceTracker,
        delta_seconds: f32,
    ) -> Option<SuspensionPose> {
        let half_track = suspension_config.track_width * 0.5;
        let half_wheelbase = suspension_config.wheelbase * 0.5;
        world_terrain_vehicle_ground_heights(
            self.position.x,
            self.position.z,
            self.heading,
            half_track,
            half_wheelbase,
        )?;

        // Work on copies so an invalid destination sample cannot partially advance
        // the vehicle, suspension, or blended surface tracker.
        let mut next_vehicle = *self;
        let mut next_suspension = *suspension;
        let mut next_surface_tracker = *surface_tracker;
        let wheel_surfaces = world_terrain_vehicle_surfaces(
            next_vehicle.position.x,
            next_vehicle.position.z,
            next_vehicle.heading,
            half_track,
            half_wheelbase,
        );
        let dominant_surface = world_terrain_vehicle_surface(
            next_vehicle.position.x,
            next_vehicle.position.z,
            next_vehicle.heading,
            half_track,
            half_wheelbase,
        );
        next_surface_tracker.update_wheel_contacts(dominant_surface, wheel_surfaces);
        let wheel_traction = next_surface_tracker.wheel_traction();
        next_vehicle.wheel_slip.update(
            input,
            next_vehicle.speed,
            0.34,
            wheel_traction,
            delta_seconds,
        );
        let slip = next_vehicle.wheel_slip.slip_ratios();
        let suspension_load = next_suspension.wheel_load_factors(suspension_config);
        let traction_with_slip = std::array::from_fn::<_, 4, _>(|index| {
            let slip_grip = (1.0 - 0.35 * slip[index].abs()).clamp(0.55, 1.0);
            wheel_traction[index] * slip_grip * suspension_load[index]
        });
        next_vehicle.update_with_four_wheel_surface_response(
            input,
            next_surface_tracker.response(),
            traction_with_slip,
            config,
            delta_seconds,
        );
        let updated_heights = world_terrain_vehicle_ground_heights(
            next_vehicle.position.x,
            next_vehicle.position.z,
            next_vehicle.heading,
            half_track,
            half_wheelbase,
        )?;
        let pose = next_suspension.update(
            updated_heights,
            next_vehicle.position.y,
            suspension_config,
            delta_seconds,
        );
        *self = next_vehicle;
        *suspension = next_suspension;
        *surface_tracker = next_surface_tracker;
        Some(pose)
    }

    /// Advances the vehicle without allocating. Delta time is bounded to avoid
    /// large jumps after stalls or application suspension.
    pub fn update(
        &mut self,
        input: VehicleInput,
        surface: VehicleSurface,
        config: VehicleConfig,
        delta_seconds: f32,
    ) {
        self.update_with_wheel_surface_response(
            input,
            (
                surface.traction(),
                surface.speed_multiplier(),
                surface.rolling_resistance(),
            ),
            (surface.traction(), surface.traction()),
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
        self.update_with_wheel_surface_response(
            input,
            (traction, speed_multiplier, rolling_resistance),
            (traction, traction),
            config,
            delta_seconds,
        );
    }

    /// Advances handling using independently blended traction at all four corners.
    /// Front and rear axle averages determine steering and drive grip; left/right
    /// averages preserve the existing asymmetric-braking response.
    pub fn update_with_four_wheel_surface_response(
        &mut self,
        input: VehicleInput,
        surface_response: (f32, f32, f32),
        wheel_traction: [f32; 4],
        config: VehicleConfig,
        delta_seconds: f32,
    ) {
        if wheel_traction
            .iter()
            .any(|grip| !grip.is_finite() || *grip <= 0.0 || *grip > 1.0)
        {
            return;
        }
        let average = wheel_traction.iter().sum::<f32>() * 0.25;
        let left = (wheel_traction[0] + wheel_traction[2]) * 0.5;
        let right = (wheel_traction[1] + wheel_traction[3]) * 0.5;
        self.update_with_wheel_surface_response(
            input,
            (average, surface_response.1, surface_response.2),
            (left, right),
            config,
            delta_seconds,
        );
    }

    /// Advances handling with separate blended left/right tyre grip.
    pub fn update_with_wheel_surface_response(
        &mut self,
        input: VehicleInput,
        surface_response: (f32, f32, f32),
        side_traction: (f32, f32),
        config: VehicleConfig,
        delta_seconds: f32,
    ) {
        let (traction, speed_multiplier, rolling_resistance) = surface_response;
        let (left_traction, right_traction) = side_traction;
        if !traction.is_finite()
            || !left_traction.is_finite()
            || !right_traction.is_finite()
            || left_traction <= 0.0
            || left_traction > 1.0
            || right_traction <= 0.0
            || right_traction > 1.0
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
        let speed_before_step = self.speed;
        let throttle = input.throttle.clamp(-1.0, 1.0);
        let brake = input.brake.clamp(0.0, 1.0);
        let steering = input.steering.clamp(-1.0, 1.0);
        let grip = traction;
        let grip_imbalance = right_traction - left_traction;
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
        let drag_rate = config.drag + rolling_resistance * 0.025;
        // Exponential damping is stable for every supported step and composes
        // consistently when the same elapsed time is split across frame rates.
        self.speed *= (-drag_rate * dt).exp();
        self.speed = self.speed.clamp(
            -config.max_reverse_speed * speed_multiplier,
            config.max_forward_speed * speed_multiplier,
        );

        let speed_limit = if self.speed < 0.0 {
            config.max_reverse_speed
        } else {
            config.max_forward_speed
        };
        let speed_fraction = (self.speed.abs() / speed_limit).clamp(0.0, 1.0);
        let steering_stability = 1.0 - grip_imbalance.abs() * 0.12;
        self.heading += steering
            * config.steering_rate
            * grip
            * steering_stability
            * speed_fraction
            * self.speed.signum()
            * dt;
        // Unequal braking grip creates a small, physically motivated pull toward
        // the side with more grip; blending the side values softens road-edge transitions.
        self.heading += grip_imbalance * brake * speed_fraction * 0.25 * dt;
        // Trapezoidal displacement integrates the changing speed over this
        // step instead of moving the entire step at its final speed.
        let average_speed = (speed_before_step + self.speed) * 0.5;
        self.position.x += self.heading.sin() * average_speed * dt;
        self.position.z -= self.heading.cos() * average_speed * dt;
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
    /// Per-wheel spring compression in front-left, front-right, rear-left, rear-right order.
    pub wheel_compressions: [f32; 4],
    /// Per-wheel spring compression velocity in world units per second.
    pub wheel_compression_velocities: [f32; 4],
    /// Peak normalized compression speed, useful for bump and landing feedback.
    pub impact_intensity: f32,
    /// Positive compression velocity on contact, emphasizing landing impacts.
    pub landing_intensity: f32,
    /// Mean spring compression across all wheels.
    pub average_compression: f32,
    /// Damped average terrain height followed by the sprung chassis.
    pub average_ground_height: f32,
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
    average_ground_height: f32,
    average_ground_velocity: f32,
    ground_reference_initialized: bool,
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
        let target_ground_height = wheel_ground_heights.iter().sum::<f32>() * 0.25;
        if !self.ground_reference_initialized {
            self.average_ground_height = target_ground_height;
            self.average_ground_velocity = 0.0;
            self.ground_reference_initialized = true;
        }
        for _ in 0..substeps {
            let body_follow_strength = config.spring_strength * 0.35;
            let body_follow_acceleration = (target_ground_height - self.average_ground_height)
                * body_follow_strength
                - self.average_ground_velocity * config.damping;
            self.average_ground_velocity += body_follow_acceleration * step_dt;
            self.average_ground_height += self.average_ground_velocity * step_dt;
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

    /// Estimates each corner's usable grip from relative spring load and contact.
    /// An uninitialized suspension keeps neutral grip; an initialized wheel at full
    /// extension contributes no load, with a smooth ramp as the spring compresses.
    #[must_use]
    pub fn wheel_load_factors(&self, config: SuspensionConfig) -> [f32; 4] {
        if !self.ground_reference_initialized {
            return [1.0; 4];
        }
        if !config.max_travel.is_finite() || config.max_travel <= f32::EPSILON {
            return [0.0; 4];
        }
        let average = self.compression.iter().sum::<f32>() * 0.25;
        std::array::from_fn(|index| {
            let relative_load = (1.0
                + (self.compression[index] - average) / config.max_travel * 0.8)
                .clamp(0.65, 1.25);
            let contact = (self.compression[index] / (config.max_travel * 0.25))
                .clamp(0.0, 1.0);
            relative_load * contact
        })
    }

    /// Reprojects wheel spring compression onto new contacts without advancing time.
    ///
    /// Use after collision resolution changes the vehicle's horizontal position.
    /// Spring velocities are preserved so the next fixed step continues smoothly.
    pub fn rebase_ground_contacts(
        &mut self,
        wheel_ground_heights: [f32; 4],
        chassis_height: f32,
        config: SuspensionConfig,
    ) -> bool {
        if !chassis_height.is_finite()
            || wheel_ground_heights.iter().any(|height| !height.is_finite())
            || !config.max_travel.is_finite()
            || !config.rest_length.is_finite()
            || config.max_travel <= 0.0
            || config.rest_length < 0.0
        {
            return false;
        }

        self.average_ground_height = wheel_ground_heights.iter().sum::<f32>() * 0.25;
        self.average_ground_velocity = 0.0;
        self.ground_reference_initialized = true;
        for (index, ground_height) in wheel_ground_heights.iter().enumerate() {
            self.compression[index] = (ground_height + config.rest_length - chassis_height)
                .clamp(0.0, config.max_travel);
            if self.compression[index] <= f32::EPSILON
                || self.compression[index] >= config.max_travel - f32::EPSILON
            {
                self.compression_velocity[index] = 0.0;
            }
        }
        true
    }

    /// Returns the current visual chassis response without advancing simulation.
    #[must_use]
    pub fn pose(&self, config: SuspensionConfig) -> SuspensionPose {
        let front = f32::midpoint(self.compression[0], self.compression[1]);
        let rear = f32::midpoint(self.compression[2], self.compression[3]);
        let left = f32::midpoint(self.compression[0], self.compression[2]);
        let right = f32::midpoint(self.compression[1], self.compression[3]);
        let peak_compression_speed = self
            .compression_velocity
            .iter()
            .map(|velocity| velocity.abs())
            .fold(0.0_f32, f32::max);
        let peak_landing_speed = self
            .compression_velocity
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        SuspensionPose {
            wheel_compressions: self.compression,
            wheel_compression_velocities: self.compression_velocity,
            impact_intensity: (peak_compression_speed / 4.0).clamp(0.0, 1.0),
            landing_intensity: (peak_landing_speed / 4.0).clamp(0.0, 1.0),
            average_compression: self.compression.iter().sum::<f32>() * 0.25,
            average_ground_height: if self.ground_reference_initialized {
                self.average_ground_height
            } else {
                0.0
            },
            pitch: ((front - rear) / config.wheelbase.max(f32::EPSILON)).atan(),
            roll: ((left - right) / config.track_width.max(f32::EPSILON)).atan(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_slip_state_tracks_four_independent_grip_values() {
        let mut state = WheelSlipState::default();
        state.update(
            VehicleInput::default(),
            6.0,
            0.3,
            [1.0, 1.0, 0.25, 0.25],
            1.0 / 60.0,
        );
        let slip = state.slip_ratios();
        assert!(slip.iter().all(|value| value.is_finite() && (-1.0..=1.0).contains(value)));
        assert!((slip[0] - slip[1]).abs() < f32::EPSILON);
        assert!((slip[2] - slip[3]).abs() < f32::EPSILON);
        assert!(slip[2].abs() > slip[0].abs());
    }

    #[test]
    fn wheel_slip_state_rejects_invalid_inputs_without_mutation() {
        let mut state = WheelSlipState::default();
        state.update(
            VehicleInput::default(),
            4.0,
            0.3,
            [1.0; 4],
            1.0 / 60.0,
        );
        let before = state;
        state.update(
            VehicleInput::default(),
            f32::NAN,
            0.3,
            [1.0; 4],
            1.0 / 60.0,
        );
        assert_eq!(state, before);
        state.update(
            VehicleInput::default(),
            4.0,
            0.0,
            [1.0; 4],
            1.0 / 60.0,
        );
        assert_eq!(state, before);
    }

    #[test]
    fn world_vehicle_update_persists_wheel_spin_and_reduces_slipping_grip() {
        let mut vehicle = VehicleState::default();
        let mut suspension = SuspensionState::default();
        let mut tracker = VehicleSurfaceTracker::default();
        vehicle.update_on_world_terrain(
            VehicleInput { throttle: 1.0, ..VehicleInput::default() },
            VehicleConfig::default(),
            &mut suspension,
            SuspensionConfig::default(),
            &mut tracker,
            1.0 / 60.0,
        ).expect("origin is within terrain bounds");
        let spin = vehicle.wheel_slip.angular_velocities();
        assert!(spin.iter().all(|speed| speed.is_finite() && *speed > 0.0));
        assert!(vehicle.wheel_slip.slip_ratios().iter().all(|slip| slip.is_finite() && (-1.0..=1.0).contains(slip)));
    }

    #[test]
    fn world_terrain_vehicle_update_combines_surface_grip_and_suspension() {
        let mut vehicle = VehicleState {
            position: Vec3::new(0.0, 1.0, 0.0),
            ..VehicleState::default()
        };
        let mut suspension = SuspensionState::default();
        let mut tracker = VehicleSurfaceTracker::default();
        let pose = vehicle
            .update_on_world_terrain(
                VehicleInput {
                    throttle: 1.0,
                    ..VehicleInput::default()
                },
                VehicleConfig::default(),
                &mut suspension,
                SuspensionConfig::default(),
                &mut tracker,
                1.0 / 60.0,
            )
            .expect("origin is inside the supported terrain range");
        assert!(vehicle.position.x.is_finite());
        assert!(vehicle.position.z < 0.0);
        assert!(pose.wheel_compressions.iter().all(|value| value.is_finite()));
        assert_eq!(tracker.current(), VehicleSurface::Asphalt);
    }

    #[test]
    fn world_terrain_vehicle_update_rejects_out_of_range_pose_without_mutation() {
        let mut vehicle = VehicleState {
            position: Vec3::new(1_000_001.0, 1.0, 0.0),
            speed: 3.0,
            ..VehicleState::default()
        };
        let before = vehicle;
        let mut suspension = SuspensionState::default();
        let mut tracker = VehicleSurfaceTracker::default();
        assert!(vehicle
            .update_on_world_terrain(
                VehicleInput::default(),
                VehicleConfig::default(),
                &mut suspension,
                SuspensionConfig::default(),
                &mut tracker,
                1.0 / 60.0,
            )
            .is_none());
        assert_eq!(vehicle, before);
    }

    #[test]
    fn suspension_rebase_updates_contacts_without_advancing_spring_time() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();
        suspension.update([0.2; 4], 1.0, config, 1.0 / 60.0);
        let before_velocity = suspension.compression_velocity;
        assert!(suspension.rebase_ground_contacts(
            [0.1, 0.3, 0.2, 0.4],
            1.0,
            config,
        ));
        let pose = suspension.pose(config);
        assert_eq!(pose.wheel_compressions, [0.1 + config.rest_length - 1.0, 0.3 + config.rest_length - 1.0, 0.2 + config.rest_length - 1.0, 0.4 + config.rest_length - 1.0].map(|value| value.clamp(0.0, config.max_travel)));
        assert_eq!(suspension.compression_velocity, before_velocity);
        assert!((pose.average_ground_height - 0.25).abs() < 0.0001);
    }

    #[test]
    fn suspension_rebase_rejects_invalid_contacts_without_mutation() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();
        suspension.update([0.2; 4], 1.0, config, 1.0 / 60.0);
        let before = suspension;
        assert!(!suspension.rebase_ground_contacts(
            [0.1, f32::NAN, 0.2, 0.4],
            1.0,
            config,
        ));
        assert_eq!(suspension, before);
    }

    #[test]
    fn suspension_response_is_consistent_at_thirty_and_sixty_fps() {
        let config = SuspensionConfig::default();
        let terrain = [0.18, 0.31, 0.22, 0.39];
        let mut at_30_fps = SuspensionState::default();
        let mut at_60_fps = SuspensionState::default();

        for _ in 0..30 {
            at_30_fps.update(terrain, 0.92, config, 1.0 / 30.0);
        }
        for _ in 0..60 {
            at_60_fps.update(terrain, 0.92, config, 1.0 / 60.0);
        }

        let pose_30 = at_30_fps.pose(config);
        let pose_60 = at_60_fps.pose(config);
        for (left, right) in pose_30
            .wheel_compressions
            .iter()
            .zip(pose_60.wheel_compressions.iter())
        {
            assert!((left - right).abs() < 0.0001);
        }
        assert!((pose_30.average_ground_height - pose_60.average_ground_height).abs() < 0.0001);
        assert!((pose_30.pitch - pose_60.pitch).abs() < 0.0001);
        assert!((pose_30.roll - pose_60.roll).abs() < 0.0001);
    }

    #[test]
    fn suspension_load_factors_transfer_grip_between_wheel_corners() {
        let mut suspension = SuspensionState::default();
        suspension.compression = [0.4, 0.2, 0.1, 0.1];
        let loads = suspension.wheel_load_factors(SuspensionConfig::default());
        assert!(loads[0] > loads[1]);
        assert!(loads[1] > loads[2]);
        assert!(loads.iter().all(|load| (0.65..=1.25).contains(load)));
        let average = loads.iter().sum::<f32>() * 0.25;
        assert!((average - 1.0).abs() < 0.0001);
    }

    #[test]
    fn suspension_load_factors_reduce_grip_for_unloaded_wheels() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();
        assert_eq!(suspension.wheel_load_factors(config), [1.0; 4]);

        suspension.ground_reference_initialized = true;
        suspension.compression = [0.0, 0.1, 0.2, 0.3];
        let loads = suspension.wheel_load_factors(config);
        assert_eq!(loads[0], 0.0);
        assert!(loads[1] > 0.0 && loads[1] < loads[2]);
        assert!(loads[2] < loads[3]);
        assert!(loads.iter().all(|load| (0.0..=1.25).contains(load)));
    }

    #[test]
    fn suspension_pose_separates_landing_compression_from_rebound() {
        let config = SuspensionConfig::default();
        let mut landing = SuspensionState::default();
        for _ in 0..30 {
            landing.update([0.0; 4], 0.58, config, 1.0 / 60.0);
        }
        let landing_pose = landing.update([0.38; 4], 0.58, config, 1.0 / 60.0);
        assert!(landing_pose.landing_intensity > 0.0);
        assert!(landing_pose.landing_intensity <= landing_pose.impact_intensity);

        let mut rebound = SuspensionState::default();
        for _ in 0..30 {
            rebound.update([0.38; 4], 0.58, config, 1.0 / 60.0);
        }
        let rebound_pose = rebound.update([0.0; 4], 0.58, config, 1.0 / 60.0);
        assert_eq!(rebound_pose.landing_intensity, 0.0);
        assert!(rebound_pose.impact_intensity > 0.0);
    }

    #[test]
    fn suspension_pose_reports_bounded_bump_intensity_from_corner_velocity() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();
        suspension.update([0.0; 4], 0.58, config, 1.0 / 60.0);
        let smooth_pose = suspension.pose(config);
        assert_eq!(smooth_pose.impact_intensity, 0.0);
        suspension.update([0.35, 0.0, 0.0, 0.0], 0.58, config, 1.0 / 60.0);
        let bump_pose = suspension.pose(config);
        assert!(bump_pose.impact_intensity > 0.0);
        assert!(bump_pose.impact_intensity <= 1.0);
        assert!(bump_pose.wheel_compression_velocities[0] > 0.0);
        assert!(bump_pose.wheel_compression_velocities[0]
            > bump_pose.wheel_compression_velocities[1]);
    }

    #[test]
    fn suspension_pose_exposes_each_corner_compression_in_stable_order() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();
        let pose = suspension.update([0.12, 0.24, 0.36, 0.48], 0.58, config, 1.0 / 60.0);

        assert!(pose.wheel_compressions[0] < pose.wheel_compressions[1]);
        assert!(pose.wheel_compressions[1] < pose.wheel_compressions[2]);
        assert!(pose.wheel_compressions[2] < pose.wheel_compressions[3]);
        assert!(pose.wheel_compressions.iter().all(|value| {
            value.is_finite() && (0.0..=config.max_travel).contains(value)
        }));
    }

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
    fn wheel_contact_response_averages_all_four_tyre_surfaces() {
        let mut tracker = VehicleSurfaceTracker::new(VehicleSurface::Asphalt);
        let contacts = [
            VehicleSurface::Asphalt,
            VehicleSurface::Asphalt,
            VehicleSurface::Snow,
            VehicleSurface::Snow,
        ];
        for _ in 0..144 {
            tracker.update_wheel_contacts(VehicleSurface::Asphalt, contacts);
        }
        let (traction, speed, resistance) = tracker.response();
        let expected_traction = (VehicleSurface::Asphalt.traction()
            + VehicleSurface::Asphalt.traction()
            + VehicleSurface::Snow.traction()
            + VehicleSurface::Snow.traction())
            * 0.25;
        let expected_speed = (VehicleSurface::Asphalt.speed_multiplier()
            + VehicleSurface::Asphalt.speed_multiplier()
            + VehicleSurface::Snow.speed_multiplier()
            + VehicleSurface::Snow.speed_multiplier())
            * 0.25;
        let expected_resistance = (VehicleSurface::Asphalt.rolling_resistance()
            + VehicleSurface::Asphalt.rolling_resistance()
            + VehicleSurface::Snow.rolling_resistance()
            + VehicleSurface::Snow.rolling_resistance())
            * 0.25;
        assert!((traction - expected_traction).abs() < 0.01);
        assert!((speed - expected_speed).abs() < 0.01);
        assert!((resistance - expected_resistance).abs() < 0.01);
        let (left, right) = tracker.side_traction();
        let expected_side_grip =
            (VehicleSurface::Asphalt.traction() + VehicleSurface::Snow.traction()) * 0.5;
        assert!((left - expected_side_grip).abs() < 0.01);
        assert!((right - expected_side_grip).abs() < 0.01);
    }

    #[test]
    fn asymmetric_braking_grip_pulls_vehicle_toward_higher_grip_side() {
        let config = VehicleConfig::default();
        let mut balanced = VehicleState {
            speed: 12.0,
            ..VehicleState::default()
        };
        let mut uneven = balanced;
        balanced.update_with_wheel_surface_response(
            VehicleInput {
                brake: 0.8,
                ..VehicleInput::default()
            },
            (0.69, 1.0, 1.5),
            (0.69, 0.69),
            config,
            0.05,
        );
        uneven.update_with_wheel_surface_response(
            VehicleInput {
                brake: 0.8,
                ..VehicleInput::default()
            },
            (0.69, 1.0, 1.5),
            (0.38, 1.0),
            config,
            0.05,
        );
        assert!(uneven.heading > balanced.heading);
    }

    #[test]
    fn side_grip_blends_without_a_heading_input_discontinuity() {
        let mut tracker = VehicleSurfaceTracker::new(VehicleSurface::Asphalt);
        let split = [
            VehicleSurface::Snow,
            VehicleSurface::Asphalt,
            VehicleSurface::Snow,
            VehicleSurface::Asphalt,
        ];
        tracker.update_wheel_contacts(VehicleSurface::Asphalt, split);
        let initial = tracker.side_traction();
        assert!((initial.0 - initial.1).abs() < 0.0001);
        for _ in 0..VehicleSurfaceTracker::TRANSITION_FRAMES {
            tracker.update_wheel_contacts(VehicleSurface::Asphalt, split);
        }
        let settled = tracker.side_traction();
        assert!(settled.1 > settled.0);
    }

    #[test]
    fn wheel_contact_blend_reaches_its_target_after_transition_frames() {
        let mut tracker = VehicleSurfaceTracker::new(VehicleSurface::Asphalt);
        let contacts = [VehicleSurface::Snow; 4];
        tracker.update_wheel_contacts(VehicleSurface::Asphalt, contacts);
        for _ in 0..VehicleSurfaceTracker::TRANSITION_FRAMES {
            tracker.update_wheel_contacts(VehicleSurface::Asphalt, contacts);
        }
        let response = tracker.response();
        assert!((response.0 - VehicleSurface::Snow.traction()).abs() < 0.0001);
        assert!((response.1 - VehicleSurface::Snow.speed_multiplier()).abs() < 0.0001);
        assert!((response.2 - VehicleSurface::Snow.rolling_resistance()).abs() < 0.0001);
    }

    #[test]
    fn wheel_contact_blend_retargets_without_a_response_jump() {
        let mut tracker = VehicleSurfaceTracker::new(VehicleSurface::Asphalt);
        let snow = [VehicleSurface::Snow; 4];
        let mud = [VehicleSurface::Mud; 4];
        tracker.update_wheel_contacts(VehicleSurface::Asphalt, snow);
        for _ in 0..7 {
            tracker.update_wheel_contacts(VehicleSurface::Asphalt, snow);
        }
        let before = tracker.response();
        tracker.update_wheel_contacts(VehicleSurface::Mud, mud);
        let after = tracker.response();
        assert!((before.0 - after.0).abs() < 0.0001);
        assert!((before.1 - after.1).abs() < 0.0001);
        assert!((before.2 - after.2).abs() < 0.0001);
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
    fn suspension_chassis_height_smooths_a_single_wheel_bump() {
        let config = SuspensionConfig::default();
        let mut suspension = SuspensionState::default();
        for _ in 0..60 {
            suspension.update([0.0; 4], config.rest_length, config, 1.0 / 60.0);
        }

        let before_bump = suspension.pose(config).average_ground_height;
        let bump = suspension.update(
            [0.48, 0.0, 0.0, 0.0],
            config.rest_length,
            config,
            1.0 / 60.0,
        );
        let raw_average = 0.12;
        assert!((bump.average_ground_height - before_bump).abs() < raw_average * 0.5);
        assert!(bump.average_ground_height.is_finite());

        let mut recovered = bump;
        for _ in 0..360 {
            recovered = suspension.update([0.0; 4], config.rest_length, config, 1.0 / 60.0);
        }
        assert!(recovered.average_ground_height.abs() < 0.005);
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
    fn passive_vehicle_drag_is_consistent_across_frame_rates() {
        let config = VehicleConfig::default();
        let input = VehicleInput::default();
        let mut at_30_fps = VehicleState {
            speed: 18.0,
            ..VehicleState::default()
        };
        let mut at_60_fps = at_30_fps;

        for _ in 0..30 {
            at_30_fps.update(input, VehicleSurface::Asphalt, config, 1.0 / 30.0);
        }
        for _ in 0..60 {
            at_60_fps.update(input, VehicleSurface::Asphalt, config, 1.0 / 60.0);
        }

        assert!((at_30_fps.speed - at_60_fps.speed).abs() < 0.0001);
        assert!((at_30_fps.position.z - at_60_fps.position.z).abs() < 0.1);
    }

    #[test]
    fn coasting_distance_uses_average_speed_during_drag() {
        let config = VehicleConfig::default();
        let mut vehicle = VehicleState {
            speed: 18.0,
            ..VehicleState::default()
        };
        let start_speed = vehicle.speed;
        vehicle.update(
            VehicleInput::default(),
            VehicleSurface::Asphalt,
            config,
            0.1,
        );
        let expected_average = (start_speed + vehicle.speed) * 0.5;
        assert!((vehicle.position.z + expected_average * 0.1).abs() < 0.0001);
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
            f32::midpoint(
                VehicleSurface::Asphalt.traction(),
                VehicleSurface::Snow.traction(),
            ),
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

    #[test]
    fn terrain_tracker_blends_response_after_confirmed_surface_change() {
        let mut tracker = VehicleSurfaceTracker::new(VehicleSurface::Asphalt);
        for _ in 0..VehicleSurfaceTracker::CONFIRMATION_FRAMES {
            tracker.update(VehicleSurface::Snow);
        }

        let start = tracker.response();
        assert!((start.0 - VehicleSurface::Asphalt.traction()).abs() < 0.0001);
        assert_eq!(tracker.current(), VehicleSurface::Snow);

        let mut previous = start.0;
        for _ in 0..VehicleSurfaceTracker::TRANSITION_FRAMES {
            tracker.update(VehicleSurface::Snow);
            let response = tracker.response();
            assert!(response.0 <= previous);
            assert!(response.0 >= VehicleSurface::Snow.traction());
            previous = response.0;
        }

        assert!((tracker.response().0 - VehicleSurface::Snow.traction()).abs() < 0.0001);
    }

    #[test]
    fn terrain_tracker_retargets_from_current_blended_response() {
        let mut tracker = VehicleSurfaceTracker::new(VehicleSurface::Asphalt);
        for _ in 0..VehicleSurfaceTracker::CONFIRMATION_FRAMES {
            tracker.update(VehicleSurface::Snow);
        }
        for _ in 0..6 {
            tracker.update(VehicleSurface::Snow);
        }
        let before_retarget = tracker.response();

        for _ in 0..VehicleSurfaceTracker::CONFIRMATION_FRAMES {
            tracker.update(VehicleSurface::Mud);
        }
        let after_retarget = tracker.response();
        assert!((before_retarget.0 - after_retarget.0).abs() < 0.0001);
        assert!((before_retarget.1 - after_retarget.1).abs() < 0.0001);
        assert!((before_retarget.2 - after_retarget.2).abs() < 0.0001);
    }
}

#[cfg(test)]
mod wheel_traction_tests {
    use super::*;

    #[test]
    fn wheel_contact_grip_is_blended_independently_per_corner() {
        let mut tracker = VehicleSurfaceTracker::new(VehicleSurface::Asphalt);
        let contacts = [
            VehicleSurface::Asphalt,
            VehicleSurface::Snow,
            VehicleSurface::Gravel,
            VehicleSurface::Mud,
        ];

        tracker.update_wheel_contacts(VehicleSurface::Asphalt, contacts);
        let initial = tracker.wheel_traction();
        assert_eq!(initial, [1.0; 4]);

        for _ in 0..19 {
            tracker.update_wheel_contacts(VehicleSurface::Asphalt, contacts);
        }
        let settled = tracker.wheel_traction();

        assert!((settled[0] - VehicleSurface::Asphalt.traction()).abs() < f32::EPSILON);
        assert!((settled[1] - VehicleSurface::Snow.traction()).abs() < f32::EPSILON);
        assert!((settled[2] - VehicleSurface::Gravel.traction()).abs() < f32::EPSILON);
        assert!((settled[3] - VehicleSurface::Mud.traction()).abs() < f32::EPSILON);
        assert!(settled[0] > settled[2]);
        assert!(settled[2] > settled[3]);
        assert!(settled[3] > settled[1]);
    }

    #[test]
    fn four_wheel_handling_rejects_invalid_corner_grip_atomically() {
        let mut vehicle = VehicleState {
            position: Vec3::new(0.0, 0.0, 0.0),
            heading: 0.0,
            speed: 4.0,
        };
        let before = vehicle;
        vehicle.update_with_four_wheel_surface_response(
            VehicleInput {
                throttle: 1.0,
                brake: 0.0,
                steering: 0.5,
            },
            (0.8, 1.0, 1.0),
            [1.0, 0.8, f32::NAN, 0.6],
            VehicleConfig::default(),
            1.0 / 60.0,
        );
        assert_eq!(vehicle, before);
    }

    #[test]
    fn four_wheel_handling_uses_corner_average_for_longitudinal_grip() {
        let mut mixed = VehicleState::default();
        let mut uniform = VehicleState::default();
        let input = VehicleInput {
            throttle: 1.0,
            brake: 0.0,
            steering: 0.0,
        };
        let config = VehicleConfig::default();
        mixed.update_with_four_wheel_surface_response(
            input,
            (1.0, 1.0, 0.8),
            [1.0, 0.6, 0.8, 0.8],
            config,
            1.0 / 60.0,
        );
        uniform.update_with_four_wheel_surface_response(
            input,
            (0.8, 1.0, 0.8),
            [0.8; 4],
            config,
            1.0 / 60.0,
        );
        assert!((mixed.speed - uniform.speed).abs() < 0.00001);
    }
}
