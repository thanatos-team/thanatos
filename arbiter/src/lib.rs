#![feature(iter_array_chunks)]

use std::{
    borrow::Cow,
    cell::Cell,
    collections::{BTreeMap, HashMap},
    fs::{File, OpenOptions},
    io::{Read, Result, Seek, SeekFrom, Write},
    marker::PhantomData,
    ops::{Index, IndexMut, Range},
    os::{
        fd::{AsFd, OwnedFd},
        unix::fs::{FileExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    ptr,
    slice::SliceIndex,
    time::Instant,
};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use rustix::{
    fs::{Mode, OFlags},
    mm::{MapFlags, MsyncFlags, ProtFlags},
};

pub struct Blocks {
    file: File,
    map: &'static mut [u8],
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

        let stat = rustix::fs::stat(path)?;
        let length = stat.st_size as usize;

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

        if self.capacity < aligned_length {
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
        }

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn sync(&mut self) {
        todo!();
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

    pub fn sync(&mut self) {
        self.raw.sync()
    }
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Pod, Zeroable, Debug)]
pub struct Tick(u64);

impl Tick {
    pub const ZERO: Self = Tick(0);

    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

pub struct HistoryMapping<T: Pod + Zeroable> {
    raw: Blocks,
    phantom: PhantomData<T>,
}

impl<T: Pod + Zeroable> HistoryMapping<T> {
    pub fn new(path: &Path) -> Result<Self> {
        Blocks::new(path).map(|raw| Self {
            raw,
            phantom: PhantomData,
        })
    }

    pub fn append_zeroed(&mut self, tick: Tick, start: usize, length: usize) -> Result<()> {
        self.raw.extend_from_slice(&bytemuck::bytes_of(&tick))?;
        self.raw.extend_from_slice(&bytemuck::bytes_of(&start))?;
        self.raw.extend_from_slice(&bytemuck::bytes_of(&length))?;
        self.raw.extend_zeroed(size_of::<T>() * length)?;

        Ok(())
    }

    pub fn append(&mut self, tick: Tick, start: usize, values: &[T]) -> Result<()> {
        self.raw.extend_from_slice(&bytemuck::bytes_of(&tick))?;
        self.raw.extend_from_slice(&bytemuck::bytes_of(&start))?;
        self.raw
            .extend_from_slice(&bytemuck::bytes_of(&values.len()))?;
        self.raw.extend_from_slice(bytemuck::cast_slice(values))?;

        Ok(())
    }

    pub fn sync(&mut self) {
        self.raw.sync()
    }
}

pub struct Column<T: Pod + Zeroable> {
    data: Mapping<T>,
    history: HistoryMapping<T>,
    phantom: PhantomData<T>,
}

impl<T: Pod + Zeroable> Column<T> {
    pub fn new(data: &Path, history: &Path) -> Result<Self> {
        let data = Mapping::new(data)?;
        let history = HistoryMapping::new(history)?;

        Ok(Self {
            data,
            history,
            phantom: PhantomData,
        })
    }

    pub fn get(&mut self, range: Range<usize>) -> &[T] {
        self.data.get(range)
    }

    pub fn set(&mut self, tick: Tick, start: usize, values: &[T]) -> Result<()> {
        let end = start + values.len();

        //let current = self.data.get(start..end);
        //self.history.append(tick, start, &current)?;
        self.data.get_mut(start..end).copy_from_slice(values);

        Ok(())
    }

    pub fn append(&mut self, tick: Tick, values: &[T]) -> Result<Range<usize>> {
        let start = self.data.len();

        self.data.extend_from_slice(values)?;
        //self.history.append_zeroed(tick, start, values.len())?;

        Ok(start..start + values.len())
    }

    pub fn remove(&mut self, tick: Tick, range: Range<usize>) -> Result<()> {
        self.set(
            tick,
            range.start,
            &vec![bytemuck::zeroed(); range.end - range.start],
        )
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn sync(&mut self) {
        self.data.sync();
        self.history.sync();
    }
}
