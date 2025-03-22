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
}

#[derive(Encode, Decode, Clone, Debug, Default)]
pub struct Players {
    pub generations: Arc<[Generation]>,
    pub positions: Arc<[Vec3]>,
    pub directions: Arc<[Vec3]>,
}

#[derive(Encode, Decode, Clone, Debug, Default)]
pub struct World {
    pub tick: Tick,
    pub players: Players,
}

#[derive(Encode, Decode, Debug)]
pub struct ClientboundMessage {
    pub world: World,
}
