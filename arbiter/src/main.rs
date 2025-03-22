use aether::{ClientboundMessage, Generation, Players, Tick, World};
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
};
use tokio_stream::{
    StreamExt as TokioStreamExt,
    wrappers::{UnboundedReceiverStream, WatchStream},
};

trait Actor<M: Debug + Send + 'static, E: Debug> {
    fn handle(&mut self, message: M) -> impl Future<Output = Result<(), E>> + Send;
}

trait System<E: Debug> {
    fn run(&mut self) -> impl Future<Output = Result<(), E>> + Send;
}

fn spawn_actor_with<M: Debug + Send + 'static, E: Debug>(
    name: impl ToString,
    mut actor: impl Actor<M, E> + Send + 'static,
    mut receiver: impl Stream<Item = M> + Unpin + Send + 'static,
) {
    let name = name.to_string();

    tokio::spawn(async move {
        info!("[{name}] started");

        while let Some(message) = receiver.next().await {
            debug!("[{name}] recieved: {message:#?}");
            if let Err(e) = actor.handle(message).await {
                error!("[{name}] failed due to `{e:?}`");
                return;
            }
        }

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

        loop {
            if let Err(e) = system.run().await {
                error!("[{name}] failed due to `{e:?}`");
                return;
            }
        }
    });
}

#[derive(Debug)]
pub enum ColumnUpdaterMessage<T: Debug + Clone + Send + Sync + 'static> {
    Set(usize, T),
    Append(T),
    Tick,
}

pub struct ColumnUpdater<T: Debug + Clone + Send + Sync + 'static> {
    sender: watch::Sender<Arc<[T]>>,
    set: Vec<(usize, T)>,
    append: Vec<T>,
}

impl<T: Debug + Clone + Send + Sync + 'static> ColumnUpdater<T> {
    pub fn new(sender: watch::Sender<Arc<[T]>>) -> Self {
        Self {
            sender,
            set: Vec::new(),
            append: Vec::new(),
        }
    }
}

impl<T: Debug + Clone + Send + Sync + 'static> Actor<ColumnUpdaterMessage<T>, Infallible>
    for ColumnUpdater<T>
{
    async fn handle(&mut self, message: ColumnUpdaterMessage<T>) -> Result<(), Infallible> {
        match message {
            ColumnUpdaterMessage::Set(index, value) => self.set.push((index, value)),
            ColumnUpdaterMessage::Append(value) => self.append.push(value),
            ColumnUpdaterMessage::Tick => {
                let mut data = self.sender.borrow().to_vec();
                self.set
                    .drain(..)
                    .for_each(|(index, value)| data[index] = value);
                self.append.drain(..).for_each(|value| data.push(value));

                self.sender.send_replace(data.into_boxed_slice().into());
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct PlayersStaging {
    generations: watch::Receiver<Arc<[Generation]>>,
    positions: watch::Receiver<Arc<[Vec3]>>,
    directions: watch::Receiver<Arc<[Vec3]>>,
}

impl PlayersStaging {
    pub fn current(&self) -> Players {
        Players {
            generations: self.generations.borrow().clone(),
            positions: self.positions.borrow().clone(),
            directions: self.directions.borrow().clone(),
        }
    }

    pub async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        tokio::try_join!(
            self.generations.changed(),
            self.positions.changed(),
            self.directions.changed()
        )?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct WorldStaging {
    tick: watch::Receiver<Tick>,
    players: PlayersStaging,
    sender: watch::Sender<World>,
}

impl WorldStaging {
    fn current(&self) -> World {
        World {
            tick: self.tick.borrow().clone(),
            players: self.players.current(),
        }
    }

    async fn changed(&mut self) -> Result<(), watch::error::RecvError> {
        tokio::try_join!(self.tick.changed(), self.players.changed())?;

        Ok(())
    }

    pub fn subscribe(&self) -> watch::Receiver<World> {
        self.sender.subscribe()
    }
}

impl System<watch::error::RecvError> for WorldStaging {
    async fn run(&mut self) -> Result<(), watch::error::RecvError> {
        self.changed().await?;
        self.sender.send_replace(self.current());
        Ok(())
    }
}

pub struct ConnectionReader {
    id: ClientId,
    reader: OwnedReadHalf,
}

impl System<std::io::Error> for ConnectionReader {
    async fn run(&mut self) -> Result<(), std::io::Error> {
        let length = self.reader.read_u64().await?;
        println!("{:?} {length:?}", self.id);

        Ok(())
    }
}

#[derive(Debug)]
pub enum ConnectionWriterMessage {
    Tick(World),
}

pub struct ConnectionWriter {
    id: ClientId,
    writer: OwnedWriteHalf,
}

impl Actor<ConnectionWriterMessage, std::io::Error> for ConnectionWriter {
    async fn handle(&mut self, message: ConnectionWriterMessage) -> Result<(), std::io::Error> {
        match message {
            ConnectionWriterMessage::Tick(world) => {
                debug!("Updating {:?}", self.id);
                let bytes = bitcode::encode(&ClientboundMessage { world });
                self.writer.write_u64(bytes.len() as u64).await?;
                self.writer.write_all(&bytes).await?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClientId(u64);

impl ClientId {
    pub fn next(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

pub struct TcpListener {
    listener: tokio::net::TcpListener,
    next_id: ClientId,
    world: watch::Receiver<World>,
}

impl TcpListener {
    pub async fn bind<T: ToSocketAddrs>(
        address: T,
        world: watch::Receiver<World>,
    ) -> Result<Self, std::io::Error> {
        tokio::net::TcpListener::bind(address)
            .await
            .map(|listener| Self {
                listener,
                next_id: ClientId(0),
                world,
            })
    }
}

impl System<std::io::Error> for TcpListener {
    async fn run(&mut self) -> Result<(), std::io::Error> {
        let (stream, addr) = self.listener.accept().await?;
        let id = self.next_id;
        self.next_id.next();

        println!("{addr:?} connected");
        let (reader, writer) = stream.into_split();

        spawn_system(format!("{id:?} Reader"), ConnectionReader { id, reader });
        spawn_actor_with(
            format!("{id:?} Writer"),
            ConnectionWriter { id, writer },
            WatchStream::new(self.world.clone()).map(ConnectionWriterMessage::Tick),
        );

        Ok(())
    }
}

#[derive(Debug)]
pub enum TickUpdaterMessage {
    Tick(World),
}

pub struct TickUpdater {
    previous: Instant,
    sender: watch::Sender<Tick>,
}

impl TickUpdater {
    const TPS: f32 = 20.0;

    pub fn new(sender: watch::Sender<Tick>) -> Self {
        Self {
            previous: Instant::now(),
            sender,
        }
    }
}

impl Actor<TickUpdaterMessage, Infallible> for TickUpdater {
    async fn handle(&mut self, message: TickUpdaterMessage) -> Result<(), Infallible> {
        match message {
            TickUpdaterMessage::Tick(_) => {
                let until = self.previous + Duration::from_secs_f32(1.0 / Self::TPS);
                tokio::time::sleep_until(until.into()).await;
                self.previous = until;

                self.sender.send_modify(|tick| *tick = tick.next())
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum TpsLoggerMessage {
    Tick(World),
}

pub struct TpsLogger {
    previous: Instant,
}

impl Actor<TpsLoggerMessage, ()> for TpsLogger {
    async fn handle(&mut self, _: TpsLoggerMessage) -> Result<(), ()> {
        let now = Instant::now();
        info!("{:.4} tps", 1.0 / (now - self.previous).as_secs_f32());
        self.previous = now;

        Ok(())
    }
}

pub struct Column<T: Clone + Debug + Sync + Send + 'static> {
    current: watch::Sender<Arc<[T]>>,
    messages: mpsc::UnboundedSender<ColumnUpdaterMessage<T>>,
}

impl<T: Clone + Debug + Sync + Send + 'static> Column<T> {
    pub fn new(
        name: impl ToString,
        sender: watch::Sender<Arc<[T]>>,
        world: watch::Receiver<World>,
    ) -> Self {
        let (messages_tx, messages_rx) = mpsc::unbounded_channel();

        spawn_actor_with(
            format!("{} Updater", name.to_string()),
            ColumnUpdater::new(sender.clone()),
            WatchStream::new(world)
                .map(|_| ColumnUpdaterMessage::Tick)
                .merge(UnboundedReceiverStream::new(messages_rx)),
        );

        Self {
            current: sender,
            messages: messages_tx,
        }
    }

    pub fn get(&self) -> Arc<[T]> {
        self.current.borrow().clone()
    }

    pub fn set(
        &self,
        index: usize,
        value: T,
    ) -> Result<(), mpsc::error::SendError<ColumnUpdaterMessage<T>>> {
        self.messages.send(ColumnUpdaterMessage::Set(index, value))
    }
}

pub struct PlayersTable {
    pub generations: Column<Generation>,
    pub positions: Column<Vec3>,
    pub directions: Column<Vec3>,
}

pub struct Context {
    pub players: PlayersTable,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let (generations_tx, generations_rx) = watch::channel(Default::default());
    let (positions_tx, positions_rx) =
        watch::channel(vec![Vec3::ZERO; 1_000].into_boxed_slice().into());
    let (directions_tx, directions_rx) = watch::channel(Default::default());

    let (tick_tx, tick_rx) = watch::channel(Tick::ZERO);

    let (world_tx, world_rx) = watch::channel(World::default());

    spawn_system(
        "World Staging",
        WorldStaging {
            tick: tick_rx,
            players: PlayersStaging {
                generations: generations_rx,
                positions: positions_rx,
                directions: directions_rx,
            },
            sender: world_tx,
        },
    );

    spawn_system(
        "TCP Listener",
        TcpListener::bind("localhost:3000", world_rx.clone()).await?,
    );

    spawn_actor_with(
        "Tick Updater",
        TickUpdater::new(tick_tx),
        WatchStream::new(world_rx.clone()).map(TickUpdaterMessage::Tick),
    );

    spawn_actor_with(
        "TPS Logger",
        TpsLogger {
            previous: Instant::now(),
        },
        WatchStream::new(world_rx.clone()).map(TpsLoggerMessage::Tick),
    );

    let players = PlayersTable {
        generations: Column::new("Generations", generations_tx, world_rx.clone()),
        positions: Column::new("Positions", positions_tx, world_rx.clone()),
        directions: Column::new("Directions", directions_tx, world_rx.clone()),
    };

    let ctx = Context { players };

    loop {}
}
