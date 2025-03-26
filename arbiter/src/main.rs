mod world;

use aether::{
    ClientboundMessage, Generation, GenerationalIndex, PLAYER_SPEED, Players, ServerboundMessage,
    Tick,
};
use futures::Stream;
use glam::Vec3;
use log::{debug, error, info};
use std::{
    cell::Ref,
    convert::Infallible,
    error::Error,
    fmt::{Debug, Display},
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{
        ToSocketAddrs,
        tcp::{OwnedReadHalf, OwnedWriteHalf},
    },
    sync::{
        OnceCell,
        broadcast::{self, error::RecvError},
        mpsc, watch,
    },
    task::JoinSet,
};
use tokio_stream::{
    StreamExt as TokioStreamExt,
    wrappers::{UnboundedReceiverStream, WatchStream},
};
use world::World;

trait Actor<M: Debug + Send + 'static, E: Debug> {
    fn init(&mut self) -> impl Future<Output = Result<(), E>> + Send {
        async { Ok(()) }
    }

    fn handle(&mut self, message: M) -> impl Future<Output = Result<(), E>> + Send;

    fn deinit(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

trait System<E: Debug> {
    fn init(&mut self) -> impl Future<Output = Result<(), E>> + Send {
        async { Ok(()) }
    }

    fn run(&mut self) -> impl Future<Output = Result<(), E>> + Send;

    fn deinit(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

fn spawn_actor_with<M: Debug + Send + 'static, E: Debug>(
    name: impl ToString,
    mut actor: impl Actor<M, E> + Send + 'static,
    mut receiver: impl Stream<Item = M> + Unpin + Send + 'static,
) {
    let name = name.to_string();

    tokio::spawn(async move {
        info!("[{name}] started");

        if let Err(e) = actor.init().await {
            error!("[{name}] failed due to `{e:?}`");
            return;
        }

        while let Some(message) = receiver.next().await {
            debug!("[{name}] recieved: {message:#?}");
            if let Err(e) = actor.handle(message).await {
                error!("[{name}] failed due to `{e:?}`");
                break;
            }
        }

        actor.deinit().await;

        info!("[{name}] finished");
    });
}

fn spawn_actor<M: Debug + Send + 'static, E: Debug>(
    name: impl ToString,
    actor: impl Actor<M, E> + Send + 'static,
) -> mpsc::UnboundedSender<M> {
    let (sender, receiver) = mpsc::unbounded_channel();

    spawn_actor_with(name, actor, UnboundedReceiverStream::new(receiver));

    sender
}

fn spawn_system<E: Debug>(name: impl ToString, mut system: impl System<E> + Send + 'static) {
    let name = name.to_string();

    tokio::spawn(async move {
        info!("[{name}] started");

        if let Err(e) = system.init().await {
            error!("[{name}] failed due to `{e:?}`");
            return;
        }

        loop {
            if let Err(e) = system.run().await {
                error!("[{name}] failed due to `{e:?}`");
                break;
            }
        }

        system.deinit().await;

        info!("[{name}] finished");
    });
}

pub struct ConnectionReader {
    player: GenerationalIndex,
    world: Arc<World>,
    reader: OwnedReadHalf,
}

impl System<std::io::Error> for ConnectionReader {
    async fn run(&mut self) -> Result<(), std::io::Error> {
        let length = self.reader.read_u64().await? as usize;
        let mut buf = vec![0_u8; length];
        self.reader.read_exact(&mut buf).await?;

        let Ok(message) = bitcode::decode::<ServerboundMessage>(&buf) else {
            return Ok(());
        };

        match message {
            ServerboundMessage::SetDirection(direction) => {
                if let Some(d) = self
                    .world
                    .players
                    .directions_mut()
                    .await
                    .get_mut(self.player.index)
                {
                    *d = direction
                }
            }
        }

        Ok(())
    }

    async fn deinit(&mut self) {
        self.world.players.remove(self.player).await;
    }
}

#[derive(Debug)]
pub enum ConnectionWriterMessage {
    Publish(Arc<aether::World>),
}

pub struct ConnectionWriter {
    player: GenerationalIndex,
    writer: OwnedWriteHalf,
}

impl ConnectionWriter {
    async fn send(&mut self, message: &ClientboundMessage) -> Result<(), std::io::Error> {
        let bytes = bitcode::encode(message);
        self.writer.write_u64(bytes.len() as u64).await?;
        self.writer.write_all(&bytes).await?;

        Ok(())
    }
}

impl Actor<ConnectionWriterMessage, std::io::Error> for ConnectionWriter {
    async fn init(&mut self) -> Result<(), std::io::Error> {
        self.send(&ClientboundMessage::SetPlayer(self.player))
            .await?;

        Ok(())
    }

    async fn handle(&mut self, message: ConnectionWriterMessage) -> Result<(), std::io::Error> {
        match message {
            ConnectionWriterMessage::Publish(world) => {
                debug!("Updating {:?}", self.player.index);
                self.send(&ClientboundMessage::Update(world)).await.unwrap();
            }
        }

        Ok(())
    }
}

pub struct TcpListener {
    listener: tokio::net::TcpListener,
    world: Arc<World>,
    publish: watch::Receiver<Arc<aether::World>>,
}

impl TcpListener {
    pub async fn bind<T: ToSocketAddrs>(
        address: T,
        world: Arc<World>,
        publish: watch::Receiver<Arc<aether::World>>,
    ) -> Result<Self, std::io::Error> {
        tokio::net::TcpListener::bind(address)
            .await
            .map(|listener| Self {
                listener,
                world,
                publish,
            })
    }
}

impl System<std::io::Error> for TcpListener {
    async fn run(&mut self) -> Result<(), std::io::Error> {
        let (stream, addr) = self.listener.accept().await?;

        println!("{addr:?} connected");
        let (reader, writer) = stream.into_split();

        let player = self.world.players.insert(Vec3::ZERO, Vec3::ZERO).await;

        spawn_system(
            format!("{:?} Reader", player.index),
            ConnectionReader {
                player,
                reader,
                world: self.world.clone(),
            },
        );

        spawn_actor_with(
            format!("{:?} Writer", player.index),
            ConnectionWriter { player, writer },
            WatchStream::new(self.publish.clone()).map(ConnectionWriterMessage::Publish),
        );

        Ok(())
    }
}

async fn update_positions(world: &World) {
    world
        .players
        .positions_mut()
        .await
        .iter_mut()
        .zip(world.players.directions().await.iter())
        .for_each(|(position, direction)| {
            *position += *direction * PLAYER_SPEED * delta().as_secs_f32()
        })
}

const TPS: f32 = 20.0;

fn delta() -> Duration {
    Duration::from_secs_f32(1.0 / TPS)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let (publish_tx, publish_rx) = watch::channel(Default::default());
    let world = World::new();

    spawn_system(
        "TCP Listener",
        TcpListener::bind("localhost:3000", world.clone(), publish_rx).await?,
    );

    let mut last_publish = Instant::now();
    let mut tick = Tick::ZERO;

    loop {
        info!("Start {tick:?}");
        update_positions(&world).await;

        last_publish += Duration::from_secs_f32(1.0 / TPS);
        tokio::time::sleep_until(last_publish.into()).await;

        info!("Publishing {tick:?}");
        publish_tx.send_replace(Arc::new(world.to_aether().await));

        tick = tick.next();
    }
}
