use std::{io::Result, num::Wrapping, path::Path, time::Instant};

use arbiter::{Column, Players, Tick};
use glam::{Vec3, Vec3A};

fn main() -> Result<()> {
    let mut players = Players::new()?;

    for i in 0..1_000_000 {
        players.insert(Vec3::new(i as f32, 0.0, 0.0), Vec3::X, 3.0)?;
    }

    let start = Instant::now();
    for _ in 0..1_000 {
        let positions = players
            .positions()
            .iter()
            .zip(players.directions())
            .zip(players.speeds())
            .map(|((position, direction), speed)| position + (direction * speed))
            .collect::<Vec<_>>();
        players.positions_mut().copy_from_slice(&positions);
    }
    println!("{:.4}ticks/s", 1_000.0 / (Instant::now() - start).as_secs_f32());

    Ok(())
}
