use std::{
    marker::PhantomData,
    ops::{Bound, RangeBounds},
};

use bytemuck::Pod;

pub struct Buffer<T: Pod> {
    pub inner: wgpu::Buffer,
    pub queue: wgpu::Queue,
    pub phantom: PhantomData<T>,
}

impl<T: Pod> Buffer<T> {
    pub fn update(&self, value: &T) {
        self.queue
            .write_buffer(&self.inner, 0, bytemuck::bytes_of(value));
    }
}

pub struct ArrayBuffer<T: Pod> {
    pub inner: wgpu::Buffer,
    pub queue: wgpu::Queue,
    pub phantom: PhantomData<T>,
}

impl<T: Pod> ArrayBuffer<T> {
    pub fn update(&self, offset: usize, values: &[T]) {
        self.queue.write_buffer(
            &self.inner,
            (offset * size_of::<T>()) as wgpu::BufferAddress,
            bytemuck::cast_slice(values),
        );
    }

    pub fn update_all(&self, values: &[T]) {
        self.update(0, values);
    }

    pub fn slice<'a, R: RangeBounds<usize>>(&'a self, range: R) -> ArrayBufferSlice<'a, R, T> {
        ArrayBufferSlice {
            buffer: self,
            range,
        }
    }
}

pub struct ArrayBufferSlice<'a, R: RangeBounds<usize>, T: Pod> {
    pub buffer: &'a ArrayBuffer<T>,
    pub range: R,
}

impl<R: RangeBounds<usize>, T: Pod> ArrayBufferSlice<'_, R, T> {
    pub fn update(&self, offset: usize, values: &[T]) {
        self.buffer.update(
            offset
                + match self.range.start_bound() {
                    Bound::Unbounded => 0,
                    Bound::Included(start) => *start,
                    Bound::Excluded(start) => start + 1,
                },
            values,
        );
    }

    pub fn update_all(&self, values: &[T]) {
        self.update(0, values);
    }
}

pub struct BindGroupBuilder<'a> {
    pub device: &'a wgpu::Device,
    pub layout: &'a wgpu::BindGroupLayout,
    pub entries: Vec<wgpu::BindGroupEntry<'a>>,
}

impl<'a> BindGroupBuilder<'a> {
    pub fn with_buffer<T: Pod>(mut self, buffer: &'a Buffer<T>) -> Self {
        self.entries.push(wgpu::BindGroupEntry {
            binding: self.entries.len() as u32,
            resource: buffer.inner.as_entire_binding(),
        });
        self
    }

    pub fn with_array_buffer<T: Pod>(mut self, buffer: &'a ArrayBuffer<T>) -> Self {
        self.entries.push(wgpu::BindGroupEntry {
            binding: self.entries.len() as u32,
            resource: buffer.inner.as_entire_binding(),
        });
        self
    }

    pub fn with_texture_view(mut self, view: &'a wgpu::TextureView) -> Self {
        self.entries.push(wgpu::BindGroupEntry {
            binding: self.entries.len() as u32,
            resource: wgpu::BindingResource::TextureView(&view),
        });
        self
    }

    pub fn with_sampler(mut self, sampler: &'a wgpu::Sampler) -> Self {
        self.entries.push(wgpu::BindGroupEntry {
            binding: self.entries.len() as u32,
            resource: wgpu::BindingResource::Sampler(sampler),
        });
        self
    }

    pub fn finish(self) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: self.layout,
            entries: &self.entries,
        })
    }
}
