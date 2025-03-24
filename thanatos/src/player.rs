use std::{
    sync::{LazyLock, Mutex},
    time::Duration,
};

use aether::{ALLOWED_POSITION_DIFFERENCE, PLAYER_SPEED};
use glam::{Mat4, Vec3};
use gltf::Glb;
use log::warn;
use winit::keyboard::KeyCode;

use crate::{
    camera::Camera, input::Keyboard, mesh::Mesh, scene::Scene, system::System, time::Clock,
    world::World,
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

        let direction = (Camera::rotation() * delta).normalize_or_zero();
        Self::update(|player| {
            player.direction = direction;
            player.position += direction * PLAYER_SPEED * Clock::delta().as_secs_f32();
            println!("{:?}", player.position);
        });

        Camera::set_centre(Self::position());
        World::send(aether::ServerboundMessage::SetDirection(direction)).unwrap();
    }

    fn draw(scene: &mut Scene) {
        Self::get(|player| {
            scene.add(&player.mesh, Mat4::from_translation(player.position));
        });
    }

    fn on_world_update() {
        Self::update(|player| {
            let server_position = World::me().map(|me| me.position).unwrap_or(player.position);

            let distance = server_position.distance(player.position);
            println!(
                "Client: {} Server: {} Distance: {distance}",
                player.position, server_position
            );

            if distance > ALLOWED_POSITION_DIFFERENCE {
                warn!("Rubber banding");
                player.position = server_position;
            }
        });
        Camera::set_centre(Self::position());
    }
}

pub struct OtherPlayers {
    positions: Vec<Vec3>,
    directions: Vec<Vec3>,
}

static OTHER_PLAYERS: Mutex<OtherPlayers> = Mutex::new(OtherPlayers {
    positions: Vec::new(),
    directions: Vec::new(),
});

impl OtherPlayers {
    fn get<T, F: FnOnce(&Self) -> T>(f: F) -> T {
        f(&OTHER_PLAYERS.lock().unwrap())
    }

    fn update<F: FnOnce(&mut Self)>(f: F) {
        f(&mut OTHER_PLAYERS.lock().unwrap())
    }
}

impl System for OtherPlayers {
    fn on_frame_end() {
        Self::update(|others| {
            others
                .positions
                .iter_mut()
                .zip(&others.directions)
                .for_each(|(position, direction)| {
                    *position += direction * PLAYER_SPEED * Clock::delta().as_secs_f32()
                });
        });
    }

    fn on_world_update() {
        let me = World::me();
        let world = World::current();
        Self::update(|others| {
            (others.positions, others.directions) = world
                .players
                .generations
                .iter()
                .zip(
                    world
                        .players
                        .positions
                        .iter()
                        .zip(&world.players.directions),
                )
                .enumerate()
                .filter(|(index, (generation, _))| {
                    me.map(|me| me.index.index != *index).unwrap_or(true) && !generation.is_dead()
                })
                .map(|(_, (_, (position, direction)))| (*position, *direction))
                .unzip();
        });
    }

    fn draw(scene: &mut Scene) {
        Player::get(|player| {
            Self::get(|others| {
                others
                    .positions
                    .iter()
                    .for_each(|position| scene.add(&player.mesh, Mat4::from_translation(*position)))
            })
        })
    }
}
