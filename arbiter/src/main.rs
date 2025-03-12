use std::{
    cell::Cell,
    fs::{File, OpenOptions},
    io::{Read, Result, Seek, SeekFrom, Write},
    marker::PhantomData,
    ops::{Index, IndexMut, Range},
    os::{fd::AsFd, unix::fs::FileExt},
    path::{Path, PathBuf},
    ptr,
    slice::SliceIndex,
    time::Instant,
};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use rustix::mm::{MapFlags, MsyncFlags, ProtFlags};

pub struct RawMapping {
    file: File,
    map: &'static mut [u8],
    length: usize,
    capacity: usize,
}

impl RawMapping {
    const MAP_SIZE: usize = 8 * 1024 * 1024 * 1024;
    const BLOCK_SIZE: usize = 1024 * 1024;

    pub fn new(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)?;

        let stat = rustix::fs::stat(path)?;
        let length = stat.st_size as usize;

        let map = unsafe {
            let ptr = rustix::mm::mmap(
                std::ptr::null_mut(),
                Self::MAP_SIZE,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::PRIVATE,
                file.as_fd(),
                0,
            )?;
            std::slice::from_raw_parts_mut(ptr.cast(), Self::MAP_SIZE)
        };

        Ok(Self {
            file,
            map,
            length,
            capacity: length,
        })
    }

    pub fn get(&self, range: Range<usize>) -> &[u8] {
        assert!(range.end <= self.length);

        &self.map[range]
    }

    pub fn get_mut(&mut self, range: Range<usize>) -> &mut [u8] {
        assert!(range.end <= self.length);

        &mut self.map[range]
    }

    pub fn extend_from_slice(&mut self, src: &[u8]) -> Result<()> {
        let current = self.len();
        let new = current + src.len();
        self.grow(new)?;
        self.map[current..new].copy_from_slice(src);

        Ok(())
    }

    fn grow(&mut self, length: usize) -> Result<()> {
        assert!(self.length <= length, "Mapped files can't shrink");
        self.length = length;

        if self.capacity < self.length {
            self.capacity = self.length + Self::BLOCK_SIZE - (self.length % Self::BLOCK_SIZE);
            rustix::fs::ftruncate(self.file.as_fd(), self.capacity as u64)?;
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn sync(&mut self) -> Result<()> {
        unsafe {
            rustix::mm::msync(
                self.map.as_mut_ptr().cast(),
                Self::MAP_SIZE,
                MsyncFlags::SYNC,
            )?
        }
        Ok(())
    }
}

pub struct Mapping<T: Pod + Zeroable> {
    raw: RawMapping,
    phantom: PhantomData<T>,
}

impl<T: Pod + Zeroable> Mapping<T> {
    const STRIDE: usize = size_of::<T>();

    pub fn new(path: &Path) -> Result<Self> {
        RawMapping::new(path).map(|raw| Self {
            raw,
            phantom: PhantomData,
        })
    }

    pub fn get(&self, range: Range<usize>) -> &[T] {
        let start = Self::STRIDE * range.start;
        let end = Self::STRIDE * range.end;
        bytemuck::cast_slice(self.raw.get(start..end))
    }

    pub fn get_mut(&mut self, range: Range<usize>) -> &mut [T] {
        let start = Self::STRIDE * range.start;
        let end = Self::STRIDE * range.end;
        bytemuck::cast_slice_mut(self.raw.get_mut(start..end))
    }

    pub fn extend_from_slice(&mut self, src: &[T]) -> Result<()> {
        self.raw.extend_from_slice(bytemuck::cast_slice(src))
    }

    pub fn len(&self) -> usize {
        let byte_length = self.raw.len();
        assert!(
            byte_length % Self::STRIDE == 0,
            "Raw mapping size is non-integer multiple of stride"
        );

        byte_length / Self::STRIDE
    }

    pub fn sync(&mut self) -> Result<()> {
        self.raw.sync()
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Pod, Zeroable, Debug)]
pub struct Tick(u64);

pub struct HistoryMapping<T: Pod + Zeroable> {
    raw: RawMapping,
    phantom: PhantomData<T>,
}

impl<T: Pod + Zeroable> HistoryMapping<T> {
    pub fn new(path: &Path) -> Result<Self> {
        RawMapping::new(path).map(|raw| Self {
            raw,
            phantom: PhantomData,
        })
    }

    pub fn append(&mut self, tick: Tick, start: usize, values: &[T]) -> Result<()> {
        let end = start + values.len();
        self.raw.extend_from_slice(&bytemuck::bytes_of(&tick))?;
        self.raw.extend_from_slice(&bytemuck::bytes_of(&start))?;
        self.raw.extend_from_slice(&bytemuck::bytes_of(&end))?;
        self.raw.extend_from_slice(bytemuck::cast_slice(values))?;

        Ok(())
    }

    pub fn sync(&mut self) -> Result<()> {
        self.raw.sync()
    }
}

pub struct Column<T: Pod + Zeroable> {
    data: Mapping<T>,
    history: HistoryMapping<T>,
    phantom: PhantomData<T>,
}

impl<T: Pod + Zeroable + Copy> Column<T> {
    pub fn new(data: &Path, history: &Path) -> Result<Self> {
        let data = Mapping::new(data)?;
        let history = HistoryMapping::new(history)?;

        Ok(Self {
            data,
            history,
            phantom: PhantomData,
        })
    }

    pub fn get(&self, range: Range<usize>) -> &[T] {
        self.data.get(range)
    }

    pub fn set(&mut self, start: usize, values: &[T]) -> Result<()> {
        let end = start + values.len();

        let current = self.data.get(start..end);
        self.history.append(Tick(0), start, current)?;
        self.data.get_mut(start..end).copy_from_slice(values);

        Ok(())
    }

    pub fn append(&mut self, values: &[T]) -> Result<Range<usize>> {
        let start = self.data.len();
        let end = self.data.len() + values.len();

        self.data.extend_from_slice(&values)?;
        self.history
            .append(Tick(0), start, &vec![bytemuck::zeroed(); values.len()])?;

        Ok(start..end)
    }

    pub fn remove(&mut self, range: Range<usize>) -> Result<()> {
        self.set(
            range.start,
            &vec![bytemuck::zeroed(); range.end - range.start],
        )
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn sync(&mut self) -> Result<()> {
        self.data.sync()?;
        self.history.sync()
    }
}

const MSAMPLES: f32 = 100.0;
const SAMPLES: usize = 100_000_000;

fn main() -> Result<()> {
    {
        let mut positions = Column::<Vec3>::new(
            Path::new("positions.column"),
            Path::new("positions.column.history"),
        )
        .unwrap();

        let start = Instant::now();
        let rows = vec![Vec3::ZERO; SAMPLES];
        println!(
            "{:.4}M rows/s appended (baseline)",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        let sum = rows.into_iter().sum::<Vec3>();
        println!(
            "{:.4}M rows/s read (baseline), sum {sum}",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        positions.append(&vec![Vec3::ONE; SAMPLES]).unwrap();
        println!(
            "{:.4}M rows/s appended",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        let sum = positions.get(0..SAMPLES).iter().sum::<Vec3>();
        println!(
            "{:.4}M rows/s read, sum {sum}",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        positions.set(0, &vec![Vec3::Y; SAMPLES]).unwrap();
        println!(
            "{:.4}M rows/s set",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        let start = Instant::now();
        positions.remove(0..SAMPLES).unwrap();
        println!(
            "{:.4}M rows/s removed",
            MSAMPLES / (Instant::now() - start).as_secs_f32()
        );

        loop {}
    }

    Ok(())
}
