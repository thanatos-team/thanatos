use std::{
    cell::LazyCell,
    rc::Rc,
    sync::{Arc, LazyLock, Mutex},
};

use glam::{Mat4, Quat, Vec3};
use gltf::Glb;
use winit::keyboard::KeyCode;

use crate::{camera::Camera, input::Keyboard, mesh::Mesh, system::System};

pub struct Player {
    position: Vec3,
    mesh: Mesh,
}

static PLAYER: LazyLock<Mutex<Player>> = LazyLock::new(|| {
    let glb = Glb::load(include_bytes!("../assets/Box.glb")).expect("Failed to load player model");
    let mesh = Mesh::from_glb(&glb).into_iter().next().unwrap();
    Mutex::new(Player {
        position: Vec3::ZERO,
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

    pub fn draw() -> Mesh {
        Self::get(|player| {
            let mut mesh = player.mesh.clone();
            mesh.info.transform = mesh.info.transform * Mat4::from_translation(player.position);
            mesh
        })
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

        delta = Camera::rotation() * delta;
        Self::update(|player| player.position += delta);
        Camera::set_centre(Player::position());
    }
}
