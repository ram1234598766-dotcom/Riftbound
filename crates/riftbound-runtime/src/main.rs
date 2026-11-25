//! Native window lifecycle and application entry point for Riftbound.

use std::sync::Arc;
use std::time::{Duration, Instant};

use riftbound_core::{
    Aabb2, Button, CharacterController, Checkpoint, CheckpointProgress, CollisionWorld,
    EnemyArchetype, EnemyBrain, EnemyConfig, EnemyDifficulty, EnemyEvent, EnemyState, EntityId,
    ExtractionZone, InputState, MaterialId, MeshId, MissionPhase, MissionProgress, ObjectiveKind,
    ObjectiveProgress, Pickup, PickupTracker, PlayerHealth, Scene, SuspensionConfig,
    SuspensionState, Transform3, Vec3, VehicleConfig, VehicleInput, VehicleRecovery, VehicleState,
    terrain_height, world_terrain_sample, world_terrain_vehicle_surface,
};
use riftbound_renderer::{HudData, Renderer};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

const PLAYER_RADIUS: f32 = 0.19;
const PICKUP_POSITIONS: [Vec3; 5] = [
    Vec3::new(-5.0, 0.65, -4.0),
    Vec3::new(5.0, 0.65, -4.0),
    Vec3::new(-5.0, 0.65, 5.0),
    Vec3::new(5.0, 0.65, 5.0),
    Vec3::new(0.0, 0.65, 7.0),
];
const PICKUP_PHASES: [f32; 5] = [0.0, 1.2, 2.4, 3.6, 4.8];

#[derive(Default)]
struct RiftboundApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    player: Option<EntityId>,
    player_parts: Option<CharacterParts>,
    vehicle: Option<EntityId>,
    vehicle_wheels: Option<[EntityId; 4]>,
    vehicle_state: VehicleState,
    vehicle_suspension: SuspensionState,
    vehicle_recovery: VehicleRecovery,
    driving: bool,
    collision: Option<CollisionWorld>,
    input: InputState,
    controller: CharacterController,
    last_frame: Option<Instant>,
    camera_target: Vec3,
    pickups: PickupTracker,
    pickup_entities: Vec<EntityId>,
    animated_transforms: Vec<(EntityId, Transform3)>,
    extraction: Option<ExtractionZone>,
    beacon_entity: Option<EntityId>,
    checkpoint_entity: Option<EntityId>,
    checkpoint: Option<Checkpoint>,
    checkpoint_progress: Option<CheckpointProgress>,
    sentinel_entity: Option<EntityId>,
    sentinel_effect_entity: Option<EntityId>,
    sentinel_brain: Option<EnemyBrain>,
    sentinel_objective: Option<ObjectiveProgress>,
    sentinel_pulse_timer: f32,
    support_enemies: Vec<(EntityId, EntityId, EnemyBrain, f32)>,
    player_health: PlayerHealth,
    animation_time: f32,
    mission: MissionProgress,
    lan_server_started: bool,
    world_streamer: Option<riftbound_core::WorldChunkStreamer>,
    world_slot_map: Option<riftbound_core::WorldChunkSlotMap>,
}

impl ApplicationHandler for RiftboundApp {
    #[allow(clippy::too_many_lines)]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.lan_server_started {
            self.lan_server_started = true;
            if let Err(error) = riftbound_net::spawn() {
                log::error!("Could not start LAN server thread: {error}");
            }
        }
        if self.renderer.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("RIFTBOUND — Native Runtime")
                        .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0)),
                )
                .expect("window creation should succeed"),
        );

        match pollster::block_on(Renderer::new(
            event_loop.owned_display_handle(),
            window.clone(),
        )) {
            Ok(mut renderer) => {
                let mut scene = renderer.scene().clone();
                build_arena(&mut scene);
                let player_transform = Transform3::new(
                    Vec3::new(0.0, PLAYER_RADIUS + 0.01, -4.0),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.38, 0.38, 0.38),
                );
                let player = scene
                    .spawn(player_transform)
                    .expect("player entity ID space available");
                scene.set_material(player, MaterialId::RIFT);
                let player_parts = build_player_character(&mut scene, player);
                let (vehicle, vehicle_wheels) = build_vehicle(&mut scene);
                let vehicle_state = VehicleState {
                    position: Vec3::new(0.0, 0.58, -1.2),
                    ..VehicleState::default()
                };
                let vehicle_suspension = SuspensionState::default();
                let vehicle_recovery = VehicleRecovery::new(vehicle_state);
                if let Some(transform) = scene.get_mut(vehicle) {
                    transform.translation = vehicle_state.position;
                }
                let (pickups, pickup_entities) = build_pickups(&mut scene);
                let (beacon_entity, extraction) = build_extraction_beacon(&mut scene);
                let (checkpoint_entity, checkpoint) = build_checkpoint(&mut scene);
                let (sentinel_entity, sentinel_effect_entity, sentinel_brain) =
                    build_sentinel(&mut scene);
                let mut support_enemies = Vec::with_capacity(2);
                for (home, archetype, material) in [
                    (
                        Vec3::new(-7.0, terrain_height(-7.0, 5.0) + 0.62, 5.0),
                        EnemyArchetype::Skirmisher,
                        MaterialId::RIFT,
                    ),
                    (
                        Vec3::new(7.0, terrain_height(7.0, 5.0) + 0.62, 5.0),
                        EnemyArchetype::Warden,
                        MaterialId::AMBER,
                    ),
                ] {
                    support_enemies.push(build_profiled_enemy(
                        &mut scene,
                        home,
                        archetype,
                        EnemyDifficulty::Standard,
                        material,
                    ));
                }
                if let Err(error) = renderer.set_scene(scene) {
                    log::error!("Could not create the arena scene: {error}");
                    event_loop.exit();
                    return;
                }
                let mut world_streamer = riftbound_core::WorldChunkStreamer::standard();
                let mut world_slot_map = riftbound_core::WorldChunkSlotMap::for_config(
                    riftbound_core::WorldChunkConfig::default(),
                );
                let _ = world_streamer.update_focus(player_transform.translation.x, player_transform.translation.z);
                if !renderer.sync_world_terrain(world_streamer.changes(), &mut world_slot_map) {
                    log::error!("Initial world terrain chunk synchronization failed");
                }
                renderer.follow_camera(player_transform.translation);
                self.camera_target = player_transform.translation;
                log::info!("Riftbound arena, player, and collision world initialized");
                window.request_redraw();
                self.window = Some(window);
                self.renderer = Some(renderer);
                self.world_streamer = Some(world_streamer);
                self.world_slot_map = Some(world_slot_map);
                self.player = Some(player);
                self.player_parts = Some(player_parts);
                self.vehicle = Some(vehicle);
                self.vehicle_wheels = Some(vehicle_wheels);
                self.vehicle_state = vehicle_state;
                self.vehicle_suspension = vehicle_suspension;
                self.vehicle_recovery = vehicle_recovery;
                self.driving = false;
                self.collision = Some(arena_collision_world());
                self.pickups = pickups;
                self.mission = MissionProgress::new(self.pickups.total());
                self.pickup_entities = pickup_entities;
                self.animated_transforms = Vec::with_capacity(8 + self.pickup_entities.len());
                self.extraction = Some(extraction);
                self.beacon_entity = Some(beacon_entity);
                self.checkpoint_entity = Some(checkpoint_entity);
                self.checkpoint = Some(checkpoint);
                self.checkpoint_progress = Some(
                    CheckpointProgress::new(player_transform.translation)
                        .expect("player start position is valid"),
                );
                self.sentinel_entity = Some(sentinel_entity);
                self.sentinel_effect_entity = Some(sentinel_effect_entity);
                self.sentinel_brain = Some(sentinel_brain);
                self.sentinel_objective =
                    Some(ObjectiveProgress::new(ObjectiveKind::Defeat { target: 1 }));
                self.sentinel_pulse_timer = 0.0;
                self.support_enemies = support_enemies;
                self.animation_time = 0.0;
                self.last_frame = Some(Instant::now());
            }
            Err(error) => {
                log::error!("Could not initialize the GPU renderer: {error}");
                event_loop.exit();
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.renderer = None;
        self.window = None;
        self.player = None;
        self.player_parts = None;
        self.vehicle = None;
        self.vehicle_wheels = None;
        self.vehicle_state = VehicleState::default();
        self.vehicle_suspension = SuspensionState::default();
        self.vehicle_recovery = VehicleRecovery::default();
        self.driving = false;
        self.collision = None;
        self.pickups = PickupTracker::new();
        self.pickup_entities.clear();
        self.animated_transforms.clear();
        self.extraction = None;
        self.beacon_entity = None;
        self.checkpoint_entity = None;
        self.checkpoint = None;
        self.checkpoint_progress = None;
        self.sentinel_entity = None;
        self.sentinel_effect_entity = None;
        self.sentinel_brain = None;
        self.sentinel_objective = None;
        self.sentinel_pulse_timer = 0.0;
        self.support_enemies.clear();
        self.player_health = PlayerHealth::default();
        self.animation_time = 0.0;
        self.mission = MissionProgress::new(0);
        self.last_frame = None;
        self.camera_target = Vec3::new(0.0, 0.0, 0.0);
        self.controller = CharacterController::default();
        self.input = InputState::default();
    }

    #[allow(clippy::too_many_lines)]
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
                if size.width > 0 && size.height > 0 {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                {
                    event_loop.exit();
                    return;
                }
                if let PhysicalKey::Code(key) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        update_camera_controls(self.renderer.as_mut(), key);
                    }
                    if event.state == ElementState::Pressed && key == KeyCode::KeyV {
                        self.driving = !self.driving;
                        if self.driving {
                            log::info!(
                                "Driving prototype active: W/S throttle, A/D steering, Space brake, V exit"
                            );
                        } else {
                            log::info!("Exited vehicle prototype");
                        }
                    }
                    if let Some(button) = movement_button(key) {
                        self.input
                            .set_button(button, event.state == ElementState::Pressed);
                    }
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let delta = self
                    .last_frame
                    .replace(now)
                    .map_or(0.0, |previous| now.duration_since(previous).as_secs_f32());
                self.animation_time += delta.min(0.1);
                self.player_health.update(delta);
                if let (Some(renderer), Some(player), Some(collision)) =
                    (self.renderer.as_mut(), self.player, self.collision.as_ref())
                {
                    let Some(mut transform) = renderer.scene().get(player).copied() else {
                        log::error!("The player entity is no longer in the active scene");
                        event_loop.exit();
                        return;
                    };
                    let previous_position = transform.translation;
                    if !self.driving {
                        self.controller.update_camera_relative(
                            &mut transform,
                            &self.input,
                            delta,
                            renderer.camera_yaw(),
                        );
                    }
                    let horizontal_velocity = if self.driving {
                        Vec3::default()
                    } else {
                        self.controller.velocity()
                    };
                    if !self.driving
                        && horizontal_velocity.x * horizontal_velocity.x
                            + horizontal_velocity.z * horizontal_velocity.z
                            > 0.01
                    {
                        transform.rotation_radians.y =
                            horizontal_velocity.x.atan2(-horizontal_velocity.z);
                    }
                    let displacement = Vec3::new(
                        transform.translation.x - previous_position.x,
                        0.0,
                        transform.translation.z - previous_position.z,
                    );
                    let resolved =
                        collision.move_character(previous_position, displacement, PLAYER_RADIUS);
                    transform.translation.x = resolved.position.x;
                    transform.translation.z = resolved.position.z;
                    let floor_height = terrain_height(resolved.position.x, resolved.position.z)
                        + PLAYER_RADIUS
                        + 0.01;
                    if !self.driving {
                        self.controller.update_vertical(
                            &mut transform,
                            &self.input,
                            delta,
                            floor_height,
                        );
                        self.controller
                            .resolve_collision(resolved.blocked_x, resolved.blocked_z);
                    }
                    if transform.translation.y < -3.0 {
                        if let Some(progress) = self.checkpoint_progress {
                            transform.translation = progress.respawn_position();
                            self.controller = CharacterController::default();
                            log::info!("Player recovered at the latest active checkpoint");
                        }
                    }
                    self.animated_transforms.clear();
                    self.animated_transforms.push((player, transform));
                    if self.driving {
                        let throttle = if self.input.is_held(Button::MoveForward) {
                            1.0
                        } else if self.input.is_held(Button::MoveBackward) {
                            -1.0
                        } else {
                            0.0
                        };
                        let steering = if self.input.is_held(Button::MoveLeft) {
                            -1.0
                        } else if self.input.is_held(Button::MoveRight) {
                            1.0
                        } else {
                            0.0
                        };
                        let vehicle_previous_position = self.vehicle_state.position;
                        self.vehicle_state.update(
                            VehicleInput {
                                throttle,
                                brake: if self.input.is_held(Button::Jump) {
                                    1.0
                                } else {
                                    0.0
                                },
                                steering,
                            },
                            world_terrain_vehicle_surface(
                                self.vehicle_state.position.x,
                                self.vehicle_state.position.z,
                                self.vehicle_state.heading,
                                0.78,
                                1.32,
                            ),
                            VehicleConfig::default(),
                            delta,
                        );
                        if let Some(collision) = self.collision.as_ref() {
                            let movement = Vec3::new(
                                self.vehicle_state.position.x - vehicle_previous_position.x,
                                0.0,
                                self.vehicle_state.position.z - vehicle_previous_position.z,
                            );
                            let resolved = collision.move_vehicle_oriented(
                                vehicle_previous_position,
                                movement,
                                self.vehicle_state.heading,
                                0.82,
                                1.38,
                            );
                            self.vehicle_state.position.x = resolved.position.x;
                            self.vehicle_state.position.z = resolved.position.z;
                            if resolved.blocked_x || resolved.blocked_z {
                                self.vehicle_state.speed = 0.0;
                            }
                        }
                        if self
                            .vehicle_recovery
                            .recover_if_fallen(&mut self.vehicle_state, -3.0)
                        {
                            self.vehicle_suspension = SuspensionState::default();
                            log::info!("Vehicle recovered at its latest safe terrain position");
                        }
                        // Sample the same deterministic terrain function used by the arena.
                        // Corner order matches SuspensionState: front-left, front-right,
                        // rear-left, rear-right.
                        let wheel_offsets = [
                            (-0.78_f32, -1.32_f32),
                            (0.78, -1.32),
                            (-0.78, 1.32),
                            (0.78, 1.32),
                        ];
                        let (sin_heading, cos_heading) = self.vehicle_state.heading.sin_cos();
                        let mut ground_heights = [0.0_f32; 4];
                        for (index, (local_x, local_z)) in wheel_offsets.iter().enumerate() {
                            let world_x = self.vehicle_state.position.x
                                + cos_heading * local_x
                                + sin_heading * local_z;
                            let world_z = self.vehicle_state.position.z - sin_heading * local_x
                                + cos_heading * local_z;
                            ground_heights[index] = world_terrain_sample(world_x, world_z).height;
                        }
                        let suspension_config = SuspensionConfig::default();
                        let suspension_pose = self.vehicle_suspension.update(
                            ground_heights,
                            self.vehicle_state.position.y,
                            suspension_config,
                            delta,
                        );
                        let average_ground = ground_heights.iter().sum::<f32>() * 0.25;
                        self.vehicle_state.position.y = average_ground
                            + suspension_config.rest_length
                            - suspension_pose.average_compression;
                        let _ = self.vehicle_recovery.record_safe_state(
                            self.vehicle_state,
                            average_ground,
                            2.5,
                        );
                        let vehicle_transform = Transform3::new(
                            self.vehicle_state.position,
                            Vec3::new(
                                suspension_pose.pitch,
                                self.vehicle_state.heading,
                                suspension_pose.roll,
                            ),
                            Vec3::new(1.0, 1.0, 1.0),
                        );
                        if let Some(vehicle) = self.vehicle {
                            self.animated_transforms.push((vehicle, vehicle_transform));
                        }
                        if let Some(wheels) = self.vehicle_wheels {
                            let wheel_spin = self.vehicle_state.speed * delta * 1.7;
                            for (index, wheel) in wheels.iter().enumerate() {
                                let front = index < 2;
                                let steer = if front { steering * 0.35 } else { 0.0 };
                                let local_y =
                                    ground_heights[index] + 0.34 - self.vehicle_state.position.y;
                                let (local_x, local_z) = wheel_offsets[index];
                                self.animated_transforms.push((
                                    *wheel,
                                    Transform3::new(
                                        Vec3::new(local_x, local_y, local_z),
                                        Vec3::new(wheel_spin, steer, std::f32::consts::FRAC_PI_2),
                                        Vec3::new(0.34, 0.68, 0.68),
                                    ),
                                ));
                            }
                        }
                    }
                    if let Some(parts) = self.player_parts {
                        let movement = ((horizontal_velocity.x * horizontal_velocity.x
                            + horizontal_velocity.z * horizontal_velocity.z)
                            .sqrt()
                            / 4.5)
                            .clamp(0.0, 1.0);
                        let gait = self.animation_time * 8.0;
                        let leg_swing = gait.sin() * 0.72 * movement;
                        let arm_swing = -leg_swing * 0.78;
                        let bob = gait.mul_add(2.0, 0.0).sin() * 0.045 * movement;
                        self.animated_transforms.extend([
                            (
                                parts.torso,
                                character_part_transform(
                                    0.0,
                                    0.48 + bob,
                                    0.0,
                                    0.0,
                                    0.0,
                                    0.0,
                                    0.58,
                                    0.72,
                                    0.34,
                                ),
                            ),
                            (
                                parts.head,
                                character_part_transform(
                                    0.0,
                                    1.15 + bob,
                                    0.0,
                                    0.0,
                                    0.0,
                                    0.0,
                                    0.38,
                                    0.38,
                                    0.38,
                                ),
                            ),
                            (
                                parts.left_arm,
                                character_part_transform(
                                    -0.43,
                                    0.48 + bob,
                                    0.0,
                                    arm_swing,
                                    0.0,
                                    -0.08,
                                    0.20,
                                    0.62,
                                    0.22,
                                ),
                            ),
                            (
                                parts.right_arm,
                                character_part_transform(
                                    0.43,
                                    0.48 + bob,
                                    0.0,
                                    -arm_swing,
                                    0.0,
                                    0.08,
                                    0.20,
                                    0.62,
                                    0.22,
                                ),
                            ),
                            (
                                parts.left_leg,
                                character_part_transform(
                                    -0.19, -0.32, 0.0, leg_swing, 0.0, 0.0, 0.22, 0.62, 0.26,
                                ),
                            ),
                            (
                                parts.right_leg,
                                character_part_transform(
                                    0.19, -0.32, 0.0, -leg_swing, 0.0, 0.0, 0.22, 0.62, 0.26,
                                ),
                            ),
                        ]);
                    }

                    for (index, entity) in self.pickup_entities.iter().enumerate() {
                        if renderer.scene().get(*entity).is_none() {
                            continue;
                        }
                        let base = PICKUP_POSITIONS[index];
                        let phase = self.animation_time * 2.4 + PICKUP_PHASES[index];
                        let animated = Transform3::new(
                            Vec3::new(base.x, base.y + phase.sin() * 0.14, base.z),
                            Vec3::new(0.0, self.animation_time * 1.35 + PICKUP_PHASES[index], 0.0),
                            Vec3::new(0.28, 0.42, 0.28),
                        );
                        self.animated_transforms.push((*entity, animated));
                    }
                    if let Some(beacon) = self.beacon_entity {
                        let pulse = 1.0 + (self.animation_time * 2.0).sin() * 0.06;
                        self.animated_transforms.push((
                            beacon,
                            Transform3::new(
                                Vec3::new(0.0, 0.55, -8.0),
                                Vec3::new(0.0, self.animation_time * 0.45, 0.0),
                                Vec3::new(0.62 * pulse, 0.78 * pulse, 0.62 * pulse),
                            ),
                        ));
                    }
                    if let Some(checkpoint_entity) = self.checkpoint_entity {
                        let pulse = 1.0 + (self.animation_time * 2.6).sin() * 0.08;
                        let y = terrain_height(0.0, 2.0) + 0.62;
                        self.animated_transforms.push((
                            checkpoint_entity,
                            Transform3::new(
                                Vec3::new(0.0, y, 2.0),
                                Vec3::new(0.0, -self.animation_time * 0.6, 0.0),
                                Vec3::new(0.48 * pulse, 0.58 * pulse, 0.48 * pulse),
                            ),
                        ));
                    }
                    if let (Some(sentinel), Some(effect), Some(brain)) = (
                        self.sentinel_entity,
                        self.sentinel_effect_entity,
                        self.sentinel_brain.as_mut(),
                    ) {
                        if let Some(mut enemy_transform) = renderer.scene().get(sentinel).copied() {
                            let event = brain.update(
                                &mut enemy_transform.translation,
                                transform.translation,
                                delta,
                            );
                            let enemy_position = enemy_transform.translation;
                            enemy_transform.translation.y =
                                terrain_height(enemy_position.x, enemy_position.z) + 0.62;
                            let dx = transform.translation.x - enemy_position.x;
                            let dz = transform.translation.z - enemy_position.z;
                            enemy_transform.rotation_radians.y = dx.atan2(-dz);
                            let telegraph = brain.state() == EnemyState::Telegraph;
                            let body_pulse = if telegraph {
                                1.0 + (self.animation_time * 18.0).sin() * 0.12
                            } else {
                                1.0 + (self.animation_time * 2.2).sin() * 0.035
                            };
                            enemy_transform.scale =
                                Vec3::new(0.58 * body_pulse, 0.78 * body_pulse, 0.58 * body_pulse);
                            self.animated_transforms.push((sentinel, enemy_transform));
                            if event == Some(EnemyEvent::PulseReleased) {
                                self.sentinel_pulse_timer = 0.32;
                                let pulse_distance =
                                    distance_squared(transform.translation, enemy_position);
                                if pulse_distance <= 2.2 * 2.2
                                    && self.player_health.apply_damage(25, 1.0)
                                {
                                    log::info!("Sentinel pulse hit the player");
                                    if !self.player_health.is_alive() {
                                        if let Some(progress) = self.checkpoint_progress {
                                            transform.translation = progress.respawn_position();
                                        }
                                        self.controller = CharacterController::default();
                                        self.player_health.restore();
                                        self.animated_transforms.push((player, transform));
                                        log::info!("Player recovered at the active checkpoint");
                                    }
                                } else {
                                    log::info!(
                                        "Sentinel pulse missed or was blocked by damage protection"
                                    );
                                }
                                log::info!("Sentinel released its pulse; close in during recovery");
                            }
                            self.sentinel_pulse_timer =
                                (self.sentinel_pulse_timer - delta.min(0.1)).max(0.0);
                            let effect_scale = if telegraph {
                                0.78 + (self.animation_time * 18.0).sin().abs() * 0.16
                            } else if self.sentinel_pulse_timer > 0.0 {
                                0.95 + (0.32 - self.sentinel_pulse_timer) * 3.2
                            } else {
                                0.01
                            };
                            let effect_y =
                                terrain_height(enemy_position.x, enemy_position.z) + 0.12;
                            self.animated_transforms.push((
                                effect,
                                Transform3::new(
                                    Vec3::new(enemy_position.x, effect_y, enemy_position.z),
                                    Vec3::new(0.0, self.animation_time * 1.8, 0.0),
                                    Vec3::new(effect_scale, 0.07, effect_scale),
                                ),
                            ));
                            match event {
                                Some(EnemyEvent::PlayerDetected) => {
                                    log::info!("Arena sentinel detected the player");
                                }
                                Some(EnemyEvent::AttackTelegraphed) => {
                                    log::info!("Sentinel attack telegraphed — prepare to evade");
                                }
                                Some(EnemyEvent::ReturningHome) => {
                                    log::info!("Sentinel lost the player and is returning home");
                                }
                                Some(EnemyEvent::PulseReleased) | None => {}
                            }
                        }
                    }
                    for (enemy, effect, brain, pulse_timer) in &mut self.support_enemies {
                        let Some(mut enemy_transform) = renderer.scene().get(*enemy).copied()
                        else {
                            continue;
                        };
                        let event = brain.update(
                            &mut enemy_transform.translation,
                            transform.translation,
                            delta,
                        );
                        let enemy_position = enemy_transform.translation;
                        enemy_transform.translation.y =
                            terrain_height(enemy_position.x, enemy_position.z) + 0.62;
                        let dx = transform.translation.x - enemy_position.x;
                        let dz = transform.translation.z - enemy_position.z;
                        enemy_transform.rotation_radians.y = dx.atan2(-dz);
                        let telegraph = brain.state() == EnemyState::Telegraph;
                        let pulse = if telegraph {
                            1.0 + (self.animation_time * 17.0).sin() * 0.1
                        } else {
                            1.0 + (self.animation_time * 1.8).sin() * 0.03
                        };
                        enemy_transform.scale = Vec3::new(0.5 * pulse, 0.68 * pulse, 0.5 * pulse);
                        self.animated_transforms.push((*enemy, enemy_transform));
                        if event == Some(EnemyEvent::PulseReleased) {
                            *pulse_timer = 0.28;
                            if distance_squared(transform.translation, enemy_position) <= 2.2 * 2.2
                                && self.player_health.apply_damage(15, 1.0)
                            {
                                log::info!("An arena enemy pulse hit the player");
                                if !self.player_health.is_alive() {
                                    if let Some(progress) = self.checkpoint_progress {
                                        transform.translation = progress.respawn_position();
                                    }
                                    self.controller = CharacterController::default();
                                    self.player_health.restore();
                                    self.animated_transforms.push((player, transform));
                                    log::info!("Player recovered at the active checkpoint");
                                }
                            }
                        }
                        *pulse_timer = (*pulse_timer - delta.min(0.1)).max(0.0);
                        let effect_scale = if telegraph {
                            0.72 + (self.animation_time * 17.0).sin().abs() * 0.14
                        } else if *pulse_timer > 0.0 {
                            0.9 + (0.28 - *pulse_timer) * 3.0
                        } else {
                            0.01
                        };
                        self.animated_transforms.push((
                            *effect,
                            Transform3::new(
                                Vec3::new(
                                    enemy_position.x,
                                    terrain_height(enemy_position.x, enemy_position.z) + 0.12,
                                    enemy_position.z,
                                ),
                                Vec3::new(0.0, self.animation_time * 1.6, 0.0),
                                Vec3::new(effect_scale, 0.06, effect_scale),
                            ),
                        ));
                    }
                    match renderer.set_object_transforms(&self.animated_transforms) {
                        Ok(true) => {
                            let camera_blend = 1.0 - (-8.0 * delta.min(0.1)).exp();
                            let camera_position = if self.driving {
                                self.vehicle_state.position
                            } else {
                                transform.translation
                            };
                            self.camera_target.x +=
                                (camera_position.x - self.camera_target.x) * camera_blend;
                            self.camera_target.y +=
                                (camera_position.y - self.camera_target.y) * camera_blend;
                            self.camera_target.z +=
                                (camera_position.z - self.camera_target.z) * camera_blend;
                            renderer.follow_camera(self.camera_target);
                            let collected = self.pickups.collect_nearby(
                                transform.translation,
                                self.input.was_pressed(Button::Interact),
                            );
                            let collected_count = collected.len();
                            if !collected.is_empty() {
                                let mut updated_scene = renderer.scene().clone();
                                for index in collected {
                                    if let Some(entity) = self.pickup_entities.get(index) {
                                        updated_scene.despawn(*entity);
                                    }
                                }
                                if let Err(error) = renderer.set_scene(updated_scene) {
                                    log::error!("Could not update collected pickups: {error}");
                                    event_loop.exit();
                                    return;
                                }
                                let unlocked = self.mission.record_collected(collected_count);
                                let progress = if self.mission.extraction_unlocked() {
                                    format!(
                                        "RIFTBOUND — Return to extraction beacon ({}/{})",
                                        self.mission.collected(),
                                        self.mission.total(),
                                    )
                                } else {
                                    format!(
                                        "RIFTBOUND — Shards: {}/{}",
                                        self.mission.collected(),
                                        self.mission.total(),
                                    )
                                };
                                window.set_title(&progress);
                                log::info!("{progress}");
                                if unlocked {
                                    log::info!(
                                        "All rift shards recovered; extraction is now unlocked"
                                    );
                                }
                            }
                            let interact_pressed = self.input.was_pressed(Button::Interact);
                            let checkpoint_activated =
                                match (self.checkpoint, self.checkpoint_progress.as_mut()) {
                                    (Some(checkpoint), Some(progress)) => progress.activate(
                                        checkpoint,
                                        transform.translation,
                                        interact_pressed,
                                    ),
                                    _ => false,
                                };
                            if checkpoint_activated {
                                window.set_title("RIFTBOUND — Checkpoint activated");
                                log::info!(
                                    "Checkpoint activated; falling will return the player here"
                                );
                            }
                            let sentinel_ready = self
                                .sentinel_brain
                                .is_some_and(|brain| brain.state() == EnemyState::Recover);
                            let sentinel_in_range = self.sentinel_entity.is_some_and(|entity| {
                                renderer.scene().get(entity).is_some_and(|enemy| {
                                    distance_squared(transform.translation, enemy.translation)
                                        <= 2.0 * 2.0
                                })
                            });
                            if interact_pressed && sentinel_ready && sentinel_in_range {
                                let mut updated_scene = renderer.scene().clone();
                                if let Some(entity) = self.sentinel_entity {
                                    updated_scene.despawn(entity);
                                }
                                if let Some(effect) = self.sentinel_effect_entity {
                                    updated_scene.despawn(effect);
                                }
                                if let Err(error) = renderer.set_scene(updated_scene) {
                                    log::error!("Could not disable the sentinel: {error}");
                                    event_loop.exit();
                                    return;
                                }
                                self.sentinel_entity = None;
                                self.sentinel_effect_entity = None;
                                self.sentinel_brain = None;
                                if let Some(objective) = self.sentinel_objective.as_mut() {
                                    objective.advance(1);
                                }
                                window.set_title("RIFTBOUND — Sentinel disabled");
                                log::info!(
                                    "Sentinel encounter complete; defeat objective advanced"
                                );
                            }
                            let inside_extraction = self.extraction.is_some_and(|zone| {
                                zone.can_extract(transform.translation, true, true)
                            });
                            if self
                                .mission
                                .try_extract(interact_pressed, inside_extraction)
                            {
                                window.set_title("RIFTBOUND — Mission complete!");
                                log::info!(
                                    "Shardfall mission completed: all shards recovered and extraction reached"
                                );
                            }

                            let near_pickup =
                                PICKUP_POSITIONS
                                    .iter()
                                    .enumerate()
                                    .any(|(index, position)| {
                                        !self.mission.extraction_unlocked()
                                            && distance_squared(transform.translation, *position)
                                                <= 1.7 * 1.7
                                            && self.pickup_entities.get(index).is_some_and(|id| {
                                                renderer.scene().get(*id).is_some()
                                            })
                                    });
                            let near_beacon =
                                distance_squared(transform.translation, Vec3::new(0.0, 0.55, -8.0))
                                    <= 1.9 * 1.9;
                            let near_sentinel = self.sentinel_entity.is_some_and(|entity| {
                                renderer.scene().get(entity).is_some_and(|enemy| {
                                    distance_squared(transform.translation, enemy.translation)
                                        <= 2.0 * 2.0
                                })
                            });
                            let sentinel_can_be_disabled = near_sentinel
                                && self
                                    .sentinel_brain
                                    .is_some_and(|brain| brain.state() == EnemyState::Recover);
                            let near_checkpoint = self.checkpoint.is_some_and(|checkpoint| {
                                !self
                                    .checkpoint_progress
                                    .is_some_and(CheckpointProgress::has_active_checkpoint)
                                    && distance_squared(
                                        transform.translation,
                                        checkpoint.position(),
                                    ) <= 1.7 * 1.7
                            });
                            let status = if self.mission.phase() == MissionPhase::Complete {
                                "MISSION COMPLETE!"
                            } else if self.sentinel_entity.is_some() {
                                "DEFEAT SENTINEL • COLLECT SHARDS"
                            } else if self.mission.extraction_unlocked() {
                                "RETURN TO BEACON"
                            } else {
                                "COLLECT RIFT SHARDS"
                            };
                            let prompt = if self.mission.is_complete() {
                                None
                            } else if sentinel_can_be_disabled {
                                Some(String::from("F - DISABLE SENTINEL"))
                            } else if near_sentinel {
                                Some(String::from("SENTINEL LOCKING ON"))
                            } else if self.mission.extraction_unlocked() && near_beacon {
                                Some(String::from("F - EXTRACT"))
                            } else if near_checkpoint {
                                Some(String::from("F - ACTIVATE CHECKPOINT"))
                            } else if !self.mission.extraction_unlocked() && near_pickup {
                                Some(String::from("F - COLLECT"))
                            } else {
                                None
                            };
                            renderer.set_hud(HudData {
                                collected: self.pickups.collected(),
                                total: self.pickups.total(),
                                status: if self.driving { String::from("DRIVING PROTOTYPE • W/S DRIVE • A/D STEER • SPACE BRAKE • V EXIT") } else { String::from(status) },
                                prompt,
                                health: self.player_health.current(),
                                max_health: self.player_health.maximum(),
                            });
                        }
                        Ok(false) => {
                            log::error!("The player entity could not be updated");
                            event_loop.exit();
                            return;
                        }
                        Err(error) => {
                            log::error!("Could not rebuild player render data: {error}");
                            event_loop.exit();
                            return;
                        }
                    }
                    let world_focus = if self.driving {
                        self.vehicle_state.position
                    } else {
                        renderer.scene().get(player).map_or(self.camera_target, |transform| transform.translation)
                    };
                    if let (Some(streamer), Some(slot_map)) =
                        (self.world_streamer.as_mut(), self.world_slot_map.as_mut())
                    {
                        if streamer.update_focus(world_focus.x, world_focus.z)
                            && !renderer.sync_world_terrain(streamer.changes(), slot_map)
                        {
                            log::error!("World terrain chunk synchronization failed");
                        }
                    }
                    renderer.render();
                }
                self.input.begin_frame();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

#[derive(Clone, Copy)]
struct CharacterParts {
    torso: EntityId,
    head: EntityId,
    left_arm: EntityId,
    right_arm: EntityId,
    left_leg: EntityId,
    right_leg: EntityId,
}

// This convenience constructor mirrors Transform3's three vector groups as scalar arguments.
#[allow(clippy::too_many_arguments)]
fn character_part_transform(
    x: f32,
    y: f32,
    z: f32,
    rotation_x: f32,
    rotation_y: f32,
    rotation_z: f32,
    scale_x: f32,
    scale_y: f32,
    scale_z: f32,
) -> Transform3 {
    Transform3::new(
        Vec3::new(x, y, z),
        Vec3::new(rotation_x, rotation_y, rotation_z),
        Vec3::new(scale_x, scale_y, scale_z),
    )
}

fn spawn_character_part(
    scene: &mut Scene,
    player: EntityId,
    transform: Transform3,
    material: MaterialId,
) -> EntityId {
    let id = scene
        .spawn(transform)
        .expect("character part entity ID space available");
    assert!(scene.set_parent(id, Some(player)));
    assert!(scene.set_material(id, material));
    id
}

fn build_player_character(scene: &mut Scene, player: EntityId) -> CharacterParts {
    // Hide the original root cube; the visible explorer is assembled from parented parts.
    if let Some(root) = scene.get_mut(player) {
        root.scale = Vec3::new(0.01, 0.01, 0.01);
    }

    let torso = spawn_character_part(
        scene,
        player,
        character_part_transform(0.0, 0.48, 0.0, 0.0, 0.0, 0.0, 0.58, 0.72, 0.34),
        MaterialId::RIFT,
    );
    let head = spawn_character_part(
        scene,
        player,
        character_part_transform(0.0, 1.15, 0.0, 0.0, 0.0, 0.0, 0.38, 0.38, 0.38),
        MaterialId::DEFAULT,
    );
    let left_arm = spawn_character_part(
        scene,
        player,
        character_part_transform(-0.43, 0.48, 0.0, 0.0, 0.0, -0.08, 0.20, 0.62, 0.22),
        MaterialId::DEFAULT,
    );
    let right_arm = spawn_character_part(
        scene,
        player,
        character_part_transform(0.43, 0.48, 0.0, 0.0, 0.0, 0.08, 0.20, 0.62, 0.22),
        MaterialId::DEFAULT,
    );
    let left_leg = spawn_character_part(
        scene,
        player,
        character_part_transform(-0.19, -0.32, 0.0, 0.0, 0.0, 0.0, 0.22, 0.62, 0.26),
        MaterialId::AMBER,
    );
    let right_leg = spawn_character_part(
        scene,
        player,
        character_part_transform(0.19, -0.32, 0.0, 0.0, 0.0, 0.0, 0.22, 0.62, 0.26),
        MaterialId::AMBER,
    );
    let _backpack = spawn_character_part(
        scene,
        player,
        character_part_transform(0.0, 0.48, 0.30, 0.0, 0.0, 0.0, 0.34, 0.48, 0.18),
        MaterialId::AMBER,
    );
    let _visor = spawn_character_part(
        scene,
        player,
        character_part_transform(0.0, 1.18, -0.17, 0.0, 0.0, 0.0, 0.27, 0.12, 0.08),
        MaterialId::RIFT,
    );
    CharacterParts {
        torso,
        head,
        left_arm,
        right_arm,
        left_leg,
        right_leg,
    }
}

fn distance_squared(a: Vec3, b: Vec3) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = a.z - b.z;
    dx * dx + dy * dy + dz * dz
}

fn build_extraction_beacon(scene: &mut Scene) -> (EntityId, ExtractionZone) {
    let position = Vec3::new(0.0, 0.55, -8.0);
    let transform = Transform3::new(
        position,
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.62, 0.78, 0.62),
    );
    let id = scene
        .spawn_with_mesh(transform, MeshId::PYRAMID)
        .expect("extraction beacon entity ID space available");
    scene.set_material(id, MaterialId::AMBER);
    (
        id,
        ExtractionZone::new(position, 1.9).expect("extraction zone data is valid"),
    )
}

fn build_profiled_enemy(
    scene: &mut Scene,
    home: Vec3,
    archetype: EnemyArchetype,
    difficulty: EnemyDifficulty,
    material: MaterialId,
) -> (EntityId, EntityId, EnemyBrain, f32) {
    let entity = scene
        .spawn_with_mesh(
            Transform3::new(home, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.5, 0.68, 0.5)),
            MeshId::CRYSTAL,
        )
        .expect("enemy entity ID space available");
    scene.set_material(entity, material);
    let effect = scene
        .spawn_with_mesh(
            Transform3::new(
                Vec3::new(home.x, terrain_height(home.x, home.z) + 0.12, home.z),
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.01, 0.06, 0.01),
            ),
            MeshId::PYRAMID,
        )
        .expect("enemy effect entity ID space available");
    scene.set_material(effect, material);
    (
        entity,
        effect,
        EnemyBrain::new(home, EnemyConfig::for_profile(archetype, difficulty)),
        0.0,
    )
}

fn build_sentinel(scene: &mut Scene) -> (EntityId, EntityId, EnemyBrain) {
    let home = Vec3::new(-5.0, terrain_height(-5.0, 0.0) + 0.62, 0.0);
    let sentinel = scene
        .spawn_with_mesh(
            Transform3::new(home, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.58, 0.78, 0.58)),
            MeshId::CRYSTAL,
        )
        .expect("sentinel entity ID space available");
    scene.set_material(sentinel, MaterialId::AMBER);
    let effect = scene
        .spawn_with_mesh(
            Transform3::new(
                Vec3::new(home.x, terrain_height(home.x, home.z) + 0.12, home.z),
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.01, 0.07, 0.01),
            ),
            MeshId::PYRAMID,
        )
        .expect("sentinel effect entity ID space available");
    scene.set_material(effect, MaterialId::RIFT);
    (
        sentinel,
        effect,
        EnemyBrain::new(
            home,
            EnemyConfig::for_profile(EnemyArchetype::Sentinel, EnemyDifficulty::Standard),
        ),
    )
}

fn build_checkpoint(scene: &mut Scene) -> (EntityId, Checkpoint) {
    let position = Vec3::new(0.0, terrain_height(0.0, 2.0) + PLAYER_RADIUS + 0.01, 2.0);
    let visual_position = Vec3::new(0.0, terrain_height(0.0, 2.0) + 0.62, 2.0);
    let transform = Transform3::new(
        visual_position,
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.48, 0.58, 0.48),
    );
    let id = scene
        .spawn_with_mesh(transform, MeshId::CRYSTAL)
        .expect("checkpoint entity ID space available");
    scene.set_material(id, MaterialId::RIFT);
    (
        id,
        Checkpoint::new(position, 1.7).expect("checkpoint zone data is valid"),
    )
}

fn build_pickups(scene: &mut Scene) -> (PickupTracker, Vec<EntityId>) {
    let positions = [
        Vec3::new(-5.0, 0.65, -4.0),
        Vec3::new(5.0, 0.65, -4.0),
        Vec3::new(-5.0, 0.65, 5.0),
        Vec3::new(5.0, 0.65, 5.0),
        Vec3::new(0.0, 0.65, 7.0),
    ];
    let mut tracker = PickupTracker::new();
    let mut entities = Vec::with_capacity(positions.len());
    for position in positions {
        tracker.add(Pickup::new(position, 1.7).expect("pickup data is valid"));
        let transform = Transform3::new(
            position,
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.28, 0.42, 0.28),
        );
        let id = scene
            .spawn_with_mesh(transform, MeshId::PYRAMID)
            .expect("pickup entity ID space available");
        scene.set_material(id, MaterialId::RIFT);
        entities.push(id);
    }
    (tracker, entities)
}

fn spawn_box(scene: &mut Scene, position: Vec3, scale: Vec3, material: MaterialId) {
    let transform = Transform3::new(position, Vec3::new(0.0, 0.0, 0.0), scale);
    let id = scene
        .spawn(transform)
        .expect("arena entity ID space available");
    scene.set_material(id, material);
}

// Keep the arena layout together so its geometry can be tuned as one authored level.
#[allow(clippy::too_many_lines)]
fn build_arena(scene: &mut Scene) {
    // Four independently culled quadrants cover the same 20x20 walkable arena.
    for (mesh, center_x, center_z) in [
        (MeshId::TERRAIN_NW, -5.0, -5.0),
        (MeshId::TERRAIN_NE, 5.0, -5.0),
        (MeshId::TERRAIN_SW, -5.0, 5.0),
        (MeshId::TERRAIN_SE, 5.0, 5.0),
    ] {
        let terrain = Transform3::new(
            Vec3::new(center_x, 0.0, center_z),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(5.0, 1.0, 5.0),
        );
        if let Some(ground) = scene.spawn_with_mesh(terrain, mesh) {
            scene.set_material(ground, MaterialId::DEFAULT);
        }
    }

    // Four perimeter walls visually match the collision world's inner boundary.
    spawn_box(
        scene,
        Vec3::new(-9.75, 1.0, 0.0),
        Vec3::new(0.5, 2.0, 20.5),
        MaterialId::RIFT,
    );
    spawn_box(
        scene,
        Vec3::new(9.75, 1.0, 0.0),
        Vec3::new(0.5, 2.0, 20.5),
        MaterialId::RIFT,
    );
    spawn_box(
        scene,
        Vec3::new(0.0, 1.0, -9.75),
        Vec3::new(20.0, 2.0, 0.5),
        MaterialId::RIFT,
    );
    spawn_box(
        scene,
        Vec3::new(0.0, 1.0, 9.75),
        Vec3::new(20.0, 2.0, 0.5),
        MaterialId::RIFT,
    );

    // Solid amber blocks are both visible and registered in the collision world.
    spawn_box(
        scene,
        Vec3::new(-2.0, 0.75, -1.0),
        Vec3::new(1.5, 1.5, 1.5),
        MaterialId::AMBER,
    );
    spawn_box(
        scene,
        Vec3::new(2.5, 0.75, 1.5),
        Vec3::new(1.5, 1.5, 1.5),
        MaterialId::AMBER,
    );
    spawn_box(
        scene,
        Vec3::new(0.0, 0.75, 4.0),
        Vec3::new(2.0, 1.5, 1.5),
        MaterialId::AMBER,
    );

    // Cyan corner pylons give the arena a readable silhouette and scale reference.
    for x in [-8.6, 8.6] {
        for z in [-8.6, 8.6] {
            spawn_box(
                scene,
                Vec3::new(x, 1.7, z),
                Vec3::new(0.55, 3.4, 0.55),
                MaterialId::RIFT,
            );
            spawn_box(
                scene,
                Vec3::new(x, 3.5, z),
                Vec3::new(0.95, 0.22, 0.95),
                MaterialId::AMBER,
            );
        }
    }

    // Low floor inlays create directional lanes without blocking player movement.
    for offset in [-6.0, -3.0, 3.0, 6.0] {
        spawn_box(
            scene,
            Vec3::new(offset, 0.012, 0.0),
            Vec3::new(0.08, 0.024, 15.0),
            MaterialId::RIFT,
        );
        spawn_box(
            scene,
            Vec3::new(0.0, 0.013, offset),
            Vec3::new(15.0, 0.026, 0.08),
            MaterialId::RIFT,
        );
    }

    // A raised, amber-edged landing pad frames the extraction beacon.
    spawn_box(
        scene,
        Vec3::new(0.0, 0.025, -8.0),
        Vec3::new(3.8, 0.05, 3.8),
        MaterialId::DEFAULT,
    );
    spawn_box(
        scene,
        Vec3::new(0.0, 0.06, -6.12),
        Vec3::new(3.8, 0.07, 0.12),
        MaterialId::AMBER,
    );
    spawn_box(
        scene,
        Vec3::new(0.0, 0.06, -9.88),
        Vec3::new(3.8, 0.07, 0.12),
        MaterialId::AMBER,
    );
    spawn_box(
        scene,
        Vec3::new(-1.88, 0.06, -8.0),
        Vec3::new(0.12, 0.07, 3.8),
        MaterialId::AMBER,
    );
    spawn_box(
        scene,
        Vec3::new(1.88, 0.06, -8.0),
        Vec3::new(0.12, 0.07, 3.8),
        MaterialId::AMBER,
    );
    build_arena_architecture(scene);
    build_crystal_markers(scene);
}

fn build_arena_architecture(scene: &mut Scene) {
    // Repeating wall fins make the perimeter read as a constructed facility.
    for z in [-5.0, 0.0, 5.0] {
        for x in [-9.18, 9.18] {
            spawn_box(
                scene,
                Vec3::new(x, 1.65, z),
                Vec3::new(0.18, 3.3, 0.42),
                MaterialId::AMBER,
            );
            spawn_box(
                scene,
                Vec3::new(x, 3.35, z),
                Vec3::new(0.34, 0.12, 0.58),
                MaterialId::RIFT,
            );
        }
    }
    for x in [-5.0, 0.0, 5.0] {
        for z in [-9.18, 9.18] {
            spawn_box(
                scene,
                Vec3::new(x, 1.65, z),
                Vec3::new(0.42, 3.3, 0.18),
                MaterialId::AMBER,
            );
        }
    }

    // A gateway marks the extraction approach while leaving its center open.
    for x in [-2.15, 2.15] {
        spawn_box(
            scene,
            Vec3::new(x, 1.65, -4.65),
            Vec3::new(0.28, 3.3, 0.32),
            MaterialId::RIFT,
        );
    }
    spawn_box(
        scene,
        Vec3::new(0.0, 3.32, -4.65),
        Vec3::new(4.55, 0.28, 0.42),
        MaterialId::AMBER,
    );
    spawn_box(
        scene,
        Vec3::new(0.0, 3.48, -4.65),
        Vec3::new(3.4, 0.06, 0.46),
        MaterialId::RIFT,
    );
}

fn build_crystal_markers(scene: &mut Scene) {
    for (x, z, scale) in [
        (-6.8, -6.8, 0.78),
        (6.8, -6.8, 0.78),
        (-6.8, 6.8, 0.78),
        (6.8, 6.8, 0.78),
        (0.0, 6.9, 0.58),
    ] {
        let transform = Transform3::new(
            Vec3::new(x, 1.05, z),
            Vec3::new(0.0, 0.0, 0.18),
            Vec3::new(scale * 0.72, scale * 1.35, scale * 0.72),
        );
        if let Some(crystal) = scene.spawn_with_mesh(transform, MeshId::CRYSTAL) {
            scene.set_material(crystal, MaterialId::RIFT);
        }
        spawn_box(
            scene,
            Vec3::new(x, 0.16, z),
            Vec3::new(0.62, 0.32, 0.62),
            MaterialId::AMBER,
        );
    }
}

/// Height of the native arena's procedural ground, in world-space Y units.
/// Keep this formula aligned with the renderer's terrain vertex generator.
fn arena_collision_world() -> CollisionWorld {
    let mut world = CollisionWorld::new(Aabb2::new(-9.5, 9.5, -9.5, 9.5));
    world.add_obstacle(Aabb2::new(-2.75, -1.25, -1.75, -0.25));
    world.add_obstacle(Aabb2::new(1.75, 3.25, 0.75, 2.25));
    world.add_obstacle(Aabb2::new(-1.0, 1.0, 3.25, 4.75));
    world
}

fn update_camera_controls(renderer: Option<&mut Renderer>, key: KeyCode) {
    let Some(renderer) = renderer else {
        return;
    };
    match key {
        KeyCode::KeyQ => renderer.orbit_camera(-0.12, 0.0),
        KeyCode::KeyE => renderer.orbit_camera(0.12, 0.0),
        KeyCode::Minus => renderer.zoom_camera(0.6),
        KeyCode::Equal => renderer.zoom_camera(-0.6),
        _ => {}
    }
}

fn build_vehicle(scene: &mut Scene) -> (EntityId, [EntityId; 4]) {
    let body = scene
        .spawn(Transform3::new(
            Vec3::new(0.0, 0.62, -1.2),
            Vec3::default(),
            Vec3::new(1.75, 0.55, 3.25),
        ))
        .expect("vehicle body entity ID space available");
    scene.set_material(body, MaterialId::RIFT);
    let mut wheels = Vec::with_capacity(4);
    for (x, z) in [(-0.95, -0.95), (0.95, -0.95), (-0.95, 0.95), (0.95, 0.95)] {
        let wheel = scene
            .spawn(Transform3::new(
                Vec3::new(x, -0.30, z),
                Vec3::new(0.0, 0.0, std::f32::consts::FRAC_PI_2),
                Vec3::new(0.34, 0.68, 0.68),
            ))
            .expect("vehicle wheel entity ID space available");
        scene.set_material(wheel, MaterialId::AMBER);
        assert!(scene.set_parent(wheel, Some(body)));
        wheels.push(wheel);
    }
    (
        body,
        wheels.try_into().expect("four vehicle wheels were created"),
    )
}

fn movement_button(key: KeyCode) -> Option<Button> {
    match key {
        KeyCode::KeyW | KeyCode::ArrowUp => Some(Button::MoveForward),
        KeyCode::KeyS | KeyCode::ArrowDown => Some(Button::MoveBackward),
        KeyCode::KeyA | KeyCode::ArrowLeft => Some(Button::MoveLeft),
        KeyCode::KeyD | KeyCode::ArrowRight => Some(Button::MoveRight),
        KeyCode::Space => Some(Button::Jump),
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(Button::Sprint),
        KeyCode::KeyF => Some(Button::Interact),
        _ => None,
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("event loop creation should succeed");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = RiftboundApp::default();
    event_loop
        .run_app(&mut app)
        .expect("Riftbound event loop should run");
}
