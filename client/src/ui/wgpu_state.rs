use std::sync::Arc;

use log::{debug, info};
use wgpu::util::DeviceExt;
use winit::{event::WindowEvent, window::Window};

use crate::double_buffer::DoubleBuffer;

pub struct WGPUState {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,

    window: Window,
    render_pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,

    texture_size: Option<wgpu::Extent3d>,
    diffuse_texture: Option<wgpu::Texture>,
    diffuse_bind_group: Option<wgpu::BindGroup>,

    num_vertices: u32,
    num_indices: u32,

    double_buffer: Arc<DoubleBuffer>,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

const VERTICES: &[Vertex] = &[
    Vertex {
        // A
        position: [-1.0, 1.0, 0.0],
        tex_coords: [0.0, 0.0],
    },
    Vertex {
        // B
        position: [-1.0, -1.0, 0.0],
        tex_coords: [0.0, 1.0],
    },
    Vertex {
        // C
        position: [1.0, -1.0, 0.0],
        tex_coords: [1.0, 1.0],
    },
    Vertex {
        // D
        position: [1.0, 1.0, 0.0],
        tex_coords: [1.0, 0.0],
    },
];

const INDICES: &[u16] = &[
    0, 1, 2, // ABC
    0, 2, 3, // ACD
];

impl WGPUState {
    pub async fn new(window: Window, double_buffer: Arc<DoubleBuffer>) -> Self {
        let size = window.inner_size();

        let num_vertices = VERTICES.len() as u32;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = unsafe { instance.create_surface(&window) }.unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .filter(|f| f.is_srgb())
            .next()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertex buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("index buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let num_indices = INDICES.len() as u32;

        let render_pipeline = None;

        let texture_size = None;
        let diffuse_bind_group = None;
        let diffuse_texture = None;

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            vertex_buffer,
            index_buffer,
            num_vertices,
            num_indices,
            double_buffer,
            texture_size,
            diffuse_texture,
            diffuse_bind_group,
        }
    }
    pub fn window(&self) -> &Window {
        &self.window
    }
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }
    pub fn input(&mut self, event: &WindowEvent) -> bool {
        false
    }
    pub fn update(&mut self) -> Result<(), wgpu::SurfaceError> {
        Ok(())
    }
    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // try to get a frame here.

        if let Some(rgba_buffer) = &*self.double_buffer.front() {
            debug!("rgba buffer is {:?}", &rgba_buffer.buffer[..20]);
            // Set up the bind group if it hasn't been created yet.

            if let None = self.diffuse_bind_group {
                let texture_size = wgpu::Extent3d {
                    width: rgba_buffer.width as u32,
                    height: rgba_buffer.height as u32,
                    depth_or_array_layers: 1,
                };

                self.texture_size = Some(texture_size);

                self.diffuse_texture = Some(self.device.create_texture(&wgpu::TextureDescriptor {
                    size: texture_size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    label: Some("diffuse_texture"),
                    view_formats: &[],
                }));

                let diffuse_texture_view = self
                    .diffuse_texture
                    .as_mut()
                    .unwrap()
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let diffuse_sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    address_mode_w: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Linear,
                    min_filter: wgpu::FilterMode::Nearest,
                    mipmap_filter: wgpu::FilterMode::Nearest,
                    ..Default::default()
                });

                let texture_bind_group_layout =
                    self.device
                        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                            entries: &[
                                wgpu::BindGroupLayoutEntry {
                                    binding: 0,
                                    visibility: wgpu::ShaderStages::FRAGMENT,
                                    ty: wgpu::BindingType::Texture {
                                        multisampled: false,
                                        view_dimension: wgpu::TextureViewDimension::D2,
                                        sample_type: wgpu::TextureSampleType::Float {
                                            filterable: true,
                                        },
                                    },
                                    count: None,
                                },
                                wgpu::BindGroupLayoutEntry {
                                    binding: 1,
                                    visibility: wgpu::ShaderStages::FRAGMENT,
                                    ty: wgpu::BindingType::Sampler(
                                        wgpu::SamplerBindingType::Filtering,
                                    ),
                                    count: None,
                                },
                            ],
                            label: Some("texture_bind_group_layout"),
                        });

                self.diffuse_bind_group =
                    Some(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: &texture_bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                            },
                        ],
                        label: Some("diffuse_bind_group"),
                    }));

                let shader = self
                    .device
                    .create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
                let render_pipeline_layout =
                    self.device
                        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                            label: Some("render pipeline layout"),
                            bind_group_layouts: &[&texture_bind_group_layout],
                            push_constant_ranges: &[],
                        });

                self.render_pipeline = Some(self.device.create_render_pipeline(
                    &wgpu::RenderPipelineDescriptor {
                        label: Some("Render pipeline"),
                        layout: Some(&render_pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: "vs_main",
                            buffers: &[Vertex::desc()],
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: "fs_main",
                            targets: &[Some(wgpu::ColorTargetState {
                                format: self.config.format,
                                blend: Some(wgpu::BlendState::REPLACE),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                        }),
                        primitive: wgpu::PrimitiveState {
                            topology: wgpu::PrimitiveTopology::TriangleList,
                            strip_index_format: None,
                            front_face: wgpu::FrontFace::Ccw,
                            cull_mode: Some(wgpu::Face::Back),
                            polygon_mode: wgpu::PolygonMode::Fill,
                            unclipped_depth: false,
                            conservative: false,
                        },
                        depth_stencil: None,
                        multisample: wgpu::MultisampleState {
                            count: 1,
                            mask: !0,
                            alpha_to_coverage_enabled: false,
                        },
                        multiview: None,
                    },
                ));
            }
            self.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: self.diffuse_texture.as_ref().unwrap(),
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &rgba_buffer.buffer,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * rgba_buffer.width as u32),
                    rows_per_image: Some(rgba_buffer.height as u32),
                },
                self.texture_size.unwrap(),
            );
        }

        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render encoder"),
            });
        {
            let mut _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            if let Some(diffuse_bind_group) = &self.diffuse_bind_group {
                let render_pipeline = self.render_pipeline.as_ref().unwrap();
                _render_pass.set_pipeline(render_pipeline);
                _render_pass.set_bind_group(0, &diffuse_bind_group, &[]);
                _render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                _render_pass
                    .set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                _render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
