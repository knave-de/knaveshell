use bytemuck::{Pod, Zeroable};
use knave_ui::UiImage;
use wgpu::util::DeviceExt;

use super::{RenderCommand, RenderList};

const SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> viewport: Viewport;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};


@vertex
fn vertex_main(input: VertexInput) -> VertexOutput {
    let normalized = vec2(
        input.position.x / viewport.size.x * 2.0 - 1.0,
        1.0 - input.position.y / viewport.size.y * 2.0,
    );
    return VertexOutput(vec4(normalized, 0.0, 1.0), input.color);
}


@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

const IMAGE_SHADER: &str = r#"
struct Viewport {
    size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> viewport: Viewport;
@group(0) @binding(1)
var image_texture: texture_2d<f32>;
@group(0) @binding(2)
var image_sampler: sampler;

struct ImageVertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct ImageVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};


@vertex
fn image_vertex_main(input: ImageVertexInput) -> ImageVertexOutput {
    let normalized = vec2(
        input.position.x / viewport.size.x * 2.0 - 1.0,
        1.0 - input.position.y / viewport.size.y * 2.0,
    );
    return ImageVertexOutput(vec4(normalized, 0.0, 1.0), input.uv);
}


@fragment
fn image_fragment_main(input: ImageVertexOutput) -> @location(0) vec4<f32> {
    return textureSample(image_texture, image_sampler, input.uv);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x4,
                    offset: 8,
                    shader_location: 1,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ImageVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl ImageVertex {
    const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                },
                wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 1,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Viewport {
    size: [f32; 2],
}

struct CachedImage {
    source: UiImage,
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

pub struct WgpuPainter {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    image_pipeline: wgpu::RenderPipeline,
    image_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    viewport: wgpu::Buffer,
    vertices: Option<wgpu::Buffer>,
    vertex_capacity: usize,
    image_vertices: Option<wgpu::Buffer>,
    image_vertex_capacity: usize,
    image_cache: Vec<CachedImage>,
}

impl WgpuPainter {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("knave-shell-painter"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("knave-shell-painter-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("knave-shell-painter-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("knave-shell-painter-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                buffers: &[Some(Vertex::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let image_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("knave-shell-image-shader"),
            source: wgpu::ShaderSource::Wgsl(IMAGE_SHADER.into()),
        });
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("knave-shell-image-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let image_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("knave-shell-image-pipeline-layout"),
                bind_group_layouts: &[Some(&image_layout)],
                immediate_size: 0,
            });
        let image_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("knave-shell-image-pipeline"),
            layout: Some(&image_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &image_shader,
                entry_point: Some("image_vertex_main"),
                buffers: &[Some(ImageVertex::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &image_shader,
                entry_point: Some("image_fragment_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("knave-shell-image-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let viewport = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("knave-shell-painter-viewport"),
            contents: bytemuck::bytes_of(&Viewport { size: [1.0, 1.0] }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("knave-shell-painter-bind-group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport.as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            bind_group,
            image_pipeline,
            image_layout,
            sampler,
            viewport,
            vertices: None,
            vertex_capacity: 0,
            image_vertices: None,
            image_vertex_capacity: 0,
            image_cache: Vec::new(),
        }
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        viewport: (u32, u32),
        render_list: &RenderList,
    ) {
        let (vertices, image_draws) = vertices_for(render_list);
        if vertices.is_empty() && image_draws.is_empty() {
            return;
        }

        queue.write_buffer(
            &self.viewport,
            0,
            bytemuck::bytes_of(&Viewport {
                size: [viewport.0.max(1) as f32, viewport.1.max(1) as f32],
            }),
        );
        if !vertices.is_empty() {
            let byte_len = vertices.len() * std::mem::size_of::<Vertex>();
            if byte_len > self.vertex_capacity {
                self.vertex_capacity = byte_len.next_power_of_two();
                self.vertices = Some(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("knave-shell-painter-vertices"),
                    size: self.vertex_capacity as wgpu::BufferAddress,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }));
            }
            let vertex_buffer = self.vertices.as_ref().expect("vertex buffer was allocated");
            queue.write_buffer(vertex_buffer, 0, bytemuck::cast_slice(&vertices));

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("knave-shell-painter-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..byte_len as wgpu::BufferAddress));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.encode_images(device, queue, encoder, view, &image_draws);
    }

    fn ensure_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &UiImage,
    ) -> usize {
        if let Some(index) = self.image_cache.iter().position(|cached| {
            cached.source.cache_key() == image.cache_key()
                && cached.source.width() == image.width()
                && cached.source.height() == image.height()
        }) {
            return index;
        }

        if self.image_cache.len() >= 16 {
            self.image_cache.remove(0);
        }
        let size = wgpu::Extent3d {
            width: image.width(),
            height: image.height(),
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("knave-shell-preview-texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            image.pixels(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width() * 4),
                rows_per_image: Some(image.height()),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("knave-shell-preview-bind-group"),
            layout: &self.image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.viewport.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.image_cache.push(CachedImage {
            source: image.clone(),
            _texture: texture,
            bind_group,
        });
        self.image_cache.len() - 1
    }

    fn encode_images(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        images: &[(knave_ui::Rect, &UiImage)],
    ) {
        if images.is_empty() {
            return;
        }
        let cache_indices = images
            .iter()
            .map(|(_, image)| self.ensure_image(device, queue, image))
            .collect::<Vec<_>>();
        let mut vertices = Vec::with_capacity(images.len() * 6);
        for (bounds, _) in images {
            push_image_quad(&mut vertices, bounds);
        }
        if vertices.is_empty() {
            return;
        }
        let byte_len = vertices.len() * std::mem::size_of::<ImageVertex>();
        if byte_len > self.image_vertex_capacity {
            self.image_vertex_capacity = byte_len.next_power_of_two();
            self.image_vertices = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("knave-shell-image-vertices"),
                size: self.image_vertex_capacity as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let vertex_buffer = self
            .image_vertices
            .as_ref()
            .expect("image vertex buffer was allocated");
        queue.write_buffer(vertex_buffer, 0, bytemuck::cast_slice(&vertices));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("knave-shell-image-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.image_pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..byte_len as wgpu::BufferAddress));
        let vertex_size = std::mem::size_of::<ImageVertex>() as wgpu::BufferAddress;
        for (index, cache_index) in cache_indices.into_iter().enumerate() {
            pass.set_bind_group(0, &self.image_cache[cache_index].bind_group, &[]);
            let offset = index as wgpu::BufferAddress * 6 * vertex_size;
            pass.set_vertex_buffer(0, vertex_buffer.slice(offset..offset + 6 * vertex_size));
            pass.draw(0..6, 0..1);
        }
    }
}

fn vertices_for(render_list: &RenderList) -> (Vec<Vertex>, Vec<(knave_ui::Rect, &UiImage)>) {
    let mut vertices = Vec::new();
    let mut image_draws = Vec::new();
    for command in &render_list.commands {
        match command {
            RenderCommand::FillRect { bounds, color } => {
                push_quad(
                    &mut vertices,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    *color,
                );
            }
            RenderCommand::Text {
                bounds,
                color,
                text,
            } => push_text(&mut vertices, bounds, *color, text),
            RenderCommand::Image { bounds, image } if bounds.width > 0.0 && bounds.height > 0.0 => {
                image_draws.push((*bounds, image));
            }
            RenderCommand::Image { .. } => {}
        }
    }
    (vertices, image_draws)
}

fn push_image_quad(vertices: &mut Vec<ImageVertex>, bounds: &knave_ui::Rect) {
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return;
    }
    let x2 = bounds.x + bounds.width;
    let y2 = bounds.y + bounds.height;
    vertices.extend_from_slice(&[
        ImageVertex {
            position: [bounds.x, bounds.y],
            uv: [0.0, 0.0],
        },
        ImageVertex {
            position: [x2, bounds.y],
            uv: [1.0, 0.0],
        },
        ImageVertex {
            position: [x2, y2],
            uv: [1.0, 1.0],
        },
        ImageVertex {
            position: [bounds.x, bounds.y],
            uv: [0.0, 0.0],
        },
        ImageVertex {
            position: [x2, y2],
            uv: [1.0, 1.0],
        },
        ImageVertex {
            position: [bounds.x, y2],
            uv: [0.0, 1.0],
        },
    ]);
}

fn push_quad(
    vertices: &mut Vec<Vertex>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    color: knave_ui::Color,
) {
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let color = [
        f32::from(color.red) / 255.0,
        f32::from(color.green) / 255.0,
        f32::from(color.blue) / 255.0,
        f32::from(color.alpha) / 255.0,
    ];
    let x2 = x + width;
    let y2 = y + height;
    vertices.extend_from_slice(&[
        Vertex {
            position: [x, y],
            color,
        },
        Vertex {
            position: [x2, y],
            color,
        },
        Vertex {
            position: [x2, y2],
            color,
        },
        Vertex {
            position: [x, y],
            color,
        },
        Vertex {
            position: [x2, y2],
            color,
        },
        Vertex {
            position: [x, y2],
            color,
        },
    ]);
}

fn push_text(
    vertices: &mut Vec<Vertex>,
    bounds: &knave_ui::Rect,
    color: knave_ui::Color,
    text: &str,
) {
    let scale = (bounds.height / 8.0).floor().clamp(1.0, 4.0);
    let advance = 6.0 * scale;
    let max_chars = (bounds.width / advance).floor().max(0.0) as usize;
    let chars = text.chars().take(max_chars);
    let glyph_height = 7.0 * scale;
    let y = bounds.y + (bounds.height - glyph_height).max(0.0) / 2.0;
    let mut x = bounds.x;
    for character in chars {
        let glyph = bitmap(character);
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    push_quad(
                        vertices,
                        x + column as f32 * scale,
                        y + row as f32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        x += advance;
    }
}

fn bitmap(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0f, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0f],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0f, 0x10, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1f],
        'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0e],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x0a, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1b, 0x11],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x06, 0x08, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x02, 0x0c],
        '.' | '·' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
        '-' => [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
        _ => [0x0e, 0x11, 0x02, 0x04, 0x04, 0x00, 0x04],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use knave_ui::{Color, Rect};

    #[test]
    fn text_becomes_bounded_gpu_quads() {
        let list = RenderList {
            revision: 1,
            clear_color: knave_ui::Color::BACKGROUND,
            commands: vec![RenderCommand::Text {
                bounds: Rect::new(0.0, 0.0, 100.0, 16.0),
                color: Color::TEXT,
                text: "Workspace 2".into(),
            }],
        };
        let (vertices, images) = vertices_for(&list);
        assert!(images.is_empty());
        assert!(!vertices.is_empty());
        assert!(vertices.len() <= 11 * 35 * 6);
    }
}
