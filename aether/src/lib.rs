use std::sync::Arc;

use bitcode::{Decode, Encode};
use glam::Vec3;

#[derive(Encode, Decode, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Tick(usize);

impl Tick {
    pub const ZERO: Self = Self(0);

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Generation(usize);

impl Generation {
    pub const ZERO: Self = Self(0);

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    pub fn is_dead(&self) -> bool {
        self.0 % 2 == 0
    }
}

#[derive(Encode, Decode, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GenerationalIndex {
    pub generation: Generation,
    pub index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub index: GenerationalIndex,
    pub position: Vec3,
    pub direction: Vec3
}

#[derive(Encode, Decode, Debug, Default)]
pub struct Players {
    pub generations: Box<[Generation]>,
    pub positions: Box<[Vec3]>,
    pub directions: Box<[Vec3]>,
}

#[derive(Encode, Decode, Debug, Default)]
pub struct World {
    pub tick: Tick,
    pub players: Players,
}


#[derive(Encode, Decode, Debug)]
pub enum ClientboundMessage {
    Update(Arc<World>),
    SetPlayer(GenerationalIndex),
}

#[derive(Encode, Decode, Debug)]
pub enum ServerboundMessage {
    SetDirection(Vec3)
}

pub const PLAYER_SPEED: f32 = 15.0;
pub const ALLOWED_POSITION_DIFFERENCE: f32 = 50.0;
