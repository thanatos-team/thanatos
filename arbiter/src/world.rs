use std::sync::{Arc, Mutex};

use aether::{Generation, GenerationalIndex, Tick};
use glam::Vec3;
use tokio::sync::{RwLock, RwLockMappedWriteGuard, RwLockReadGuard, RwLockWriteGuard, mpsc};

#[derive(Default, Debug)]
pub struct Players {
    generations: RwLock<Vec<Generation>>,
    positions: RwLock<Vec<Vec3>>,
    directions: RwLock<Vec<Vec3>>,
}

impl Players {
    pub async fn generations(&self) -> RwLockReadGuard<'_, [Generation]> {
        RwLockReadGuard::map(self.generations.read().await, |generations| {
            generations.as_slice()
        })
    }

    pub async fn positions(&self) -> RwLockReadGuard<'_, [Vec3]> {
        RwLockReadGuard::map(self.positions.read().await, |positions| {
            positions.as_slice()
        })
    }

    pub async fn positions_mut(&self) -> RwLockMappedWriteGuard<'_, [Vec3]> {
        RwLockWriteGuard::map(self.positions.write().await, |positions| {
            positions.as_mut_slice()
        })
    }

    pub async fn directions(&self) -> RwLockReadGuard<'_, [Vec3]> {
        RwLockReadGuard::map(self.directions.read().await, |directions| {
            directions.as_slice()
        })
    }

    pub async fn directions_mut(&self) -> RwLockMappedWriteGuard<'_, [Vec3]> {
        RwLockWriteGuard::map(self.directions.write().await, |directions| {
            directions.as_mut_slice()
        })
    }

    pub async fn insert(&self, position: Vec3, direction: Vec3) -> GenerationalIndex {
        let mut generations = self.generations.write().await;
        let mut positions = self.positions.write().await;
        let mut directions = self.directions.write().await;

        if let Some((index, generation)) = generations
            .iter()
            .enumerate()
            .find(|(_, generation)| generation.is_dead())
        {
            let generation = generation.next();
            generations[index] = generation;
            positions[index] = position;
            directions[index] = direction;

            GenerationalIndex { index, generation }
        } else {
            let generation = Generation::ZERO.next();
            generations.push(generation);
            positions.push(position);
            directions.push(direction);

            GenerationalIndex {
                index: generations.len() - 1,
                generation,
            }
        }
    }

    pub async fn remove(&self, index: GenerationalIndex) -> bool {
        let mut generations = self.generations.write().await;

        if generations[index.index] == index.generation {
            generations[index.index] = index.generation.next();
            true
        } else {
            false
        }
    }
}

#[derive(Debug)]
pub struct World {
    pub players: Players,
}

impl World {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            players: Players::default(),
        })
    }

    pub async fn to_aether(&self) -> aether::World {
        aether::World {
            tick: Tick::ZERO,
            players: aether::Players {
                generations: self
                    .players
                    .generations
                    .read()
                    .await
                    .clone()
                    .into_boxed_slice(),
                positions: self
                    .players
                    .positions
                    .read()
                    .await
                    .clone()
                    .into_boxed_slice(),
                directions: self
                    .players
                    .directions
                    .read()
                    .await
                    .clone()
                    .into_boxed_slice(),
            },
        }
    }
}
