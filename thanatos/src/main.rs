#![warn(clippy::pedantic)]
#![warn(clippy::perf)]

mod camera;
mod input;
mod mesh;
mod player;
mod renderer;
mod scene;
mod system;
mod world;

use std::sync::{Arc, LazyLock};
use std::time::Instant;

use aether::ClientboundMessage;
use anyhow::Result;
use camera::Camera;
use input::{Keyboard, Mouse};
use player::Player;
use renderer::Renderer;
use system::Systems;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedReadHalf;
use tokio::sync::watch;
use winit::application::ApplicationHandler;
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};
use world::World;

#[derive(Default)]
struct App<'a> {
    renderer: Option<Renderer<'a>>,
    window: Option<Arc<Window>>,
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

        Systems::register::<Camera>();
        Systems::register::<Mouse>();
        Systems::register::<Keyboard>();
        Systems::register::<Player>();
        Systems::register::<World>()
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        Systems::on_window_event(&event);

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
                    .draw(self.window.as_ref().unwrap(), Systems::draw());
                self.window.as_mut().unwrap().pre_present_notify();
                frame.present();

                Systems::on_frame_end();
                println!("{:4.0} fps", 1.0 / (Instant::now() - start).as_secs_f32());

                self.window.as_ref().unwrap().request_redraw();
            }
            _ => (),
        }
    }
}

async fn handle_read(
    mut reader: OwnedReadHalf,
    sender: watch::Sender<aether::World>,
) -> Result<()> {
    loop {
        let length = reader.read_u64().await? as usize;
        let mut buf = vec![0_u8; length];
        reader.read_exact(&mut buf).await?;
        let message = bitcode::decode::<ClientboundMessage>(&buf)?;

        sender.send_replace(message.world);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    for _ in 0..1_000 {
        let stream = TcpStream::connect("localhost:3000").await?;
        let (reader, mut writer) = stream.into_split();
        writer.write_u64(13).await?;

        let (sender, receiver) = watch::channel(aether::World::default());
        World::set_receiver(receiver);

        tokio::spawn(handle_read(reader, sender));
    }

    let event_loop = EventLoop::new().unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app)?;

    Ok(())
}
