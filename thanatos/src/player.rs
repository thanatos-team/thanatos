use std::sync::{LazyLock, Mutex};

use glam::{Mat4, Vec3};
use gltf::Glb;
use winit::keyboard::KeyCode;

use crate::{
    camera::Camera, input::Keyboard, mesh::Mesh, scene::Scene, system::System, world::World,
};

pub struct Player {
    position: Vec3,
    direction: Vec3,
    mesh: Mesh,
}

static PLAYER: LazyLock<Mutex<Player>> = LazyLock::new(|| {
    let glb = Glb::load(include_bytes!("../assets/Box.glb")).expect("Failed to load player model");
    let mesh = Mesh::from_glb(&glb).into_iter().next().unwrap();
    Mutex::new(Player {
        position: Vec3::ZERO,
        direction: Vec3::ZERO,
        mesh,
    })
});

impl Player {
    fn get<T, F: FnOnce(&Self) -> T>(f: F) -> T {
        f(&PLAYER.lock().unwrap())
    }

    fn update<F: FnOnce(&mut Self)>(f: F) {
        f(&mut PLAYER.lock().unwrap())
    }

    pub fn position() -> Vec3 {
        Self::get(|player| player.position)
    }
}

impl System for Player {
    fn on_frame_end() {
        let mut delta = Vec3::ZERO;

        if Keyboard::is_down(KeyCode::KeyW) {
            delta += Vec3::NEG_Y;
        }
        if Keyboard::is_down(KeyCode::KeyA) {
            delta += Vec3::X;
        }
        if Keyboard::is_down(KeyCode::KeyS) {
            delta += Vec3::Y;
        }
        if Keyboard::is_down(KeyCode::KeyD) {
            delta += Vec3::NEG_X;
        }

        Self::update(|player| player.direction = (Camera::rotation() * delta).normalize_or_zero());
    }

    fn draw(scene: &mut Scene) {
        Self::get(|player| {
            scene.add(&player.mesh, Mat4::from_translation(player.position));
        });
    }

    fn on_world_update() {
        Self::update(|player| {
            player.position = World::me().map(|me| me.position).unwrap_or(player.position)
        });

        Camera::set_centre(Self::position());
    }
}

pub struct OtherPlayers;

impl System for OtherPlayers {
    fn draw(scene: &mut Scene) {
        Player::get(|player| {
            World::current()
                .players
                .positions
                .iter()
                .for_each(|position| scene.add(&player.mesh, Mat4::from_translation(*position)))
        })
    }
}
