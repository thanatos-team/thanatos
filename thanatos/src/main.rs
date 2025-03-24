#![warn(clippy::pedantic)]
#![warn(clippy::perf)]

mod camera;
mod input;
mod mesh;
mod player;
mod renderer;
mod scene;
mod system;
mod time;
mod world;

use std::sync::{Arc, LazyLock};
use std::time::Instant;

use aether::{ClientboundMessage, GenerationalIndex, ServerboundMessage};
use anyhow::Result;
use camera::Camera;
use input::{Keyboard, Mouse};
use player::{OtherPlayers, Player};
use renderer::Renderer;
use system::Systems;
use time::Clock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, oneshot, watch};
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
        Systems::register::<World>();
        Systems::register::<OtherPlayers>();
        Systems::register::<Clock>();
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

async fn handle_read(mut reader: OwnedReadHalf) -> Result<()> {
    loop {
        let length = reader.read_u64().await? as usize;
        let mut buf = vec![0_u8; length];
        reader.read_exact(&mut buf).await?;
        match bitcode::decode::<ClientboundMessage>(&buf)? {
            ClientboundMessage::Update(world) => World::set_world(world),
            ClientboundMessage::SetPlayer(me) => World::set_me(me),
        }
    }
}

async fn handle_write(mut writer: OwnedWriteHalf) -> Result<()> {
    let (sender, mut receiver) = mpsc::unbounded_channel();

    World::set_sender(sender);

    while let Some(message) = receiver.recv().await {
        let buf = bitcode::encode(&message);
        writer.write_u64(buf.len() as u64).await?;
        writer.write_all(&buf).await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let stream = TcpStream::connect("localhost:3000").await?;
    let (reader, writer) = stream.into_split();
    tokio::spawn(handle_read(reader));
    tokio::spawn(handle_write(writer));

    let event_loop = EventLoop::new().unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app)?;

    Ok(())
}
