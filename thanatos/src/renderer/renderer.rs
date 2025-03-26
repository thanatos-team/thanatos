use std::sync::Arc;

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, UVec2};
use wgpu::util::TextureBlitterBuilder;
use winit::window::Window;

use crate::{
    camera::Camera,
    mesh::{Mesh, Vertex, VertexData},
    scene::Scene,
};

use super::{context::Context, utils::Buffer};

pub struct Renderer<'a> {
    ctx: Context<'a>,
    gpass_bind_group_layout: wgpu::BindGroupLayout,
    gpass_pipeline: wgpu::RenderPipeline,
    light_bind_group_layout: wgpu::BindGroupLayout,
    light_pipeline: wgpu::RenderPipeline,
    light_sampler: wgpu::Sampler,
    view_buffer: Buffer<Mat4>,
}

impl Renderer<'_> {
    const NORMAL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;

    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let ctx = Context::new(window.clone()).await?;

        let (gpass_bind_group_layout, gpass_pipeline) = {
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

            let pipeline_layout =
                ctx.device
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: None,
                        bind_group_layouts: &[&bind_group_layout],
                        push_constant_ranges: &[],
                    });

            let shader = ctx.create_shader_module(include_str!("../../assets/gpass.wgsl"), "gpass");

            let pipeline = ctx
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
                        targets: &[
                            Some(ctx.get_swapchain_format().into()),
                            Some(Self::NORMAL_FORMAT.into()),
                        ],
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

            (bind_group_layout, pipeline)
        };

        let (light_bind_group_layout, light_pipeline) = {
            let bind_group_layout =
                ctx.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: None,
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Sampler(
                                    wgpu::SamplerBindingType::NonFiltering,
                                ),
                                count: None,
                            },
                            // Colour
                            wgpu::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: false,
                                    },
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                    multisampled: false,
                                },
                                count: None,
                            },
                            // Normal
                            wgpu::BindGroupLayoutEntry {
                                binding: 2,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    sample_type: wgpu::TextureSampleType::Float {
                                        filterable: false,
                                    },
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                    multisampled: false,
                                },
                                count: None,
                            },
                            // Depth
                            wgpu::BindGroupLayoutEntry {
                                binding: 3,
                                visibility: wgpu::ShaderStages::FRAGMENT,
                                ty: wgpu::BindingType::Texture {
                                    sample_type: wgpu::TextureSampleType::Depth,
                                    view_dimension: wgpu::TextureViewDimension::D2,
                                    multisampled: false,
                                },
                                count: None,
                            },
                        ],
                    });

            let pipeline_layout =
                ctx.device
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: None,
                        bind_group_layouts: &[&bind_group_layout],
                        push_constant_ranges: &[],
                    });

            let shader = ctx.create_shader_module(include_str!("../../assets/light.wgsl"), "light");

            let pipeline = ctx
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: None,
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        targets: &[Some(ctx.get_swapchain_format().into())],
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                });

            (bind_group_layout, pipeline)
        };

        let light_sampler = ctx
            .device
            .create_sampler(&wgpu::SamplerDescriptor::default());

        let view_buffer = ctx.create_buffer(
            &Mat4::IDENTITY,
            "view matrix",
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );

        Ok(Self {
            ctx,
            gpass_bind_group_layout,
            gpass_pipeline,
            light_bind_group_layout,
            light_pipeline,
            light_sampler,
            view_buffer,
        })
    }

    pub fn resize(&self, size: UVec2) {
        self.ctx.resize(size);
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

        let gpass_bind_group = self
            .ctx
            .create_bind_group(&self.gpass_bind_group_layout)
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

        let window_size = window.inner_size();
        let aspect = window_size.width as f32 / window_size.height as f32;

        let render_height = 240;
        let size = UVec2::new((render_height as f32 * aspect) as u32, render_height);

        let diffuse = self.ctx.create_colour_texture(
            size,
            self.ctx.get_swapchain_format(),
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        
        let normal = self.ctx.create_colour_texture(
            size,
            Self::NORMAL_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        
        let depth = self.ctx.create_depth_texture(
            size,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );

        let colour = self.ctx.create_colour_texture(
            size,
            self.ctx.get_swapchain_format(),
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );

        let light_bind_group = self
            .ctx
            .create_bind_group(&self.light_bind_group_layout)
            .with_sampler(&self.light_sampler)
            .with_texture_view(&diffuse)
            .with_texture_view(&normal)
            .with_texture_view(&depth)
            .finish();

        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut gpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &diffuse,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &normal,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
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
            gpass.set_pipeline(&self.gpass_pipeline);
            gpass.set_index_buffer(index_buffer.inner.slice(..), wgpu::IndexFormat::Uint32);
            gpass.set_vertex_buffer(0, vertex_buffer.inner.slice(..));
            gpass.set_bind_group(0, &gpass_bind_group, &[]);
            gpass.draw_indexed(0..scene.indices.len() as u32, 0, 0..1);
        }

        {
            let mut lpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &colour,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            lpass.set_pipeline(&self.light_pipeline);
            lpass.set_bind_group(0, &light_bind_group, &[]);
            lpass.draw(0..3, 0..1);
        }

        TextureBlitterBuilder::new(&self.ctx.device, self.ctx.get_swapchain_format())
            .sample_type(wgpu::FilterMode::Nearest)
            .blend_state(wgpu::BlendState::REPLACE)
            .build()
            .copy(&self.ctx.device, &mut encoder, &colour, &view);

        self.ctx.queue.submit(Some(encoder.finish()));
        frame
    }
}
