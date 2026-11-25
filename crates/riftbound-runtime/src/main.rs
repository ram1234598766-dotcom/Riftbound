//! Native window lifecycle and application entry point for Riftbound.

mod save;

use std::sync::Arc;
use std::time::{Duration, Instant};

use riftbound_core::{
    Aabb2, Button, CharacterController, Checkpoint, CheckpointProgress, CollisionWorld,
    EnemyArchetype, EnemyBrain, EnemyConfig, EnemyDifficulty, EnemyEvent, EnemyState, EntityId,
    Equipment, EquipmentSlot, ExtractionZone, InputState, Inventory, ItemKind, MaterialId, MeshId,
    MissionPhase, MissionProgress, ObjectiveKind, ObjectiveProgress, Pickup, PickupTracker,
    PlayerHealth, PlayerProgression, Scene, SuspensionConfig, SuspensionState, Transform3, Vec3,
    VehicleConfig, VehicleInput, VehicleRecovery, VehicleState, VehicleUpgrades, terrain_height,
    world_terrain_sample, world_terrain_vehicle_surface, WorldChunkRequest,
    WORLD_CHUNK_SIZE,
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
    vehicle_upgrades: VehicleUpgrades,
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
    loot_entities: Vec<(EntityId, Vec3, ItemKind)>,
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
    progression: PlayerProgression,
    inventory: Inventory,
    equipment: Equipment,
    inventory_open: bool,
    garage_open: bool,
    animation_time: f32,
    mission: MissionProgress,
    lan_server_started: bool,
    world_streamer: Option<riftbound_core::WorldChunkStreamer>,
    world_slot_map: Option<riftbound_core::WorldChunkSlotMap>,
    world_obstacle_entities: Vec<EntityId>,
}

impl RiftboundApp {
    fn saved_profile(&self) -> save::SavedProfile {
        let player_position = self.renderer.as_ref().and_then(|renderer| {
            self.player
                .and_then(|player| renderer.scene().get(player))
                .map(|transform| transform.translation)
        });
        save::SavedProfile {
            upgrades: self.vehicle_upgrades,
            inventory: self.inventory.clone(),
            equipment: self.equipment,
            progression: self.progression,
            checkpoint_active: self.checkpoint_progress.is_some_and(
                CheckpointProgress::has_active_checkpoint,
            ),
            player_position,
        }
    }
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
        let saved_profile = save::load_profile();
        let checkpoint_active = saved_profile.checkpoint_active;
        let saved_position = saved_profile.player_position;
        self.vehicle_upgrades = saved_profile.upgrades;
        self.inventory = saved_profile.inventory;
        self.equipment = saved_profile.equipment;
        self.progression = saved_profile.progression;
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
                let spawn_position = saved_position.map_or(
                    Vec3::new(0.0, PLAYER_RADIUS + 0.01, -4.0),
                    |saved| {
                        Vec3::new(
                            saved.x,
                            world_terrain_sample(saved.x, saved.z).height + PLAYER_RADIUS + 0.01,
                            saved.z,
                        )
                    },
                );
                let player_transform = Transform3::new(
                    spawn_position,
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
                let loot_entities = build_loot_pickups(&mut scene);
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
                let mut world_obstacle_entities = Vec::new();
                let mut scene_with_props = renderer.scene().clone();
                spawn_world_obstacle_props(
                    &mut scene_with_props,
                    world_streamer.active_chunks(),
                    &mut world_obstacle_entities,
                );
                if let Err(error) = renderer.set_scene(scene_with_props) {
                    log::error!("Could not initialize streamed world props: {error}");
                    event_loop.exit();
                    return;
                }
                renderer.follow_camera(player_transform.translation);
                self.camera_target = player_transform.translation;
                log::info!("Riftbound arena, player, and collision world initialized");
                window.request_redraw();
                self.window = Some(window);
                self.renderer = Some(renderer);
                self.world_streamer = Some(world_streamer);
                self.world_slot_map = Some(world_slot_map);
                self.world_obstacle_entities = world_obstacle_entities;
                self.player = Some(player);
                self.player_parts = Some(player_parts);
                self.vehicle = Some(vehicle);
                self.vehicle_wheels = Some(vehicle_wheels);
                self.vehicle_state = vehicle_state;
                self.vehicle_suspension = vehicle_suspension;
                self.vehicle_recovery = vehicle_recovery;
                self.driving = false;
                let mut collision = world_collision_world();
                if let Some(streamer) = self.world_streamer.as_ref() {
                    sync_world_collision(&mut collision, streamer.active_chunks());
                }
                self.collision = Some(collision);
                self.pickups = pickups;
                self.inventory_open = false;
                self.garage_open = false;
                self.mission = MissionProgress::new(self.pickups.total());
                self.pickup_entities = pickup_entities;
                self.loot_entities = loot_entities;
                self.animated_transforms = Vec::with_capacity(8 + self.pickup_entities.len());
                self.extraction = Some(extraction);
                self.beacon_entity = Some(beacon_entity);
                self.checkpoint_entity = Some(checkpoint_entity);
                self.checkpoint = Some(checkpoint);
                let mut checkpoint_progress = CheckpointProgress::new(player_transform.translation)
                    .expect("player start position is valid");
                if checkpoint_active {
                    let _ = checkpoint_progress.activate(checkpoint, checkpoint.position(), true);
                }
                self.checkpoint_progress = Some(checkpoint_progress);
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
        if let Err(error) = save::save_profile(&self.saved_profile()) {
            log::warn!("Could not save player profile while suspending: {error}");
        }
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
        self.world_streamer = None;
        self.world_slot_map = None;
        self.world_obstacle_entities.clear();
        self.pickups = PickupTracker::new();
        self.pickup_entities.clear();
        self.loot_entities.clear();
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
            WindowEvent::CloseRequested => {
                if let Err(error) = save::save_profile(&self.saved_profile()) {
                    log::warn!("Could not save vehicle upgrades: {error}");
                }
                event_loop.exit();
            }
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
                    if let Err(error) = save::save_profile(&self.saved_profile()) {
                        log::warn!("Could not save player profile: {error}");
                    }
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
                    if event.state == ElementState::Pressed && key == KeyCode::KeyI {
                        self.inventory_open = !self.inventory_open;
                    }
                    if event.state == ElementState::Pressed && key == KeyCode::KeyG {
                        let near_vehicle = self.driving
                            || self.renderer.as_ref().and_then(|renderer| {
                                self.player.and_then(|player| renderer.scene().get(player).copied())
                            }).is_some_and(|player| {
                                distance_squared(player.translation, self.vehicle_state.position) <= 3.5 * 3.5
                            });
                        if near_vehicle {
                            self.garage_open = !self.garage_open;
                            log::info!("{}", if self.garage_open {
                                "GARAGE OPEN: U engine upgrade, H handling/tires, G close"
                            } else { "Garage closed" });
                        } else {
                            log::info!("Move near your vehicle to open the garage");
                        }
                    }
                    if event.state == ElementState::Pressed && self.garage_open {
                        match key {
                            KeyCode::KeyU => {
                                if self.vehicle_upgrades.purchase_engine_upgrade(&mut self.inventory) {
                                    log::info!("Engine upgraded to tier {}", self.vehicle_upgrades.engine_level());
                                    if let Err(error) = save::save_profile(&self.saved_profile()) {
                                        log::warn!("Could not save vehicle upgrades: {error}");
                                    }
                                } else if let Some(cost) = self.vehicle_upgrades.next_engine_cost() {
                                    log::info!("Engine upgrade needs {cost} Salvage; you have {}", self.inventory.count(ItemKind::Salvage));
                                } else {
                                    log::info!("Engine is already at maximum tier");
                                }
                            }
                            KeyCode::KeyH => {
                                if self.vehicle_upgrades.purchase_handling_upgrade(&mut self.inventory) {
                                    log::info!("Handling and tires upgraded to tier {}", self.vehicle_upgrades.handling_level());
                                    if let Err(error) = save::save_profile(&self.saved_profile()) {
                                        log::warn!("Could not save vehicle upgrades: {error}");
                                    }
                                } else if let Some(cost) = self.vehicle_upgrades.next_handling_cost() {
                                    log::info!("Handling upgrade needs {cost} Salvage; you have {}", self.inventory.count(ItemKind::Salvage));
                                } else {
                                    log::info!("Handling and tires are already at maximum tier");
                                }
                            }
                            _ => {}
                        }
                    }
                    if event.state == ElementState::Pressed {
                        match key {
                            KeyCode::Digit5 => {
                                if self.inventory.count(ItemKind::Medkit) > 0 {
                                    let restored = self.player_health.heal(35);
                                    if restored > 0 {
                                        self.inventory.remove(ItemKind::Medkit, 1);
                                        log::info!("Medkit used: restored {restored} health");
                                    } else {
                                        log::info!("Health is already full; medkit was not consumed");
                                    }
                                } else {
                                    log::info!("No medkits available");
                                }
                            }
                            KeyCode::Digit1 => {
                                if self.inventory.equip_from_inventory(
                                    &mut self.equipment,
                                    ItemKind::ExplorerVest,
                                ) {
                                    log::info!("Explorer Vest equipped: incoming damage reduced by 8");
                                } else {
                                    log::info!("No Explorer Vest available in inventory");
                                }
                            }
                            KeyCode::Digit2 => {
                                if self.inventory.equip_from_inventory(
                                    &mut self.equipment,
                                    ItemKind::TrailBoots,
                                ) {
                                    log::info!("Trail Boots equipped: traversal speed increased by 8%");
                                } else {
                                    log::info!("No Trail Boots available in inventory");
                                }
                            }
                            KeyCode::Digit3 => {
                                if self.inventory.unequip_to_inventory(
                                    &mut self.equipment,
                                    EquipmentSlot::Body,
                                ) {
                                    log::info!("Body equipment returned to inventory");
                                }
                            }
                            KeyCode::Digit4 => {
                                if self.inventory.unequip_to_inventory(
                                    &mut self.equipment,
                                    EquipmentSlot::Feet,
                                ) {
                                    log::info!("Foot equipment returned to inventory");
                                }
                            }
                            _ => {}
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
                        self.controller
                            .set_movement_bonus(self.equipment.movement_bonus());
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
                    let floor_height = world_terrain_sample(resolved.position.x, resolved.position.z).height
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
                            self.vehicle_upgrades.apply_to(VehicleConfig::default()),
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
                        let horizontal_speed = (horizontal_velocity.x * horizontal_velocity.x
                            + horizontal_velocity.z * horizontal_velocity.z)
                            .sqrt();
                        let movement = (horizontal_speed / 4.5).clamp(0.0, 1.0);
                        let gait = self.animation_time * (7.2 + movement * 2.0);
                        let leg_swing = gait.sin() * 0.72 * movement;
                        let arm_swing = -leg_swing * 0.78;
                        let idle_breath = (self.animation_time * 1.65).sin()
                            * 0.018
                            * (1.0 - movement);
                        let stride_bob = (gait * 2.0).sin() * 0.045 * movement;
                        let bob = idle_breath + stride_bob;
                        let forward_lean = (movement * 0.11).min(0.11);
                        let lateral_lean = (-horizontal_velocity.x * 0.025).clamp(-0.07, 0.07);
                        let idle_sway = (self.animation_time * 0.9).sin()
                            * 0.025
                            * (1.0 - movement);
                        self.animated_transforms.extend([
                            (
                                parts.torso,
                                character_part_transform(
                                    0.0,
                                    0.48 + bob,
                                    0.0,
                                    forward_lean,
                                    0.0,
                                    lateral_lean + idle_sway,
                                    0.58,
                                    0.72 + idle_breath * 0.35,
                                    0.34,
                                ),
                            ),
                            (
                                parts.head,
                                character_part_transform(
                                    0.0,
                                    1.15 + bob,
                                    0.0,
                                    -forward_lean * 0.35,
                                    0.0,
                                    -lateral_lean * 0.35 - idle_sway * 0.4,
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
                                    arm_swing + idle_sway * 0.35,
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
                                    -arm_swing - idle_sway * 0.35,
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
                    for ((entity, base, _kind), phase_offset) in
                        self.loot_entities.iter().zip([0.0_f32, 1.7, 3.4])
                    {
                        if renderer.scene().get(*entity).is_none() {
                            continue;
                        }
                        let phase = self.animation_time * 1.8 + phase_offset;
                        self.animated_transforms.push((
                            *entity,
                            Transform3::new(
                                Vec3::new(base.x, base.y + phase.sin() * 0.09, base.z),
                                Vec3::new(0.0, self.animation_time * 0.8 + phase_offset, 0.0),
                                Vec3::new(0.22, 0.28, 0.22),
                            ),
                        ));
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
                                    && self.player_health.apply_damage(
                                        25_u8.saturating_sub(self.equipment.armor_bonus()).max(1),
                                        1.0,
                                    )
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
                                && self.player_health.apply_damage(
                                    15_u8.saturating_sub(self.equipment.armor_bonus()).max(1),
                                    1.0,
                                )
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
                            if collected_count > 0 {
                                let shard_overflow = self
                                    .inventory
                                    .add(ItemKind::RiftShard, collected_count as u16);
                                if shard_overflow > 0 {
                                    log::warn!(
                                        "Inventory full: {shard_overflow} rift shard(s) could not be stored"
                                    );
                                }
                                let awarded_xp = (collected_count as u64) * 25;
                                let levels_gained = self.progression.award_xp(awarded_xp);
                                log::info!(
                                    "Collected {collected_count} rift shard(s): +{awarded_xp} XP, level {}{}",
                                    self.progression.level(),
                                    if levels_gained > 0 { " (level up!)" } else { "" },
                                );
                            }
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
                            let nearby_loot = self
                                .loot_entities
                                .iter()
                                .position(|(entity, position, _)| {
                                    renderer.scene().get(*entity).is_some()
                                        && distance_squared(transform.translation, *position)
                                            <= 1.8 * 1.8
                                });
                            if interact_pressed {
                                if let Some(index) = nearby_loot {
                                    let (entity, _position, kind) = self.loot_entities[index];
                                    if self.inventory.add(kind, 1) == 0 {
                                        let mut updated_scene = renderer.scene().clone();
                                        updated_scene.despawn(entity);
                                        if let Err(error) = renderer.set_scene(updated_scene) {
                                            log::error!("Could not remove collected world loot: {error}");
                                            event_loop.exit();
                                            return;
                                        }
                                        self.loot_entities.remove(index);
                                        let label = match kind {
                                            ItemKind::Salvage => "salvage",
                                            ItemKind::Medkit => "medkit",
                                            _ => "item",
                                        };
                                        log::info!("Collected 1 {label}");
                                    } else {
                                        log::info!("Inventory full; world loot was left in place");
                                    }
                                }
                            }
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
                                if let Err(error) = save::save_profile(&self.saved_profile()) {
                                    log::warn!("Could not save activated checkpoint: {error}");
                                }
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
                                let levels_gained = self.progression.award_xp(100);
                                window.set_title(&format!(
                                    "RIFTBOUND — Mission complete! • Level {}",
                                    self.progression.level()
                                ));
                                log::info!(
                                    "Shardfall mission completed: +100 XP bonus, level {}{}",
                                    self.progression.level(),
                                    if levels_gained > 0 { " (level up!)" } else { "" },
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
                            let near_loot = nearby_loot.is_some();
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
                            let prompt = if self.garage_open {
                                Some(String::from("GARAGE: U ENGINE • H HANDLING/TIRES • G CLOSE"))
                            } else if self.mission.is_complete() {
                                None
                            } else if sentinel_can_be_disabled {
                                Some(String::from("F - DISABLE SENTINEL"))
                            } else if near_sentinel {
                                Some(String::from("SENTINEL LOCKING ON"))
                            } else if self.mission.extraction_unlocked() && near_beacon {
                                Some(String::from("F - EXTRACT"))
                            } else if near_checkpoint {
                                Some(String::from("F - ACTIVATE CHECKPOINT"))
                            } else if near_loot {
                                Some(String::from("F - PICK UP LOOT"))
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
                                inventory: (self.inventory_open || self.garage_open).then(|| {
                                    format!(
                                        "{} SALV {} ENG {} HAND {}",
                                        if self.garage_open { "GARAGE" } else { "SHARDS" },
                                        self.inventory.count(ItemKind::Salvage),
                                        self.vehicle_upgrades.engine_level(),
                                        self.vehicle_upgrades.handling_level(),
                                    )
                                }),
                                equipment: format!(
                                    "BODY {} FEET {}",
                                    if self.equipment.item(EquipmentSlot::Body).is_some() {
                                        "VEST"
                                    } else {
                                        "NONE"
                                    },
                                    if self.equipment.item(EquipmentSlot::Feet).is_some() {
                                        "BOOTS"
                                    } else {
                                        "NONE"
                                    },
                                ),
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
                        if streamer.update_focus(world_focus.x, world_focus.z) {
                            if !renderer.sync_world_terrain(streamer.changes(), slot_map) {
                                log::error!("World terrain chunk synchronization failed");
                            }
                            if let Some(collision) = self.collision.as_mut() {
                                sync_world_collision(collision, streamer.active_chunks());
                            }
                            let mut updated_scene = renderer.scene().clone();
                            spawn_world_obstacle_props(
                                &mut updated_scene,
                                streamer.active_chunks(),
                                &mut self.world_obstacle_entities,
                            );
                            if let Err(error) = renderer.set_scene(updated_scene) {
                                log::error!("World obstacle prop synchronization failed: {error}");
                            }
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

fn spawn_attached_character_detail(
    scene: &mut Scene,
    parent: EntityId,
    transform: Transform3,
    material: MaterialId,
) -> EntityId {
    let id = scene
        .spawn(transform)
        .expect("character detail entity ID space available");
    assert!(scene.set_parent(id, Some(parent)));
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
    assert!(scene.set_mesh(head, MeshId::CRYSTAL));

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

    // Chest plate, collar and shoulder shells follow the animated torso.
    spawn_attached_character_detail(
        scene,
        torso,
        character_part_transform(0.0, 0.06, -0.88, 0.0, 0.0, 0.0, 0.78, 0.62, 0.20),
        MaterialId::DEFAULT,
    );
    spawn_attached_character_detail(
        scene,
        torso,
        character_part_transform(0.0, 0.48, -0.10, 0.0, 0.0, 0.0, 0.44, 0.18, 0.42),
        MaterialId::AMBER,
    );
    for x in [-0.78, 0.78] {
        spawn_attached_character_detail(
            scene,
            torso,
            character_part_transform(x, 0.34, 0.0, 0.0, 0.0, 0.0, 0.38, 0.34, 0.42),
            MaterialId::AMBER,
        );
    }

    // A compact field pack and its cyan power cell sit behind the torso.
    spawn_attached_character_detail(
        scene,
        torso,
        character_part_transform(0.0, 0.0, 0.92, 0.0, 0.0, 0.0, 0.58, 0.70, 0.36),
        MaterialId::AMBER,
    );
    spawn_attached_character_detail(
        scene,
        torso,
        character_part_transform(0.0, 0.0, 1.28, 0.0, 0.0, 0.0, 0.28, 0.42, 0.12),
        MaterialId::RIFT,
    );

    // The visor is parented to the helmet so it remains aligned during motion.
    spawn_attached_character_detail(
        scene,
        head,
        character_part_transform(0.0, -0.05, -0.82, 0.0, 0.0, 0.0, 0.68, 0.22, 0.16),
        MaterialId::RIFT,
    );

    // Gloves and reinforced boots follow the corresponding animated limbs.
    for arm in [left_arm, right_arm] {
        spawn_attached_character_detail(
            scene,
            arm,
            character_part_transform(0.0, -0.82, 0.0, 0.0, 0.0, 0.0, 1.24, 0.34, 1.20),
            MaterialId::AMBER,
        );
    }
    for leg in [left_leg, right_leg] {
        spawn_attached_character_detail(
            scene,
            leg,
            character_part_transform(0.0, -0.92, -0.10, 0.0, 0.0, 0.0, 1.42, 0.40, 1.62),
            MaterialId::DEFAULT,
        );
        spawn_attached_character_detail(
            scene,
            leg,
            character_part_transform(0.0, 0.02, -0.12, 0.0, 0.0, 0.0, 1.18, 0.22, 1.18),
            MaterialId::RIFT,
        );
    }

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

fn build_loot_pickups(scene: &mut Scene) -> Vec<(EntityId, Vec3, ItemKind)> {
    let pickups = [
        (
            Vec3::new(-7.0, terrain_height(-7.0, -2.0) + 0.32, -2.0),
            ItemKind::Salvage,
        ),
        (
            Vec3::new(7.0, terrain_height(7.0, -2.0) + 0.32, -2.0),
            ItemKind::Medkit,
        ),
        (
            Vec3::new(0.0, terrain_height(0.0, 8.0) + 0.32, 8.0),
            ItemKind::Salvage,
        ),
    ];
    pickups
        .into_iter()
        .map(|(position, kind)| {
            let transform = Transform3::new(
                position,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.22, 0.28, 0.22),
            );
            let mesh = match kind {
                ItemKind::Salvage => MeshId::PYRAMID,
                ItemKind::Medkit => MeshId::CRYSTAL,
                _ => MeshId::CUBE,
            };
            let entity = scene
                .spawn_with_mesh(transform, mesh)
                .expect("world loot entity ID space available");
            let material = if kind == ItemKind::Medkit {
                MaterialId::AMBER
            } else {
                MaterialId::RIFT
            };
            scene.set_material(entity, material);
            (entity, position, kind)
        })
        .collect()
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

/// Broad world bounds for the streamed-world collision prototype.
fn world_collision_world() -> CollisionWorld {
    CollisionWorld::new(Aabb2::new(
        -1_000_000.0,
        1_000_000.0,
        -1_000_000.0,
        1_000_000.0,
    ))
}

/// Deterministic obstacle footprints for each active terrain chunk.
#[allow(clippy::cast_precision_loss)] // Chunk indices and generated offsets are tightly bounded.
fn world_obstacle_footprints(chunks: &[WorldChunkRequest]) -> Vec<Aabb2> {
    let mut obstacles = Vec::with_capacity(chunks.len() * 2);
    for request in chunks {
        let mut hash = request.coord.x.cast_unsigned().wrapping_mul(0x9E37_79B9)
            ^ request.coord.z.cast_unsigned().wrapping_mul(0x85EB_CA6B);
        hash ^= hash >> 16;
        hash = hash.wrapping_mul(0x7FEB_352D);
        hash ^= hash >> 15;
        let origin_x = request.coord.x as f32 * WORLD_CHUNK_SIZE;
        let origin_z = request.coord.z as f32 * WORLD_CHUNK_SIZE;
        let prop_count = match request.lod {
            riftbound_core::WorldChunkLod::Detailed => 2_u32,
            riftbound_core::WorldChunkLod::Distant => 1_u32,
        };
        for index in 0..prop_count {
            let selector = hash.rotate_left(index * 11).wrapping_add(index * 0x9E37_79B9);
            let side = if selector & 1 == 0 { -1.0 } else { 1.0 };
            let center_x = origin_x + side * (13.0 + (selector % 9) as f32);
            let center_z = origin_z + 10.0 + ((selector >> 8) % 40) as f32;
            let half_size = 0.65 + ((selector >> 16) % 8) as f32 * 0.12;

            // Keep generated props out of the primary road and both branch roads.
            // The extra margin accounts for the visible tree crown, not just its trunk.
            let main_road = world_road_sample(center_x, center_z);
            let on_eastbound_spur = (0.0..=160.0).contains(&center_x)
                && center_z.abs() <= 11.0;
            let on_northbound_branch = (-132.0..=2.0).contains(&center_z)
                && (center_x - 80.0).abs() <= 11.0;
            let road_clearance = main_road.lateral_distance.abs()
                > main_road.half_width + 3.0;
            if selector % 5 != 0
                && road_clearance
                && !on_eastbound_spur
                && !on_northbound_branch
            {
                obstacles.push(Aabb2::new(
                    center_x - half_size,
                    center_x + half_size,
                    center_z - half_size,
                    center_z + half_size,
                ));
            }
        }
    }
    obstacles
}

/// Derives visual variation from world position so chunk enumeration order cannot change props.
fn world_prop_variant(x: f32, z: f32) -> (u32, f32) {
    let mut hash = x.to_bits().rotate_left(7) ^ z.to_bits().rotate_right(5);
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7FEB_352D);
    hash ^= hash >> 15;
    let yaw = (hash % 701) as f32 * 0.001 - 0.35;
    (hash % 3, yaw)
}

/// Biases prop silhouettes toward vegetation suited to the sampled terrain biome.
fn world_prop_variant_for_surface(surface: VehicleSurface, variant: u32) -> u32 {
    match surface {
        VehicleSurface::Snow if variant == 0 => 1,
        VehicleSurface::Sand | VehicleSurface::Mud if variant == 0 => 2,
        VehicleSurface::Rock if variant == 2 => 1,
        _ => variant,
    }
}

/// Keeps visible blockout props and collision footprints aligned to active chunks.
fn sync_world_collision(world: &mut CollisionWorld, chunks: &[WorldChunkRequest]) {
    let obstacles = world_obstacle_footprints(chunks);
    world.replace_obstacles(&obstacles);
}

/// Rebuilds the bounded set of visible world obstacle props from active chunks.
fn spawn_world_obstacle_props(
    scene: &mut Scene,
    chunks: &[WorldChunkRequest],
    entity_ids: &mut Vec<EntityId>,
) {
    for id in entity_ids.drain(..) {
        scene.despawn(id);
    }
    for footprint in world_obstacle_footprints(chunks) {
        let x = f32::midpoint(footprint.min_x, footprint.max_x);
        let z = f32::midpoint(footprint.min_z, footprint.max_z);
        let half_x = (footprint.max_x - footprint.min_x) * 0.5;
        let half_z = (footprint.max_z - footprint.min_z) * 0.5;
        let terrain = world_terrain_sample(x, z);
        let ground = terrain.height;
        let (base_variant, yaw) = world_prop_variant(x, z);
        let variant = world_prop_variant_for_surface(terrain.surface, base_variant);

        match variant {
            // One joined tree mesh keeps streamed vegetation to a single draw instance.
            0 => {
                let tree = Transform3::new(
                    Vec3::new(x, ground + 1.25, z),
                    Vec3::new(0.0, yaw, 0.0),
                    Vec3::new(half_x * 1.9, 1.0, half_z * 1.9),
                );
                if let Some(id) = scene.spawn_with_mesh(tree, MeshId::ENV_TREE) {
                    scene.set_material(id, MaterialId::DEFAULT);
                    entity_ids.push(id);
                }
            }
            // Joined faceted forms give boulders a less uniform silhouette.
            1 => {
                let height = 0.85 + half_x * 0.5;
                let rock = Transform3::new(
                    Vec3::new(x, ground + height * 0.5, z),
                    Vec3::new(yaw * 0.7, yaw, 0.12),
                    Vec3::new(half_x * 1.8, height, half_z * 1.8),
                );
                if let Some(id) = scene.spawn_with_mesh(rock, MeshId::ENV_BOULDER) {
                    scene.set_material(id, MaterialId::DEFAULT);
                    entity_ids.push(id);
                }
            }
            // A compact joined shrub mesh adds foliage without separate scene parts.
            _ => {
                let shrub = Transform3::new(
                    Vec3::new(x, ground + 0.38, z),
                    Vec3::new(0.0, yaw, 0.0),
                    Vec3::new(half_x * 1.55, 0.72, half_z * 1.55),
                );
                if let Some(id) = scene.spawn_with_mesh(shrub, MeshId::ENV_SHRUB) {
                    scene.set_material(id, MaterialId::DEFAULT);
                    entity_ids.push(id);
                }
            }
        }
    }
}


#[cfg(test)]
mod world_prop_tests {
    use super::*;

    #[test]
    fn distant_chunks_use_half_the_detailed_prop_budget() {
        let coord = riftbound_core::WorldChunkCoord { x: 4, z: -2 };
        let detailed = [WorldChunkRequest {
            coord,
            lod: riftbound_core::WorldChunkLod::Detailed,
            distance_squared: 0,
        }];
        let distant = [WorldChunkRequest {
            coord,
            lod: riftbound_core::WorldChunkLod::Distant,
            distance_squared: 1,
        }];
        assert_eq!(
            world_obstacle_footprints(&detailed).len(),
            world_obstacle_footprints(&distant).len() * 2
        );
    }

    #[test]
    fn obstacle_footprints_are_deterministic_and_chunk_local() {
        let chunks = [
            WorldChunkRequest {
                coord: riftbound_core::WorldChunkCoord { x: 2, z: -3 },
                lod: riftbound_core::WorldChunkLod::Detailed,
                distance_squared: 0,
            },
            WorldChunkRequest {
                coord: riftbound_core::WorldChunkCoord { x: 3, z: -3 },
                lod: riftbound_core::WorldChunkLod::Distant,
                distance_squared: 1,
            },
        ];
        let first = world_obstacle_footprints(&chunks);
        assert_eq!(first, world_obstacle_footprints(&chunks));
        assert!(first.iter().all(|footprint| {
            footprint.min_x.is_finite()
                && footprint.max_x.is_finite()
                && footprint.min_z.is_finite()
                && footprint.max_z.is_finite()
                && footprint.min_x < footprint.max_x
                && footprint.min_z < footprint.max_z
        }));
        assert!(first.len() <= chunks.len() * 2);
    }

    #[test]
    fn prop_variants_and_yaw_are_independent_of_chunk_order() {
        let first = world_prop_variant(17.5, -42.0);
        let second = world_prop_variant(-91.0, 128.0);
        assert_eq!(first, world_prop_variant(17.5, -42.0));
        assert_eq!(second, world_prop_variant(-91.0, 128.0));
        assert_ne!(first, second);
    }

    #[test]
    fn prop_silhouettes_follow_terrain_surface() {
        assert_eq!(
            world_prop_variant_for_surface(VehicleSurface::Snow, 0),
            1
        );
        assert_eq!(
            world_prop_variant_for_surface(VehicleSurface::Sand, 0),
            2
        );
        assert_eq!(
            world_prop_variant_for_surface(VehicleSurface::Mud, 0),
            2
        );
        assert_eq!(
            world_prop_variant_for_surface(VehicleSurface::Rock, 2),
            1
        );
        assert_eq!(
            world_prop_variant_for_surface(VehicleSurface::Gravel, 0),
            0
        );
        assert_eq!(
            world_prop_variant_for_surface(VehicleSurface::Asphalt, 2),
            2
        );
    }

    #[test]
    fn obstacle_footprints_keep_clear_of_all_road_corridors() {
        let chunks = [
            WorldChunkRequest {
                coord: riftbound_core::WorldChunkCoord { x: 0, z: 0 },
                lod: riftbound_core::WorldChunkLod::Detailed,
                distance_squared: 0,
            },
            WorldChunkRequest {
                coord: riftbound_core::WorldChunkCoord { x: 1, z: -1 },
                lod: riftbound_core::WorldChunkLod::Detailed,
                distance_squared: 1,
            },
            WorldChunkRequest {
                coord: riftbound_core::WorldChunkCoord { x: 1, z: -2 },
                lod: riftbound_core::WorldChunkLod::Detailed,
                distance_squared: 4,
            },
        ];
        for footprint in world_obstacle_footprints(&chunks) {
            let x = f32::midpoint(footprint.min_x, footprint.max_x);
            let z = f32::midpoint(footprint.min_z, footprint.max_z);
            let road = world_road_sample(x, z);
            assert!(road.lateral_distance.abs() > road.half_width + 3.0);
            assert!(!((0.0..=160.0).contains(&x) && z.abs() <= 11.0));
            assert!(!((-132.0..=2.0).contains(&z) && (x - 80.0).abs() <= 11.0));
        }
    }
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
        .spawn_with_mesh(
            Transform3::new(
                Vec3::new(0.0, 0.62, -1.2),
                Vec3::default(),
                Vec3::new(1.0, 1.0, 1.0),
            ),
            MeshId::VEHICLE_BODY,
        )
        .expect("vehicle body entity ID space available");
    scene.set_material(body, MaterialId::RIFT);
    let mut wheels = Vec::with_capacity(4);
    for (x, z) in [(-0.95, -0.95), (0.95, -0.95), (-0.95, 0.95), (0.95, 0.95)] {
        let wheel = scene
            .spawn_with_mesh(
                Transform3::new(
                    Vec3::new(x, -0.30, z),
                    Vec3::new(0.0, 0.0, std::f32::consts::FRAC_PI_2),
                    Vec3::new(0.34, 0.68, 0.68),
                ),
                MeshId::VEHICLE_WHEEL,
            )
            .expect("vehicle wheel entity ID space available");
        scene.set_material(wheel, MaterialId::DEFAULT);
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
