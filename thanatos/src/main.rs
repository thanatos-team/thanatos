#![warn(clippy::pedantic)]
#![warn(clippy::perf)]

mod input;
mod mesh;
mod system;

use std::any::type_name;
use std::borrow::Cow;
use std::marker::PhantomData;
use std::ops::{Bound, RangeBounds};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Quat, Vec2, Vec3};
use gltf::Glb;
use input::Mouse;
use mesh::{Mesh, Vertex};
use system::{System, Systems};
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::error::EventLoopError;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

struct Context<'a> {
    instance: wgpu::Instance,
    surface: wgpu::Surface<'a>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

struct Renderer<'a> {
    ctx: Context<'a>,
    bind_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    view_buffer: Buffer<Mat4>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct VertexData {
    vertex: Vertex,
    mesh_index: u32,
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

struct Buffer<T: Pod> {
    inner: wgpu::Buffer,
    queue: wgpu::Queue,
    phantom: PhantomData<T>,
}

impl<T: Pod> Buffer<T> {
    pub fn update(&self, value: &T) {
        self.queue
            .write_buffer(&self.inner, 0, bytemuck::bytes_of(value));
    }
}

struct ArrayBuffer<T: Pod> {
    inner: wgpu::Buffer,
    queue: wgpu::Queue,
    phantom: PhantomData<T>,
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

struct ArrayBufferSlice<'a, R: RangeBounds<usize>, T: Pod> {
    buffer: &'a ArrayBuffer<T>,
    range: R,
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

struct BindGroupBuilder<'a> {
    device: &'a wgpu::Device,
    layout: &'a wgpu::BindGroupLayout,
    entries: Vec<wgpu::BindGroupEntry<'a>>,
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

    pub fn finish(self) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: self.layout,
            entries: &self.entries,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct Camera {
    centre: Vec3,
    distance: f32,
    angle: f32,
    pitch: f32,
}

static CAMERA: Mutex<Camera> = Mutex::new(Camera {
    centre: Vec3::ZERO,
    distance: 250.0,
    angle: 0.0,
    pitch: 0.0,
});

impl Camera {
    pub fn get() -> Camera {
        *CAMERA.lock().unwrap()
    }

    pub fn update<F: FnOnce(&mut Camera)>(f: F) {
        let Ok(mut camera) = CAMERA.lock() else {
            return;
        };
        f(&mut camera);

        camera.pitch = camera.pitch.clamp(0.0, std::f32::consts::FRAC_PI_2 - 0.01); 
    }

    pub fn matrix(&self) -> Mat4 {
        let rotation = Quat::from_euler(glam::EulerRot::YZX, 0.0, -self.angle, self.pitch);
        let eye = self.centre + rotation * Vec3::Y * self.distance;

        Mat4::look_at_rh(eye, self.centre, Vec3::Z)
    }
}

impl System for Camera {
    fn on_frame_end() {
        let mouse = Mouse::get();
        if !mouse.left_down || mouse.delta == Vec2::ZERO {
            return;
        }

        Self::update(|camera| {
            camera.angle += Mouse::get().delta.x * 0.005;
            camera.pitch += Mouse::get().delta.y * 0.005;
        });
    }
}

impl Renderer<'_> {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let ctx = Context::new(window.clone()).await?;

        let bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(64),
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(0),
                            },
                            count: None,
                        },
                    ],
                });

        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let swapchain_capabilities = ctx.surface.get_capabilities(&ctx.adapter);
        let swapchain_format = swapchain_capabilities.formats[0];

        let shader = ctx.create_shader_module(include_str!("../assets/shader.wgsl"), "shader");

        let render_pipeline = ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: size_of::<VertexData>() as wgpu::BufferAddress,
                        step_mode: wgpu::VertexStepMode::Vertex,
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

        let view_buffer = ctx.create_buffer(
            &Mat4::IDENTITY,
            "view matrix",
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        Ok(Self {
            ctx,
            bind_group_layout,
            render_pipeline,
            view_buffer,
        })
    }

    pub fn resize(&self, width: u32, height: u32) {
        self.ctx.resize(width, height);
    }

    pub fn draw(&self, window: &Window, scene: &[Mesh]) -> wgpu::SurfaceTexture {
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

        let meshes = scene.iter().map(|mesh| mesh.info).collect::<Vec<_>>();

        let vertex_buffer =
            self.ctx
                .create_array_buffer(&vertices, "vertices", wgpu::BufferUsages::VERTEX);
        let index_buffer =
            self.ctx
                .create_array_buffer(&indices, "indices", wgpu::BufferUsages::INDEX);
        let mesh_buffer =
            self.ctx
                .create_array_buffer(&meshes, "meshes", wgpu::BufferUsages::STORAGE);

        let size = window.inner_size();
        let projection = Mat4::perspective_infinite_rh(
            std::f32::consts::FRAC_PI_4,
            size.width.max(1) as f32 / size.height.max(1) as f32,
            0.1,
        );
        let view = Camera::get().matrix();
        self.view_buffer.update(&(projection * view));

        let bind_group = self
            .ctx
            .create_bind_group(&self.bind_group_layout)
            .with_buffer(&self.view_buffer)
            .with_array_buffer(&mesh_buffer)
            .finish();

        let frame = self
            .ctx
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture");

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let depth = self.ctx.create_depth_texture(window); // TODO: Texture system

        let mut encoder = self
            .ctx
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
            rpass.set_index_buffer(index_buffer.inner.slice(..), wgpu::IndexFormat::Uint32);
            rpass.set_vertex_buffer(0, vertex_buffer.inner.slice(..));
            rpass.set_bind_group(0, &bind_group, &[]);
            rpass.draw_indexed(0..indices.len() as u32, 0, 0..1);
        }

        self.ctx.queue.submit(Some(encoder.finish()));
        frame
    }
}

#[derive(Default)]
struct App<'a> {
    renderer: Option<Renderer<'a>>,
    window: Option<Arc<Window>>,
    scene: Vec<Mesh>,
    systems: Systems,
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );
        self.renderer = pollster::block_on(Renderer::new(window.clone())).ok();
        self.window = Some(window);
        let cube = Glb::load(include_bytes!("../assets/Box.glb")).unwrap();
        let duck = Glb::load(include_bytes!("../assets/Duck.glb")).unwrap();
        self.scene = [Mesh::from_glb(&cube), Mesh::from_glb(&duck)]
            .into_iter()
            .flatten()
            .collect();

        self.systems.register::<Camera>();
        self.systems.register::<Mouse>();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.systems.on_window_event(&event);

        match event {
            WindowEvent::Resized(new_size) => {
                self.renderer
                    .as_ref()
                    .unwrap()
                    .resize(new_size.width.max(1), new_size.height.max(1));
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::CloseRequested => {
                println!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let frame = self
                    .renderer
                    .as_ref()
                    .unwrap()
                    .draw(self.window.as_ref().unwrap(), &self.scene);
                self.window.as_mut().unwrap().pre_present_notify();
                frame.present();

                self.systems.on_frame_end();
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
