//! Native window lifecycle and application entry point for Riftbound.

use std::sync::Arc;
use std::time::{Duration, Instant};

use riftbound_core::{Button, CharacterController, EntityId, InputState, Transform3, Vec3};
use riftbound_renderer::Renderer;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

#[derive(Default)]
struct RiftboundApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    player: Option<EntityId>,
    input: InputState,
    controller: CharacterController,
    last_frame: Option<Instant>,
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
                let player_transform = Transform3::new(
                    Vec3::new(0.0, 0.0, -1.8),
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.38, 0.38, 0.38),
                );
                let player = scene
                    .spawn(player_transform)
                    .expect("player entity ID space available");
                if let Err(error) = renderer.set_scene(scene) {
                    log::error!("Could not add the player to the scene: {error}");
                    event_loop.exit();
                    return;
                }
                renderer.follow_camera(player_transform.translation);
                log::info!("Riftbound GPU surface and player initialized");
                window.request_redraw();
                self.window = Some(window);
                self.renderer = Some(renderer);
                self.player = Some(player);
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
        self.last_frame = None;
    }

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
                if let (Some(renderer), Some(player)) = (self.renderer.as_mut(), self.player) {
                    let Some(mut transform) = renderer.scene().get(player).copied() else {
                        log::error!("The player entity is no longer in the active scene");
                        event_loop.exit();
                        return;
                    };
                    self.controller.update(&mut transform, &self.input, delta);
                    match renderer.set_object_transform(player, transform) {
                        Ok(true) => renderer.follow_camera(transform.translation),
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

fn movement_button(key: KeyCode) -> Option<Button> {
    match key {
        KeyCode::KeyW | KeyCode::ArrowUp => Some(Button::MoveForward),
        KeyCode::KeyS | KeyCode::ArrowDown => Some(Button::MoveBackward),
        KeyCode::KeyA | KeyCode::ArrowLeft => Some(Button::MoveLeft),
        KeyCode::KeyD | KeyCode::ArrowRight => Some(Button::MoveRight),
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
