use std::{collections::HashMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use knave_ui::{DisplayCommand, ImageFit, ImageStyle, Rect, Transform2D, UiImage};
use wgpu::util::DeviceExt;

const MAX_IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_IMAGE_CACHE_ENTRIES: usize = 128;
const MAX_IMAGE_FRAME_BYTES: usize = 64 * 1024 * 1024;

const SHADER: &str = r#"
struct Viewport { size: vec2<f32> };
struct ImageInstance {
    bounds: vec4<f32>,
    matrix: vec4<f32>,
    translation: vec4<f32>,
    uv: vec4<f32>,
    parameters: vec4<f32>,
};

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(0) @binding(1) var<storage, read> images: array<ImageInstance>;
@group(0) @binding(2) var image_texture: texture_2d<f32>;
@group(0) @binding(3) var image_sampler: sampler;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) instance_index: u32,
};

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32,
               @builtin(instance_index) instance_index: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
    let image = images[instance_index];
    let unit = corners[vertex_index];
    let local = unit * image.bounds.zw;
    let point = image.bounds.xy + local;
    let world = vec2(
        image.matrix.x * point.x + image.matrix.z * point.y + image.translation.x,
        image.matrix.y * point.x + image.matrix.w * point.y + image.translation.y);
    let normalized = vec2(
        world.x / viewport.size.x * 2.0 - 1.0,
        1.0 - world.y / viewport.size.y * 2.0);
    var output: VertexOutput;
    output.position = vec4(normalized, 0.0, 1.0);
    output.local = local;
    output.uv = image.uv.xy + unit * image.uv.zw;
    output.instance_index = instance_index;
    return output;
}

fn rounded_box_distance(point: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    let half_size = size * 0.5;
    let clamped_radius = clamp(radius, 0.0, min(size.x, size.y) * 0.5);
    let q = abs(point - half_size) - half_size + vec2(clamped_radius);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - clamped_radius;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let image = images[input.instance_index];
    let distance = rounded_box_distance(input.local, image.bounds.zw, image.parameters.y);
    let coverage = 1.0 - smoothstep(-max(fwidth(distance), 0.75), max(fwidth(distance), 0.75), distance);
    var color = textureSample(image_texture, image_sampler, input.uv);
    color.a *= coverage * clamp(image.parameters.x, 0.0, 1.0);
    return color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Viewport {
    size: [f32; 2],
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ImageInstance {
    bounds: [f32; 4],
    matrix: [f32; 4],
    translation: [f32; 4],
    uv: [f32; 4],
    meta: [f32; 4],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ImageKey {
    identity: usize,
    width: u32,
    height: u32,
}

impl ImageKey {
    fn new(image: &UiImage) -> Self {
        Self {
            identity: image.cache_key(),
            width: image.width(),
            height: image.height(),
        }
    }
}

#[derive(Default)]
struct FrameImageBudget {
    charged: std::collections::HashSet<ImageKey>,
    bytes: usize,
}

impl FrameImageBudget {
    fn can_charge(&self, key: ImageKey, bytes: usize) -> bool {
        bytes <= MAX_IMAGE_FRAME_BYTES
            && (self.charged.contains(&key)
                || self.bytes.saturating_add(bytes) <= MAX_IMAGE_FRAME_BYTES)
    }

    fn charge(&mut self, key: ImageKey, bytes: usize) {
        if self.charged.insert(key) {
            self.bytes += bytes;
        }
    }
}

struct CachedImage {
    _source: UiImage,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bytes: usize,
}

struct CacheEntry {
    image: Arc<CachedImage>,
    last_used: u64,
}

#[derive(Default)]
struct ImageCache {
    entries: HashMap<ImageKey, CacheEntry>,
    bytes: usize,
    clock: u64,
}

#[derive(Clone, Copy)]
pub(super) struct ImageDraw {
    pub instance_index: usize,
    pub clip: Option<Rect>,
}

pub(super) struct ImagePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    viewport: wgpu::Buffer,
    sampler: wgpu::Sampler,
    cache: ImageCache,
    instances: Option<wgpu::Buffer>,
    bind_groups: Vec<wgpu::BindGroup>,
    resources: Vec<Arc<CachedImage>>,
    pub draws: Vec<Option<ImageDraw>>,
}

impl ImagePass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("knave-images-layout"),
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
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("knave-images-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("knave-images-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("knave-images-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                buffers: &[],
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
        let viewport = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("knave-images-viewport"),
            size: std::mem::size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("knave-images-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            viewport,
            sampler,
            cache: ImageCache::default(),
            instances: None,
            bind_groups: Vec::new(),
            resources: Vec::new(),
            draws: Vec::new(),
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: (u32, u32),
        commands: &[DisplayCommand],
    ) {
        queue.write_buffer(
            &self.viewport,
            0,
            bytemuck::bytes_of(&Viewport {
                size: [size.0.max(1) as f32, size.1.max(1) as f32],
                _pad: [0.0; 2],
            }),
        );
        self.draws.clear();
        self.draws.resize(commands.len(), None);
        self.bind_groups.clear();
        self.resources.clear();
        let mut instances = Vec::new();
        let mut frame_budget = FrameImageBudget::default();
        let mut frame_resources = HashMap::<ImageKey, Arc<CachedImage>>::new();
        for (command_index, command) in commands.iter().enumerate() {
            let DisplayCommand::Image {
                bounds,
                image,
                style,
                transform,
                clip,
            } = command
            else {
                continue;
            };
            let key = ImageKey::new(image);
            if !bounds.is_finite_positive()
                || !transform_is_finite(*transform)
                || !transform
                    .transform_rect_bounds(*bounds)
                    .is_finite_positive()
            {
                continue;
            }
            let Some(image_bytes) = (image.width() as usize)
                .checked_mul(image.height() as usize)
                .and_then(|pixels| pixels.checked_mul(4))
            else {
                continue;
            };
            if !frame_budget.can_charge(key, image_bytes) {
                continue;
            }
            let resource = if let Some(resource) = frame_resources.get(&key) {
                Arc::clone(resource)
            } else {
                let Some(resource) = self.cache.get_or_upload(device, queue, image) else {
                    continue;
                };
                frame_budget.charge(key, image_bytes);
                frame_resources.insert(key, Arc::clone(&resource));
                resource
            };
            let (bounds, uv) = fitted_geometry(*bounds, image, *style);
            if !bounds.is_finite_positive()
                || !transform.transform_rect_bounds(bounds).is_finite_positive()
                || !uv.iter().all(|value| value.is_finite())
            {
                continue;
            }
            let instance_index = instances.len();
            instances.push(ImageInstance {
                bounds: [bounds.x, bounds.y, bounds.width, bounds.height],
                matrix: [transform.a, transform.b, transform.c, transform.d],
                translation: [transform.tx, transform.ty, 0.0, 0.0],
                uv,
                meta: [
                    sane(style.opacity).clamp(0.0, 1.0),
                    sane(style.corner_radius).max(0.0),
                    0.0,
                    0.0,
                ],
            });
            self.draws[command_index] = Some(ImageDraw {
                instance_index,
                clip: *clip,
            });
            self.resources.push(resource);
        }

        self.instances = None;
        if instances.is_empty() {
            return;
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("knave-images-instances"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::STORAGE,
        });
        for resource in &self.resources {
            self.bind_groups
                .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("knave-images-bind-group"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.viewport.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&resource.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                }));
        }
        self.instances = Some(buffer);
    }

    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        instance_index: usize,
        scissor: [u32; 4],
    ) {
        pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_groups[instance_index], &[]);
        let index = instance_index as u32;
        pass.draw(0..6, index..index + 1);
    }
}

impl ImageCache {
    fn get_or_upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &UiImage,
    ) -> Option<Arc<CachedImage>> {
        self.clock = self.clock.wrapping_add(1);
        let key = ImageKey::new(image);
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = self.clock;
            return Some(Arc::clone(&entry.image));
        }
        let bytes = (image.width() as usize)
            .checked_mul(image.height() as usize)?
            .checked_mul(4)?;
        if image.width() > device.limits().max_texture_dimension_2d
            || image.height() > device.limits().max_texture_dimension_2d
        {
            return None;
        }
        let cached = Arc::new(upload(device, queue, image, bytes));
        if bytes > MAX_IMAGE_CACHE_BYTES {
            return Some(cached);
        }
        while self.entries.len() >= MAX_IMAGE_CACHE_ENTRIES
            || self.bytes.saturating_add(bytes) > MAX_IMAGE_CACHE_BYTES
        {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(entry.image.bytes);
            }
        }
        self.bytes += bytes;
        self.entries.insert(
            key,
            CacheEntry {
                image: Arc::clone(&cached),
                last_used: self.clock,
            },
        );
        Some(cached)
    }
}

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &UiImage,
    bytes: usize,
) -> CachedImage {
    let size = wgpu::Extent3d {
        width: image.width(),
        height: image.height(),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("knave-shell-image"),
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
    CachedImage {
        _source: image.clone(),
        _texture: texture,
        view,
        bytes,
    }
}

fn fitted_geometry(bounds: Rect, image: &UiImage, style: ImageStyle) -> (Rect, [f32; 4]) {
    let source_ratio = image.width() as f32 / image.height() as f32;
    let target_ratio = bounds.width / bounds.height;
    match style.fit {
        ImageFit::Stretch => (bounds, [0.0, 0.0, 1.0, 1.0]),
        ImageFit::Contain => {
            if source_ratio > target_ratio {
                let height = bounds.width / source_ratio;
                (
                    Rect::new(
                        bounds.x,
                        bounds.y + (bounds.height - height) * 0.5,
                        bounds.width,
                        height,
                    ),
                    [0.0, 0.0, 1.0, 1.0],
                )
            } else {
                let width = bounds.height * source_ratio;
                (
                    Rect::new(
                        bounds.x + (bounds.width - width) * 0.5,
                        bounds.y,
                        width,
                        bounds.height,
                    ),
                    [0.0, 0.0, 1.0, 1.0],
                )
            }
        }
        ImageFit::Cover => {
            if source_ratio > target_ratio {
                let width = target_ratio / source_ratio;
                ((bounds), [(1.0 - width) * 0.5, 0.0, width, 1.0])
            } else {
                let height = source_ratio / target_ratio;
                ((bounds), [0.0, (1.0 - height) * 0.5, 1.0, height])
            }
        }
    }
}

fn transform_is_finite(transform: Transform2D) -> bool {
    [
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.tx,
        transform.ty,
    ]
    .iter()
    .all(|value| value.is_finite())
}

fn sane(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32) -> UiImage {
        UiImage::from_rgba(
            width,
            height,
            vec![255; width as usize * height as usize * 4],
        )
        .unwrap()
    }

    #[test]
    fn contain_preserves_aspect_ratio_inside_destination() {
        let (bounds, uv) = fitted_geometry(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &image(200, 100),
            ImageStyle {
                fit: ImageFit::Contain,
                ..ImageStyle::default()
            },
        );
        assert_eq!(bounds, Rect::new(0.0, 25.0, 100.0, 50.0));
        assert_eq!(uv, [0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn cover_crops_the_excess_source_axis() {
        let (bounds, uv) = fitted_geometry(
            Rect::new(0.0, 0.0, 100.0, 100.0),
            &image(200, 100),
            ImageStyle {
                fit: ImageFit::Cover,
                ..ImageStyle::default()
            },
        );
        assert_eq!(bounds, Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(uv, [0.25, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn repeated_image_resources_are_charged_once_per_frame() {
        let image = ImageKey {
            identity: 1,
            width: 2048,
            height: 2048,
        };
        let other = ImageKey {
            identity: 2,
            ..image
        };
        let image_bytes = 16 * 1024 * 1024;
        let mut budget = FrameImageBudget::default();

        assert!(budget.can_charge(image, image_bytes));
        budget.charge(image, image_bytes);
        for _ in 0..5 {
            assert!(budget.can_charge(image, image_bytes));
            budget.charge(image, image_bytes);
        }
        assert_eq!(budget.bytes, image_bytes);
        assert!(!budget.can_charge(other, MAX_IMAGE_FRAME_BYTES));
    }
}
