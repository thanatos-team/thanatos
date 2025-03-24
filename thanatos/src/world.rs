use std::sync::{Arc, LazyLock, Mutex};

use aether::{GenerationalIndex, Player, ServerboundMessage};
use anyhow::Result;
use tokio::sync::mpsc;

use crate::system::{System, Systems};

#[derive(Default)]
pub struct World {
    changed: bool,
    current: Arc<aether::World>,
    me: Option<GenerationalIndex>,
    sender: Option<mpsc::UnboundedSender<ServerboundMessage>>,
}

static WORLD: LazyLock<Mutex<World>> = LazyLock::new(|| Mutex::new(World::default()));

impl World {
    fn get<T, F: FnOnce(&Self) -> T>(f: F) -> T {
        f(&WORLD.lock().unwrap())
    }

    fn update<F: FnOnce(&mut Self)>(f: F) {
        f(&mut WORLD.lock().unwrap())
    }

    pub fn me() -> Option<Player> {
        Self::get(|world| {
            world.me.and_then(|me| {
                let generation = *world.current.players.generations.get(me.index)?;
                if generation != me.generation {
                    return None;
                }

                let position = *world.current.players.positions.get(me.index)?;
                let direction = *world.current.players.directions.get(me.index)?;

                Some(Player {
                    index: me,
                    position,
                    direction,
                })
            })
        })
    }

    pub fn current() -> Arc<aether::World> {
        Self::get(|world| world.current.clone())
    }

    pub fn set_world(new: Arc<aether::World>) {
        Self::update(|world| {
            world.changed = true;
            world.current = new;
        });
    }

    pub fn set_me(new: GenerationalIndex) {
        Self::update(|world| world.me = Some(new));
    }

    pub fn set_sender(sender: mpsc::UnboundedSender<ServerboundMessage>) {
        Self::update(|world| world.sender = Some(sender));
    }

    pub fn send(message: ServerboundMessage) -> Result<()> {
        Self::get(|world| {
            if let Some(sender) = &world.sender {
                sender.send(message)?
            }

            Ok(())
        })
    }
}

impl System for World {
    fn on_frame_end() {
        if Self::get(|world| world.changed) {
            Self::update(|world| world.changed = false);
            Systems::on_world_update();
        }
    }
}
