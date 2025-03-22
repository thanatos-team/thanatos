use std::sync::{LazyLock, Mutex};

use aether::Players;
use gltf::{Glb, Mesh};
use tokio::sync::watch;

use crate::system::{System, Systems};

pub struct World {
    current: aether::World,
    receiver: Option<watch::Receiver<aether::World>>,
}

static PLAYER: LazyLock<Mutex<World>> = LazyLock::new(|| {
    Mutex::new(World {
        current: aether::World::default(),
        receiver: None,
    })
});

impl World {
    fn get<T, F: FnOnce(&Self) -> T>(f: F) -> T {
        f(&PLAYER.lock().unwrap())
    }

    fn update<F: FnOnce(&mut Self)>(f: F) {
        f(&mut PLAYER.lock().unwrap())
    }

    pub fn players() -> Players {
        Self::get(|world| world.current.players.clone())
    }

    pub fn set_receiver(receiver: watch::Receiver<aether::World>) {
        Self::update(|world| world.receiver = Some(receiver));
    }
}

impl System for World {
    fn on_frame_end() {
        World::update(|world| {
            if let Some(receiver) = &world.receiver {
                world.current = receiver.borrow().clone()
            }
        });

        Systems::on_world_update();
    }
}
