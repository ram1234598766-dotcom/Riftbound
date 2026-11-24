//! Native window lifecycle and application entry point for Riftbound.

use std::sync::Arc;

use riftbound_renderer::Renderer;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

#[derive(Default)]
struct RiftboundApp {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
}

impl ApplicationHandler for RiftboundApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.renderer.is_some() { return; }

        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("RIFTBOUND — Native Runtime")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0)),
            ).expect("window creation should succeed"),
        );

        match pollster::block_on(Renderer::new(
            event_loop.owned_display_handle(), window.clone(),
        )) {
            Ok(renderer) => {
                log::info!("Riftbound GPU surface initialized");
                window.request_redraw();
                self.window = Some(window);
                self.renderer = Some(renderer);
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
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.as_ref() else { return; };
        if window.id() != window_id { return; }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size.width, size.height);
                }
                if size.width > 0 && size.height > 0 { window.request_redraw(); }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() { renderer.render(); }
            }
            _ => {}
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("event loop creation should succeed");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = RiftboundApp::default();
    event_loop.run_app(&mut app).expect("Riftbound event loop should run");
}
