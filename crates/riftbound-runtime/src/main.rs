//! Native window lifecycle and application entry point for Riftbound.

use std::sync::Arc;
use std::time::{Duration, Instant};

use riftbound_core::{
    Aabb2, Button, CharacterController, CollisionWorld, EntityId, ExtractionZone, InputState,
    MaterialId, MeshId, Pickup, PickupTracker, Scene, Transform3, Vec3,
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
    collision: Option<CollisionWorld>,
    input: InputState,
    controller: CharacterController,
    last_frame: Option<Instant>,
    camera_target: Vec3,
    pickups: PickupTracker,
    pickup_entities: Vec<EntityId>,
    extraction: Option<ExtractionZone>,
    beacon_entity: Option<EntityId>,
    animation_time: f32,
    mission_complete: bool,
}

impl ApplicationHandler for RiftboundApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
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
                let (pickups, pickup_entities) = build_pickups(&mut scene);
                let (beacon_entity, extraction) = build_extraction_beacon(&mut scene);
                if let Err(error) = renderer.set_scene(scene) {
                    log::error!("Could not create the arena scene: {error}");
                    event_loop.exit();
                    return;
                }
                renderer.follow_camera(player_transform.translation);
                self.camera_target = player_transform.translation;
                log::info!("Riftbound arena, player, and collision world initialized");
                window.request_redraw();
                self.window = Some(window);
                self.renderer = Some(renderer);
                self.player = Some(player);
                self.collision = Some(arena_collision_world());
                self.pickups = pickups;
                self.pickup_entities = pickup_entities;
                self.extraction = Some(extraction);
                self.beacon_entity = Some(beacon_entity);
                self.animation_time = 0.0;
                self.mission_complete = false;
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
        self.collision = None;
        self.pickups = PickupTracker::new();
        self.pickup_entities.clear();
        self.extraction = None;
        self.beacon_entity = None;
        self.animation_time = 0.0;
        self.mission_complete = false;
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
                if let (Some(renderer), Some(player), Some(collision)) =
                    (self.renderer.as_mut(), self.player, self.collision.as_ref())
                {
                    let Some(mut transform) = renderer.scene().get(player).copied() else {
                        log::error!("The player entity is no longer in the active scene");
                        event_loop.exit();
                        return;
                    };
                    let previous_position = transform.translation;
                    self.controller.update(&mut transform, &self.input, delta);
                    let horizontal_velocity = self.controller.velocity();
                    if horizontal_velocity.x * horizontal_velocity.x
                        + horizontal_velocity.z * horizontal_velocity.z
                        > 0.01
                    {
                        transform.rotation_radians.y =
                            horizontal_velocity.x.atan2(-horizontal_velocity.z);
                    }
                    self.controller.update_vertical(
                        &mut transform,
                        &self.input,
                        delta,
                        PLAYER_RADIUS + 0.01,
                    );
                    let displacement = Vec3::new(
                        transform.translation.x - previous_position.x,
                        0.0,
                        transform.translation.z - previous_position.z,
                    );
                    let resolved =
                        collision.move_character(previous_position, displacement, PLAYER_RADIUS);
                    transform.translation.x = resolved.position.x;
                    transform.translation.z = resolved.position.z;
                    self.controller
                        .resolve_collision(resolved.blocked_x, resolved.blocked_z);
                    let mut animated_transforms = vec![(player, transform)];

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
                        animated_transforms.push((*entity, animated));
                    }
                    if let Some(beacon) = self.beacon_entity {
                        let pulse = 1.0 + (self.animation_time * 2.0).sin() * 0.06;
                        animated_transforms.push((
                            beacon,
                            Transform3::new(
                                Vec3::new(0.0, 0.55, -8.0),
                                Vec3::new(0.0, self.animation_time * 0.45, 0.0),
                                Vec3::new(0.62 * pulse, 0.78 * pulse, 0.62 * pulse),
                            ),
                        ));
                    }
                    match renderer.set_object_transforms(&animated_transforms) {
                        Ok(true) => {
                            let camera_blend = 1.0 - (-8.0 * delta.min(0.1)).exp();
                            self.camera_target.x +=
                                (transform.translation.x - self.camera_target.x) * camera_blend;
                            self.camera_target.y +=
                                (transform.translation.y - self.camera_target.y) * camera_blend;
                            self.camera_target.z +=
                                (transform.translation.z - self.camera_target.z) * camera_blend;
                            renderer.follow_camera(self.camera_target);
                            let collected = self.pickups.collect_nearby(
                                transform.translation,
                                self.input.was_pressed(Button::Interact),
                            );
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
                                let progress = if self.pickups.is_complete() {
                                    format!(
                                        "RIFTBOUND — Return to extraction beacon ({}/{})",
                                        self.pickups.collected(),
                                        self.pickups.total(),
                                    )
                                } else {
                                    format!(
                                        "RIFTBOUND — Shards: {}/{}",
                                        self.pickups.collected(),
                                        self.pickups.total(),
                                    )
                                };
                                window.set_title(&progress);
                                log::info!("{progress}");
                            }
                            if !self.mission_complete
                                && self.extraction.is_some_and(|zone| {
                                    zone.can_extract(
                                        transform.translation,
                                        self.input.was_pressed(Button::Interact),
                                        self.pickups.is_complete(),
                                    )
                                })
                            {
                                self.mission_complete = true;
                                window.set_title("RIFTBOUND — Mission complete!");
                                log::info!(
                                    "Shardfall mission completed: all shards recovered and extraction reached"
                                );
                            }

                            let near_pickup = PICKUP_POSITIONS.iter().enumerate().any(|(index, position)| {
                                index >= self.pickups.total().saturating_sub(self.pickups.total())
                                    && self.pickups.collected() < self.pickups.total()
                                    && distance_squared(transform.translation, *position) <= 1.7 * 1.7
                                    && self.pickup_entities.get(index).is_some_and(|id| renderer.scene().get(*id).is_some())
                            });
                            let near_beacon = distance_squared(transform.translation, Vec3::new(0.0, 0.55, -8.0)) <= 1.9 * 1.9;
                            let status = if self.mission_complete {
                                "MISSION COMPLETE!"
                            } else if self.pickups.is_complete() {
                                "RETURN TO BEACON"
                            } else {
                                "COLLECT RIFT SHARDS"
                            };
                            let prompt = if self.mission_complete {
                                None
                            } else if self.pickups.is_complete() && near_beacon {
                                Some(String::from("F - EXTRACT"))
                            } else if !self.pickups.is_complete() && near_pickup {
                                Some(String::from("F - COLLECT"))
                            } else {
                                None
                            };
                            renderer.set_hud(HudData {
                                collected: self.pickups.collected(),
                                total: self.pickups.total(),
                                status: String::from(status),
                                prompt,
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

fn build_arena(scene: &mut Scene) {
    // A broad, flat cube forms the walkable floor.
    spawn_box(
        scene,
        Vec3::new(0.0, -0.15, 0.0),
        Vec3::new(20.0, 0.3, 20.0),
        MaterialId::DEFAULT,
    );

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
}

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

fn movement_button(key: KeyCode) -> Option<Button> {
    match key {
        KeyCode::KeyW | KeyCode::ArrowUp => Some(Button::MoveForward),
        KeyCode::KeyS | KeyCode::ArrowDown => Some(Button::MoveBackward),
        KeyCode::KeyA | KeyCode::ArrowLeft => Some(Button::MoveLeft),
        KeyCode::KeyD | KeyCode::ArrowRight => Some(Button::MoveRight),
        KeyCode::Space => Some(Button::Jump),
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
