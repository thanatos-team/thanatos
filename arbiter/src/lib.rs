#![feature(iter_array_chunks)]
#![feature(seek_stream_len)]

use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::{Read, Result, Seek, SeekFrom, Write},
    marker::PhantomData,
    ops::Range,
    os::fd::AsFd,
    path::Path,
};

use bytemuck::{Pod, Zeroable};
use rustix::mm::{MapFlags, MsyncFlags, ProtFlags};

pub struct Blocks {
    file: File,
    map: &'static mut [u8],
    dirty: BTreeSet<usize>,
    length: usize,
    capacity: usize,
}

impl Blocks {
    const BLOCK_SIZE: usize = 1024 * 1024;
    const MAP_SIZE: usize = 8 * 1024 * 1024 * 1024;

    pub fn new(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path)?;

        let map = unsafe {
            let ptr = rustix::mm::mmap_anonymous(
                std::ptr::null_mut(),
                Self::MAP_SIZE,
                ProtFlags::empty(),
                MapFlags::PRIVATE | MapFlags::NORESERVE,
            )?;
            std::slice::from_raw_parts_mut(ptr.cast(), Self::MAP_SIZE)
        };

        let mut blocks = Self {
            file,
            map,
            dirty: BTreeSet::new(),
            length: 0,
            capacity: 0,
        };

        blocks.grow(blocks.length)?;

        Ok(blocks)
    }

    pub fn get(&self, range: Range<usize>) -> &[u8] {
        assert!(self.capacity >= range.end);
        &self.map[range]
    }

    pub fn get_mut(&mut self, range: Range<usize>) -> &mut [u8] {
        assert!(self.capacity >= range.end);

        let start_block = range.start / Self::BLOCK_SIZE;
        let end_block = range.end / Self::BLOCK_SIZE;
        (start_block..=end_block).for_each(|index| {
            self.dirty.insert(index);
        });

        &mut self.map[range]
    }

    pub fn extend_from_slice(&mut self, src: &[u8]) -> Result<()> {
        let current = self.len();
        let new = current + src.len();
        self.grow(new)?;
        self.get_mut(current..new).copy_from_slice(src);

        Ok(())
    }

    pub fn extend_zeroed(&mut self, length: usize) -> Result<()> {
        let current = self.len();
        let new = current + length;
        self.grow(new)?;

        Ok(())
    }

    fn grow(&mut self, length: usize) -> Result<()> {
        assert!(self.length <= length, "Mapped files can't shrink");
        self.length = length;
        let aligned_length = self.length + Self::BLOCK_SIZE - (self.length % Self::BLOCK_SIZE);
        if aligned_length <= self.capacity {
            return Ok(());
        }

        rustix::fs::ftruncate(self.file.as_fd(), aligned_length as u64)?;

        let new_blocks = (aligned_length - self.capacity) / Self::BLOCK_SIZE;
        (0..new_blocks)
            .map(|i| self.capacity + (i * Self::BLOCK_SIZE))
            .for_each(|offset| unsafe {
                rustix::mm::mmap(
                    self.map[offset..offset + Self::BLOCK_SIZE]
                        .as_mut_ptr()
                        .cast(),
                    Self::BLOCK_SIZE,
                    ProtFlags::READ | ProtFlags::WRITE,
                    MapFlags::PRIVATE | MapFlags::FIXED,
                    self.file.as_fd(),
                    offset as u64,
                )
                .unwrap();
            });

        self.capacity = aligned_length;

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.length
    }

    fn write_diff(&self, tick: Tick, old: &[u8], history: &mut impl Write) -> Result<()> {
        let mut header = bytemuck::bytes_of(&tick).to_vec();
        header.extend_from_slice(bytemuck::bytes_of(&self.dirty.len()));
        header.extend_from_slice(&bytemuck::cast_slice(
            &self.dirty.iter().copied().collect::<Vec<_>>(),
        ));
        history.write_all(&header)?;

        let mut diff = Vec::with_capacity(Self::BLOCK_SIZE);
        self.dirty
            .iter()
            .map(|block| block * Self::BLOCK_SIZE)
            .map(|offset| {
                diff.extend(
                    self.map[offset..offset + Self::BLOCK_SIZE]
                        .iter()
                        .zip(&old[offset..offset + Self::BLOCK_SIZE])
                        .map(|(new, old)| old - new),
                );
                history.write_all(&mut diff)?;
                diff.clear();
                Ok(())
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(())
    }

    pub fn sync(&mut self, tick: Tick, history: &mut impl Write) -> Result<()> {
        let old: &mut [u8] = unsafe {
            let ptr = rustix::mm::mmap(
                std::ptr::null_mut(),
                Self::MAP_SIZE,
                ProtFlags::READ | ProtFlags::WRITE,
                MapFlags::SHARED,
                self.file.as_fd(),
                0,
            )?;
            std::slice::from_raw_parts_mut(ptr.cast(), Self::MAP_SIZE)
        };

        self.write_diff(tick, old, history)?;

        self.dirty
            .iter()
            .map(|block| block * Self::BLOCK_SIZE)
            .for_each(|offset| {
                old[offset..offset + Self::BLOCK_SIZE]
                    .copy_from_slice(&self.map[offset..offset + Self::BLOCK_SIZE]);
            });

        unsafe {
            rustix::mm::msync(old.as_mut_ptr().cast(), Self::MAP_SIZE, MsyncFlags::SYNC)?;
            rustix::mm::munmap(old.as_mut_ptr().cast(), Self::MAP_SIZE)?;
        }

        self.dirty.clear();

        Ok(())
    }

    pub fn apply(&mut self, diff: &[u8]) {
        let num_blocks = diff.len() / (size_of::<usize>() + Self::BLOCK_SIZE);
        let diff_start = num_blocks * size_of::<usize>();
        let block_indices: &[usize] = bytemuck::cast_slice(&diff[0..diff_start]);

        let diff = &diff[diff_start..];
        block_indices
            .iter()
            .zip(diff.chunks_exact(Self::BLOCK_SIZE))
            .for_each(|(block_index, diff)| {
                self.dirty.insert(*block_index);
                self.map[(block_index * Self::BLOCK_SIZE)..(block_index + 1) * Self::BLOCK_SIZE]
                    .iter_mut()
                    .zip(diff)
                    .for_each(|(current, diff)| *current += diff);
            });
    }
}

pub struct Mapping<T: Pod + Zeroable> {
    raw: Blocks,
    phantom: PhantomData<T>,
}

impl<T: Pod + Zeroable> Mapping<T> {
    const STRIDE: usize = size_of::<T>();

    pub fn new(path: &Path) -> Result<Self> {
        Blocks::new(path).map(|raw| Self {
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

    pub fn extend_zeroed(&mut self, length: usize) -> Result<()> {
        self.raw.extend_zeroed(Self::STRIDE * length)
    }

    pub fn len(&self) -> usize {
        let byte_length = self.raw.len();
        assert!(
            byte_length % Self::STRIDE == 0,
            "Raw mapping size is non-integer multiple of stride"
        );

        byte_length / Self::STRIDE
    }

    pub fn sync(&mut self, tick: Tick, history: &mut impl Write) -> Result<()> {
        self.raw.sync(tick, history)
    }

    pub fn apply(&mut self, diff: &[u8]) {
        self.raw.apply(diff)
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Pod, Zeroable, Debug)]
pub struct Tick(u64);

impl Tick {
    pub const ZERO: Self = Tick(0);

    pub const fn next(&mut self) -> Self {
        self.0 += 1;
        *self
    }
}

pub struct Column<T: Pod + Zeroable> {
    data: Mapping<T>,
    history: File,
    phantom: PhantomData<T>,
}

impl<T: Pod + Zeroable> Column<T> {
    pub fn new(data: &Path, history: &Path) -> Result<Self> {
        let data = Mapping::new(data)?;
        let history = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(history)?;

        Ok(Self {
            data,
            history,
            phantom: PhantomData,
        })
    }

    pub fn get(&mut self, range: Range<usize>) -> &[T] {
        self.data.get(range)
    }

    pub fn set(&mut self, start: usize, values: &[T]) -> Result<()> {
        let end = start + values.len();

        self.data.get_mut(start..end).copy_from_slice(values);

        Ok(())
    }

    pub fn append(&mut self, values: &[T]) -> Result<Range<usize>> {
        let start = self.data.len();
        self.data.extend_from_slice(values)?;
        Ok(start..start + values.len())
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

    pub fn sync(&mut self, tick: Tick) -> Result<()> {
        self.data.sync(tick, &mut self.history)?;

        Ok(())
    }

    pub fn restore(&mut self, to: Tick) -> Result<()> {
        self.history.seek(SeekFrom::Start(0))?;

        loop {
            let mut header_buf = [0_u8; size_of::<Tick>() + size_of::<usize>()];
            self.history.read_exact(&mut header_buf)?;

            let tick = *bytemuck::from_bytes::<Tick>(&header_buf[..size_of::<Tick>()]);
            let num_blocks = *bytemuck::from_bytes::<usize>(&header_buf[size_of::<Tick>()..]);

            let length = num_blocks * (size_of::<usize>() + Blocks::BLOCK_SIZE);
            self.history.seek_relative(length as i64)?;

            if tick == to {
                break;
            }
        }

        while self.history.stream_position()? < self.history.stream_len()? {
            let mut header_buf = [0_u8; size_of::<Tick>() + size_of::<usize>()];
            self.history.read_exact(&mut header_buf)?;

            let tick = *bytemuck::from_bytes::<Tick>(&header_buf[..size_of::<Tick>()]);
            println!("Undoing sync {}", tick.0);
            let num_blocks = *bytemuck::from_bytes::<usize>(&header_buf[size_of::<Tick>()..]);

            let length = num_blocks * (size_of::<usize>() + Blocks::BLOCK_SIZE);
            let mut diff = vec![0; length];
            self.history.read_exact(&mut diff)?;
            self.data.apply(&diff);
        }

        assert_eq!(self.history.stream_position()?, self.history.stream_len()?);

        Ok(())
    }
}
