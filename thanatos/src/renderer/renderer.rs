use std::sync::Arc;

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use winit::window::Window;

use crate::{
    camera::Camera,
    mesh::{Mesh, Vertex, VertexData},
    scene::Scene,
};

use super::{context::Context, utils::Buffer};

pub struct Renderer<'a> {
    ctx: Context<'a>,
    bind_group_layout: wgpu::BindGroupLayout,
    render_pipeline: wgpu::RenderPipeline,
    view_buffer: Buffer<Mat4>,
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

        let shader = ctx.create_shader_module(include_str!("../../assets/shader.wgsl"), "shader");

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

    pub fn draw(&self, window: &Window, scene: Scene) -> wgpu::SurfaceTexture {
        let vertex_buffer =
            self.ctx
                .create_array_buffer(&scene.vertices, "vertices", wgpu::BufferUsages::VERTEX);
        let index_buffer =
            self.ctx
                .create_array_buffer(&scene.indices, "indices", wgpu::BufferUsages::INDEX);
        let mesh_buffer =
            self.ctx
                .create_array_buffer(&scene.infos, "meshes", wgpu::BufferUsages::STORAGE);

        let size = window.inner_size();
        let projection = Mat4::perspective_infinite_rh(
            std::f32::consts::FRAC_PI_4,
            size.width.max(1) as f32 / size.height.max(1) as f32,
            0.1,
        );
        let view = Camera::get_matrix();
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
            rpass.draw_indexed(0..scene.indices.len() as u32, 0, 0..1);
        }

        self.ctx.queue.submit(Some(encoder.finish()));
        frame
    }
}
