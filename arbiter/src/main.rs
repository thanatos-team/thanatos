use std::{io::Result, num::Wrapping, path::Path, time::Instant};

use arbiter::{Column, Tick};

const MSAMPLES: f32 = 100.0;
const SAMPLES: usize = 100_000_000;

fn main() -> Result<()> {
    {
        let mut positions = Column::<u8>::new(
            Path::new("positions.column"),
            Path::new("positions.column.history"),
        )
        .unwrap();

        let tick = Tick::ZERO;

        let mut rows = Vec::new();
        let samples = vec![1; SAMPLES];
        let start = Instant::now();
        rows.extend_from_slice(&samples);
        println!(
            "{:.4} MB/s appended (baseline)",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        let sum = rows.iter().copied().map(Wrapping).sum::<Wrapping<u8>>();
        println!(
            "{:.4} MB/s read (baseline), sum {sum}",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        for i in 0..10 {
            rows.get_mut(0..SAMPLES).unwrap().copy_from_slice(&vec![i; SAMPLES]);
        }
        println!(
            "{:.4} MB/s set (baseline)",
            (MSAMPLES * 10.0) / (Instant::now() - start).as_secs_f32()
        );
        
        let samples = vec![1; SAMPLES];
        let start = Instant::now();
        positions.append(tick, &samples).unwrap();
        println!(
            "{:.4} MB/s appended",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        let sum = positions.get(0..SAMPLES).iter().sum::<u8>();
        println!(
            "{:.4} MB/s read, sum {sum}",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        let samples = vec![3; SAMPLES];
        for i in 0..10 {
            positions.set(tick, 0, &samples).unwrap();
        }
        println!(
            "{:.4} MB/s set",
            (MSAMPLES * 10.0) / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        positions.remove(tick, 0..SAMPLES).unwrap();
        println!(
            "{:.4} MB/s removed",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        loop {}
    }

    Ok(())
}
