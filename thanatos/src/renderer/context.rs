use std::{any::type_name, borrow::Cow, marker::PhantomData, sync::Arc};

use anyhow::Result;
use bytemuck::Pod;
use wgpu::util::DeviceExt;
use winit::window::Window;

use super::utils::{ArrayBuffer, BindGroupBuilder, Buffer};

pub struct Context<'a> {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'a>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl<'a> Context<'a> {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                // Request an adapter which can render to our surface
                compatible_surface: Some(&surface),
            })
            .await
            .expect("Failed to find an appropriate adapter");

        // Create the logical device and command queue
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    // Make sure we use the texture resolution limits from the adapter, so we can support images the size of the swapchain.
                    required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
            .expect("Failed to create device");

        let ctx = Self {
            instance,
            surface,
            adapter,
            device,
            queue,
        };

        let size = window.inner_size();
        ctx.resize(size.width, size.height);
        Ok(ctx)
    }

    fn get_size(window: &Window) -> (u32, u32) {
        let size = window.inner_size();
        (size.width.max(1), size.height.max(1))
    }

    pub fn resize(&self, width: u32, height: u32) {
        let config = self
            .surface
            .get_default_config(&self.adapter, width, height)
            .unwrap();
        self.surface.configure(&self.device, &config);
    }

    pub fn create_depth_texture(&self, window: &Window) -> wgpu::TextureView {
        let (width, height) = Self::get_size(window);
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    pub fn create_buffer<T: Pod>(
        &self,
        value: &T,
        label: &str,
        usage: wgpu::BufferUsages,
    ) -> Buffer<T> {
        let inner = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("Buffer<{}> ({label})", type_name::<T>())),
                contents: bytemuck::bytes_of(value),
                usage,
            });

        Buffer {
            inner,
            queue: self.queue.clone(),
            phantom: PhantomData,
        }
    }

    pub fn create_array_buffer<T: Pod>(
        &self,
        values: &[T],
        label: &str,
        usage: wgpu::BufferUsages,
    ) -> ArrayBuffer<T> {
        let inner = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("ArrayBuffer<{}> ({label})", type_name::<T>())),
                contents: bytemuck::cast_slice(values),
                usage,
            });

        ArrayBuffer {
            inner,
            queue: self.queue.clone(),
            phantom: PhantomData,
        }
    }

    pub fn create_bind_group<'b>(
        &'b self,
        layout: &'b wgpu::BindGroupLayout,
    ) -> BindGroupBuilder<'b> {
        BindGroupBuilder {
            device: &self.device,
            layout,
            entries: Vec::new(),
        }
    }

    pub fn create_shader_module(&self, shader: &str, label: &str) -> wgpu::ShaderModule {
        self.device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader)),
            })
    }
}
