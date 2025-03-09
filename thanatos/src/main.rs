#![warn(clippy::pedantic)]
#![warn(clippy::perf)]

mod camera;
mod input;
mod mesh;
mod player;
mod renderer;
mod system;

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use camera::Camera;
use input::{Keyboard, Mouse};
use player::Player;
use renderer::Renderer;
use system::Systems;
use winit::application::ApplicationHandler;
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct App<'a> {
    renderer: Option<Renderer<'a>>,
    window: Option<Arc<Window>>,
    systems: Systems,
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );
        self.renderer = pollster::block_on(Renderer::new(window.clone())).ok();
        self.window = Some(window);

        self.systems.register::<Camera>();
        self.systems.register::<Mouse>();
        self.systems.register::<Keyboard>();
        self.systems.register::<Player>();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.systems.on_window_event(&event);

        match event {
            WindowEvent::Resized(new_size) => {
                self.renderer
                    .as_ref()
                    .unwrap()
                    .resize(new_size.width.max(1), new_size.height.max(1));
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let start = Instant::now();
                let frame = self
                    .renderer
                    .as_ref()
                    .unwrap()
                    .draw(self.window.as_ref().unwrap(), &[Player::draw()]);
                self.window.as_mut().unwrap().pre_present_notify();
                frame.present();

                self.systems.on_frame_end();
                println!("{:4.0} fps", 1.0 / (Instant::now() - start).as_secs_f32());

                self.window.as_ref().unwrap().request_redraw();
            }
            _ => (),
        }
    }
}

fn main() -> Result<(), EventLoopError> {
    let event_loop = EventLoop::new().unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app)
}
