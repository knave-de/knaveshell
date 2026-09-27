use bytemuck::{Pod, Zeroable};
use knave_ui::{Color, DisplayCommand, Rect, ShapePaint, Transform2D};

const SHADER: &str = r#"
struct Viewport { size: vec2<f32> };
struct ShapeInstance {
    bounds: vec4<f32>,
    quad: vec4<f32>,
    matrix: vec4<f32>,
    translation: vec4<f32>,
    radii: vec4<f32>,
    fill: vec4<f32>,
    border: vec4<f32>,
    parameters: vec4<f32>,
    shadow: vec4<f32>,
    shadow_data: vec4<f32>,
};

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(0) @binding(1) var<storage, read> shapes: array<ShapeInstance>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) instance_index: u32,
};

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32,
               @builtin(instance_index) instance_index: u32) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0));
    let shape = shapes[instance_index];
    let unit = corners[vertex_index];
    let local = shape.quad.xy + unit * shape.quad.zw;
    let point = shape.bounds.xy + local;
    let world = vec2(
        shape.matrix.x * point.x + shape.matrix.z * point.y + shape.translation.x,
        shape.matrix.y * point.x + shape.matrix.w * point.y + shape.translation.y);
    let normalized = vec2(
        world.x / viewport.size.x * 2.0 - 1.0,
        1.0 - world.y / viewport.size.y * 2.0);
    var output: VertexOutput;
    output.position = vec4(normalized, 0.0, 1.0);
    output.local = local;
    output.instance_index = instance_index;
    return output;
}

fn rounded_box_distance(point: vec2<f32>, size: vec2<f32>, radii: vec4<f32>) -> f32 {
    let center = point - size * 0.5;
    var radius = radii.x;
    if (center.x >= 0.0 && center.y < 0.0) { radius = radii.y; }
    if (center.x >= 0.0 && center.y >= 0.0) { radius = radii.z; }
    if (center.x < 0.0 && center.y >= 0.0) { radius = radii.w; }
    radius = clamp(radius, 0.0, min(size.x, size.y) * 0.5);
    let q = abs(center) - size * 0.5 + vec2(radius);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let shape = shapes[input.instance_index];
    let size = shape.bounds.zw;
    let distance = rounded_box_distance(input.local, size, shape.radii);
    let antialias = max(fwidth(distance), 0.75);
    let opacity = clamp(shape.parameters.y, 0.0, 1.0);
    if (distance <= antialias) {
        let outer = 1.0 - smoothstep(-antialias, antialias, distance);
        let border_width = max(shape.parameters.x, 0.0);
        let inner = 1.0 - smoothstep(-antialias, antialias, distance + border_width);
        let border_amount = clamp(outer - inner, 0.0, 1.0);
        let fill_amount = select(outer, inner, border_width > 0.0 && shape.border.a > 0.0);
        let border_alpha = shape.border.a * border_amount;
        let fill_alpha = shape.fill.a * fill_amount;
        let alpha = (border_alpha + fill_alpha * (1.0 - border_alpha)) * opacity;
        if (alpha <= 0.0) { return vec4(0.0); }
        let rgb = (shape.border.rgb * border_alpha + shape.fill.rgb * fill_alpha * (1.0 - border_alpha))
            / max(border_alpha + fill_alpha * (1.0 - border_alpha), 0.00001);
        return vec4(rgb, alpha);
    }

    if (shape.shadow.a <= 0.0) { return vec4(0.0); }
    let shadow_point = input.local - shape.shadow_data.xy;
    let shadow_distance = rounded_box_distance(shadow_point, size, shape.radii) - shape.shadow_data.w;
    let blur = max(shape.shadow_data.z, antialias);
    let outside = max(shadow_distance, 0.0);
    if (outside > blur * 4.0) { return vec4(0.0); }
    let shadow_alpha = shape.shadow.a * exp(-0.5 * (outside / blur) * (outside / blur)) * opacity;
    return vec4(shape.shadow.rgb, shadow_alpha);
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
struct ShapeInstance {
    bounds: [f32; 4],
    quad: [f32; 4],
    matrix: [f32; 4],
    translation: [f32; 4],
    radii: [f32; 4],
    fill: [f32; 4],
    border: [f32; 4],
    meta: [f32; 4],
    shadow: [f32; 4],
    shadow_data: [f32; 4],
}

pub(super) struct ShapePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    viewport: wgpu::Buffer,
    bind_group: Option<wgpu::BindGroup>,
    instances: Option<wgpu::Buffer>,
    prepared: Vec<Option<(usize, Option<Rect>)>>,
    staging: Vec<ShapeInstance>,
    capacity: usize,
    pub allocations: u64,
}

impl ShapePass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("knave-shapes-layout"),
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
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("knave-shapes-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("knave-shapes-pipeline-layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("knave-shapes-pipeline"),
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
            label: Some("knave-shapes-viewport"),
            size: std::mem::size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            layout,
            viewport,
            bind_group: None,
            instances: None,
            prepared: Vec::new(),
            staging: Vec::new(),
            capacity: 0,
            allocations: 0,
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
        self.staging.clear();
        let instances = &mut self.staging;
        self.prepared.clear();
        self.prepared.resize(commands.len(), None);
        for (command_index, command) in commands.iter().enumerate() {
            let DisplayCommand::Shape {
                bounds,
                paint,
                transform,
                clip,
            } = command
            else {
                continue;
            };
            if !bounds.is_finite_positive()
                || !transform_is_finite(*transform)
                || !transform
                    .transform_rect_bounds(*bounds)
                    .is_finite_positive()
            {
                continue;
            }
            let index = instances.len();
            instances.push(instance(*bounds, paint, *transform));
            self.prepared[command_index] = Some((index, *clip));
        }
        if instances.is_empty() {
            return;
        }
        if instances.len() > self.capacity {
            self.capacity = instances.len().next_power_of_two();
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("knave-shapes-instances"),
                size: (self.capacity * std::mem::size_of::<ShapeInstance>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("knave-shapes-bind-group"),
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
                ],
            }));
            self.instances = Some(buffer);
            self.allocations += 1;
        }
        queue.write_buffer(
            self.instances.as_ref().expect("shape capacity allocated"),
            0,
            bytemuck::cast_slice(instances),
        );
    }

    pub fn draw_for_command(&self, command_index: usize) -> Option<(usize, Option<Rect>)> {
        self.prepared.get(command_index).copied().flatten()
    }

    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, instance: usize, scissor: [u32; 4]) {
        let Some(bind_group) = &self.bind_group else {
            return;
        };
        pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..6, instance as u32..instance as u32 + 1);
    }
}

fn instance(bounds: Rect, paint: &ShapePaint, transform: Transform2D) -> ShapeInstance {
    let color = paint.fill.map(color_array).unwrap_or([0.0; 4]);
    let border = paint
        .border
        .map(|border| color_array(border.color))
        .unwrap_or([0.0; 4]);
    let shadow = paint
        .shadow
        .map(|shadow| color_array(shadow.color))
        .unwrap_or([0.0; 4]);
    let radii = paint.radii;
    let (left, top, right, bottom) =
        paint
            .shadow
            .map_or((0.0, 0.0, bounds.width, bounds.height), |shadow| {
                let extent = sane(shadow.blur_radius).clamp(0.0, 2048.0).max(0.75) * 4.0
                    + sane(shadow.spread).clamp(0.0, 2048.0);
                let offset_x = sane(shadow.offset_x).clamp(-4096.0, 4096.0);
                let offset_y = sane(shadow.offset_y).clamp(-4096.0, 4096.0);
                (
                    0.0_f32.min(offset_x - extent),
                    0.0_f32.min(offset_y - extent),
                    bounds.width.max(bounds.width + offset_x + extent),
                    bounds.height.max(bounds.height + offset_y + extent),
                )
            });
    let shadow_data = paint.shadow.map_or([0.0; 4], |shadow| {
        [
            sane(shadow.offset_x).clamp(-4096.0, 4096.0),
            sane(shadow.offset_y).clamp(-4096.0, 4096.0),
            sane(shadow.blur_radius).clamp(0.0, 2048.0),
            sane(shadow.spread).clamp(-2048.0, 2048.0),
        ]
    });
    ShapeInstance {
        bounds: [bounds.x, bounds.y, bounds.width, bounds.height],
        quad: [left, top, right - left, bottom - top],
        matrix: [transform.a, transform.b, transform.c, transform.d],
        translation: [transform.tx, transform.ty, 0.0, 0.0],
        radii: [
            sane(radii.top_left).max(0.0),
            sane(radii.top_right).max(0.0),
            sane(radii.bottom_right).max(0.0),
            sane(radii.bottom_left).max(0.0),
        ],
        fill: color,
        border,
        meta: [
            paint
                .border
                .map_or(0.0, |border| sane(border.width).max(0.0)),
            sane(paint.opacity).clamp(0.0, 1.0),
            0.0,
            0.0,
        ],
        shadow,
        shadow_data,
    }
}

fn color_array(color: Color) -> [f32; 4] {
    color.to_linear_rgba()
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
    use knave_ui::BoxShadow;

    #[test]
    fn shadow_geometry_expands_the_quad_past_shape_bounds() {
        let paint = ShapePaint {
            shadow: Some(BoxShadow {
                color: Color::rgba(0, 0, 0, 160),
                offset_x: 3.0,
                offset_y: -2.0,
                blur_radius: 2.0,
                spread: 1.0,
            }),
            ..ShapePaint::fill(Color::ACCENT)
        };
        let instance = instance(
            Rect::new(10.0, 20.0, 10.0, 8.0),
            &paint,
            Transform2D::IDENTITY,
        );
        assert_eq!(instance.quad, [-6.0, -11.0, 28.0, 26.0]);
        assert_eq!(instance.bounds, [10.0, 20.0, 10.0, 8.0]);
    }
}
