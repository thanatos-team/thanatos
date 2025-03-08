#![warn(clippy::pedantic)]
#![warn(clippy::perf)]

mod mesh;

use std::borrow::Cow;
use std::sync::Arc;

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use gltf::Glb;
use mesh::{Mesh, MeshInfo, Vertex};
use wgpu::util::{BufferInitDescriptor, DeviceExt};
use wgpu::{
    Adapter, BindGroup, BindGroupLayoutEntry, BindingType, Buffer, BufferAddress, BufferDescriptor,
    BufferSize, BufferUsages, Device, IndexFormat, Instance, InstanceDescriptor, Queue,
    RenderPipeline, ShaderStages, Surface, SurfaceTexture, VertexAttribute, VertexBufferLayout,
    VertexFormat, VertexState, VertexStepMode,
};
use winit::application::ApplicationHandler;
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct Context<'a> {
    instance: Instance,
    surface: Surface<'a>,
    adapter: Adapter,
    device: Device,
    queue: Queue,
    render_pipeline: RenderPipeline,
    view_buffer: Arc<Buffer>,
    bind_group_layout: wgpu::BindGroupLayout,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct VertexData {
    vertex: Vertex,
    mesh_index: u32,
}

impl<'a> Context<'a> {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let instance = Instance::new(&InstanceDescriptor::from_env_or_default());
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

        // Load the shaders from disk
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("../assets/shader.wgsl"))),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: BufferSize::new(64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: BufferSize::new(0),
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let swapchain_capabilities = surface.get_capabilities(&adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[VertexBufferLayout {
                    array_stride: size_of::<VertexData>() as BufferAddress,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: size_of::<[f32; 3]>() as wgpu::BufferAddress,
                            shader_location: 1,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Uint32,
                            offset: size_of::<[f32; 6]>() as wgpu::BufferAddress,
                            shader_location: 2,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(swapchain_format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let view_buffer = Arc::new(device.create_buffer(&BufferDescriptor {
            label: Some("view matrix buffer"),
            size: (size_of::<f32>() * 4 * 4) as BufferAddress,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));

        let ctx = Self {
            instance,
            surface,
            adapter,
            device,
            queue,
            render_pipeline,
            view_buffer,
            bind_group_layout,
        };
        ctx.resize(&window);
        Ok(ctx)
    }

    fn get_size(window: &Window) -> (u32, u32) {
        let size = window.inner_size();
        (size.width.max(1), size.height.max(1))
    }

    pub fn resize(&self, window: &Window) {
        let (width, height) = Self::get_size(window);

        let config = self
            .surface
            .get_default_config(&self.adapter, width, height)
            .unwrap();
        self.surface.configure(&self.device, &config);

        let projection = Mat4::perspective_infinite_rh(
            std::f32::consts::FRAC_PI_4,
            width as f32 / height as f32,
            1.0,
        );
        let view = Mat4::look_at_rh(100.0 * Vec3::new(1.5, -5.0, 3.0), Vec3::ZERO, Vec3::Z);
        let matrix = projection * view;

        self.queue
            .write_buffer(&self.view_buffer, 0, bytemuck::bytes_of(&matrix));
    }

    fn create_depth_texture(&self, window: &Window) -> wgpu::TextureView {
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

    pub fn draw(&self, window: &Window, scene: &[Mesh]) -> SurfaceTexture {
        let (vertices, indices) = scene.iter().enumerate().fold(
            (Vec::new(), Vec::new()),
            |(mut vertices, mut indices), (mesh_index, mesh)| {
                indices.extend_from_slice(
                    &mesh
                        .indices
                        .iter()
                        .map(|index| index + vertices.len() as u32)
                        .collect::<Vec<_>>(),
                );
                vertices.extend_from_slice(
                    &mesh
                        .vertices
                        .clone()
                        .into_iter()
                        .map(|vertex| VertexData {
                            vertex,
                            mesh_index: mesh_index as u32,
                        })
                        .collect::<Vec<_>>(),
                );
                (vertices, indices)
            },
        );

        let vertex_buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: BufferUsages::VERTEX,
        });
        let index_buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("index buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: BufferUsages::INDEX,
        });

        let meshes = scene.iter().map(|mesh| mesh.info).collect::<Vec<_>>();
        let mesh_buffer = self.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("mesh buffer"),
            contents: bytemuck::cast_slice(&meshes),
            usage: BufferUsages::STORAGE,
        });

        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame bind group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.view_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: mesh_buffer.as_entire_binding(),
                },
            ],
        });

        let frame = self
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let depth = self.create_depth_texture(window);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.set_pipeline(&self.render_pipeline);
            rpass.set_index_buffer(index_buffer.slice(..), IndexFormat::Uint32);
            rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        self.queue.submit(Some(encoder.finish()));
        frame
    }
}

#[derive(Default)]
struct App<'a> {
    ctx: Option<Context<'a>>,
    window: Option<Arc<Window>>,
    scene: Vec<Mesh>,
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );
        self.ctx = pollster::block_on(Context::new(window.clone())).ok();
        self.window = Some(window);
        let cube = Glb::load(include_bytes!("../assets/Box.glb")).unwrap();
        let duck = Glb::load(include_bytes!("../assets/Duck.glb")).unwrap();
        self.scene = [Mesh::from_glb(&cube), Mesh::from_glb(&duck)]
            .into_iter()
            .flatten()
            .collect();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Resized(_) => {
                self.ctx
                    .as_ref()
                    .unwrap()
                    .resize(self.window.as_ref().unwrap());
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.scene[1].info.transform *= Mat4::from_translation(Vec3::Z);

                let frame = self
                    .ctx
                    .as_ref()
                    .unwrap()
                    .draw(self.window.as_ref().unwrap(), &self.scene);
                self.window.as_mut().unwrap().pre_present_notify();
                frame.present();
                self.window.as_ref().unwrap().request_redraw();
            }
            _ => (),
        }
    }
}

fn main() -> Result<(), EventLoopError> {
    let event_loop = EventLoop::new().unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app)
}
