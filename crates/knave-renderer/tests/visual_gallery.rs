use std::{fs::File, sync::mpsc};

use knave_renderer::WgpuPainter;
use knave_ui::{
    Border, BoxShadow, Color, CornerRadii, DisplayListBuilder, ImageFit, ImageStyle, Rect,
    ShapePaint, TextStyle, Transform2D, UiImage,
};

const FRAME_SIZE: (u32, u32) = (1200, 820);
const BACKGROUND: Color = Color::rgba(22, 25, 31, 255);
const FOREGROUND: Color = Color::rgba(234, 237, 242, 255);
const MUTED: Color = Color::rgba(158, 166, 179, 255);

#[test]
fn gpu_renderer_visual_gallery() {
    let instance = wgpu::Instance::default();
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("skipping WGPU visual gallery: no headless adapter: {error}");
            return;
        }
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("request an offscreen rendering device");
    let info = adapter.get_info();
    eprintln!(
        "visual gallery adapter: {} ({:?}, {:?})",
        info.name, info.backend, info.device_type
    );

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("knave-renderer-gallery-target"),
        size: wgpu::Extent3d {
            width: FRAME_SIZE.0,
            height: FRAME_SIZE.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    let display_list = gallery_scene();
    let mut painter = WgpuPainter::new(&device, &queue, format).expect("sRGB output format");
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("knave-renderer-gallery-encoder"),
    });
    {
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("knave-renderer-gallery-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu_color(BACKGROUND)),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    painter
        .encode(
            &device,
            &queue,
            &mut encoder,
            &view,
            FRAME_SIZE,
            &display_list,
        )
        .expect("render the gallery through the production WGPU painter");

    let row_pitch = (FRAME_SIZE.0 * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("knave-renderer-gallery-readback"),
        size: u64::from(row_pitch) * u64::from(FRAME_SIZE.1),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_pitch),
                rows_per_image: Some(FRAME_SIZE.1),
            },
        },
        wgpu::Extent3d {
            width: FRAME_SIZE.0,
            height: FRAME_SIZE.1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("wait for gallery frame completion");
    receiver
        .recv()
        .expect("receive gallery readback callback")
        .expect("map gallery pixels");
    let mapped = slice.get_mapped_range().expect("read mapped gallery");
    let mut pixels = Vec::with_capacity((FRAME_SIZE.0 * FRAME_SIZE.1) as usize);
    for y in 0..FRAME_SIZE.1 {
        let row_start = (y * row_pitch) as usize;
        for x in 0..FRAME_SIZE.0 {
            let offset = row_start + (x * 4) as usize;
            pixels.push([
                mapped[offset],
                mapped[offset + 1],
                mapped[offset + 2],
                mapped[offset + 3],
            ]);
        }
    }
    assert!(
        pixels
            .iter()
            .filter(|pixel| {
                **pixel
                    != [
                        BACKGROUND.red,
                        BACKGROUND.green,
                        BACKGROUND.blue,
                        BACKGROUND.alpha,
                    ]
            })
            .count()
            > 80_000,
        "the gallery should contain substantial rendered content"
    );
    assert!(
        pixels
            .chunks(FRAME_SIZE.0 as usize)
            .skip(200)
            .take(280)
            .flat_map(|row| &row[..500])
            .any(|pixel| pixel[0] > 180 && pixel[1] > 180 && pixel[2] > 180),
        "the system-font samples should produce visible glyph pixels"
    );

    if let Ok(path) = std::env::var("KNAVE_RENDERER_GALLERY_FRAME") {
        write_png(&path, &pixels);
        eprintln!("wrote renderer gallery to {path}");
    }
    drop(mapped);
    readback.unmap();
}

fn gallery_scene() -> knave_ui::DisplayList {
    let mut builder = DisplayListBuilder::new(1, BACKGROUND);
    add_text(
        &mut builder,
        Rect::new(48.0, 30.0, 1080.0, 48.0),
        "WGPU renderer — visual review",
        32.0,
        600,
        FOREGROUND,
    );
    add_text(
        &mut builder,
        Rect::new(50.0, 86.0, 1080.0, 28.0),
        "System text, shape edges, image sampling, clipping, transforms and opacity",
        16.0,
        400,
        MUTED,
    );
    builder.fill_rect(
        Rect::new(48.0, 132.0, 1104.0, 1.0),
        Color::rgba(68, 74, 85, 255),
    );

    add_text(
        &mut builder,
        Rect::new(48.0, 150.0, 420.0, 36.0),
        "Text",
        23.0,
        500,
        FOREGROUND,
    );
    add_text(
        &mut builder,
        Rect::new(540.0, 150.0, 600.0, 36.0),
        "Shapes and effects",
        23.0,
        500,
        FOREGROUND,
    );
    builder.fill_rect(
        Rect::new(48.0, 195.0, 1104.0, 1.0),
        Color::rgba(50, 56, 66, 255),
    );

    add_text(
        &mut builder,
        Rect::new(50.0, 211.0, 440.0, 28.0),
        "13 px   The quick brown fox",
        13.0,
        400,
        MUTED,
    );
    add_text(
        &mut builder,
        Rect::new(50.0, 250.0, 440.0, 32.0),
        "18 px   Aa Gg Rr 0123456789",
        18.0,
        400,
        FOREGROUND,
    );
    add_text(
        &mut builder,
        Rect::new(50.0, 294.0, 440.0, 42.0),
        "26 px   Workspace overview",
        26.0,
        500,
        FOREGROUND,
    );
    add_text(
        &mut builder,
        Rect::new(50.0, 346.0, 440.0, 56.0),
        "38 px   Aa 09",
        38.0,
        500,
        Color::rgba(132, 181, 255, 255),
    );
    builder.push_clip(Rect::new(50.0, 414.0, 430.0, 70.0));
    add_text(
        &mut builder,
        Rect::new(50.0, 414.0, 430.0, 70.0),
        "A longer sentence wraps naturally inside a bounded text area, without drawing past its clip.",
        16.0,
        400,
        FOREGROUND,
    );
    builder.pop_clip().expect("text clip is balanced");

    shape_sample(
        &mut builder,
        Rect::new(540.0, 232.0, 108.0, 72.0),
        ShapePaint::fill(Color::rgba(116, 171, 248, 255)),
    );
    shape_sample(
        &mut builder,
        Rect::new(680.0, 232.0, 108.0, 72.0),
        ShapePaint {
            radii: CornerRadii::uniform(18.0),
            ..ShapePaint::fill(Color::rgba(112, 196, 160, 255))
        },
    );
    shape_sample(
        &mut builder,
        Rect::new(820.0, 232.0, 108.0, 72.0),
        ShapePaint {
            fill: None,
            border: Some(Border {
                color: Color::rgba(247, 179, 104, 255),
                width: 4.0,
            }),
            radii: CornerRadii::uniform(12.0),
            ..ShapePaint::default()
        },
    );
    shape_sample(
        &mut builder,
        Rect::new(960.0, 232.0, 108.0, 72.0),
        ShapePaint {
            shadow: Some(BoxShadow {
                color: Color::rgba(0, 0, 0, 190),
                offset_x: 7.0,
                offset_y: 8.0,
                blur_radius: 9.0,
                spread: 1.0,
            }),
            ..ShapePaint::fill(Color::rgba(79, 92, 111, 255))
        },
    );
    for (x, label) in [
        (540.0, "Square"),
        (680.0, "Rounded"),
        (820.0, "Border"),
        (960.0, "Shadow"),
    ] {
        add_text(
            &mut builder,
            Rect::new(x, 312.0, 120.0, 24.0),
            label,
            14.0,
            400,
            MUTED,
        );
    }

    shape_sample(
        &mut builder,
        Rect::new(540.0, 360.0, 128.0, 82.0),
        ShapePaint::fill(Color::rgba(124, 132, 145, 255)),
    );
    shape_sample(
        &mut builder,
        Rect::new(574.0, 379.0, 90.0, 54.0),
        ShapePaint {
            opacity: 0.58,
            ..ShapePaint::fill(Color::rgba(103, 157, 245, 255))
        },
    );
    add_text(
        &mut builder,
        Rect::new(540.0, 450.0, 140.0, 24.0),
        "Layered opacity",
        14.0,
        400,
        MUTED,
    );

    let rotation = Transform2D::translation(785.0, 399.0)
        .compose(Transform2D::rotation(-0.24))
        .compose(Transform2D::translation(-785.0, -399.0));
    builder.push_transform(rotation);
    shape_sample(
        &mut builder,
        Rect::new(728.0, 366.0, 114.0, 66.0),
        ShapePaint::fill(Color::rgba(214, 151, 108, 255)),
    );
    builder
        .pop_transform()
        .expect("shape transform is balanced");
    add_text(
        &mut builder,
        Rect::new(710.0, 450.0, 160.0, 24.0),
        "Affine rotation",
        14.0,
        400,
        MUTED,
    );

    shape_sample(
        &mut builder,
        Rect::new(906.0, 356.0, 216.0, 86.0),
        ShapePaint::fill(Color::rgba(48, 56, 68, 255)),
    );
    builder.push_clip(Rect::new(972.0, 364.0, 116.0, 70.0));
    shape_sample(
        &mut builder,
        Rect::new(930.0, 370.0, 220.0, 54.0),
        ShapePaint::fill(Color::rgba(115, 165, 233, 255)),
    );
    add_text(
        &mut builder,
        Rect::new(972.0, 381.0, 208.0, 34.0),
        "This text continues beyond the edge",
        17.0,
        500,
        FOREGROUND,
    );
    builder.pop_clip().expect("shape clip is balanced");
    add_text(
        &mut builder,
        Rect::new(920.0, 450.0, 190.0, 24.0),
        "Nested rectangular clip",
        14.0,
        400,
        MUTED,
    );

    builder.fill_rect(
        Rect::new(48.0, 505.0, 1104.0, 1.0),
        Color::rgba(50, 56, 66, 255),
    );
    add_text(
        &mut builder,
        Rect::new(48.0, 520.0, 500.0, 36.0),
        "Images",
        23.0,
        500,
        FOREGROUND,
    );
    let image = gradient_image(192, 96);
    let image_examples = [
        (
            Rect::new(48.0, 600.0, 220.0, 126.0),
            ImageStyle {
                fit: ImageFit::Stretch,
                ..ImageStyle::default()
            },
            "Stretch",
        ),
        (
            Rect::new(300.0, 590.0, 170.0, 146.0),
            ImageStyle {
                fit: ImageFit::Contain,
                ..ImageStyle::default()
            },
            "Contain",
        ),
        (
            Rect::new(500.0, 590.0, 170.0, 146.0),
            ImageStyle {
                fit: ImageFit::Cover,
                ..ImageStyle::default()
            },
            "Cover",
        ),
        (
            Rect::new(700.0, 600.0, 220.0, 126.0),
            ImageStyle {
                opacity: 0.74,
                corner_radius: 22.0,
                ..ImageStyle::default()
            },
            "Rounded + opacity",
        ),
    ];
    for (bounds, style, label) in image_examples {
        shape_sample(
            &mut builder,
            bounds,
            ShapePaint::fill(Color::rgba(57, 64, 76, 255)),
        );
        builder.image(bounds, image.clone(), style);
        add_text(
            &mut builder,
            Rect::new(bounds.x, 568.0, bounds.width, 24.0),
            label,
            14.0,
            400,
            MUTED,
        );
    }
    let transformed_bounds = Rect::new(970.0, 604.0, 120.0, 106.0);
    shape_sample(
        &mut builder,
        transformed_bounds,
        ShapePaint::fill(Color::rgba(57, 64, 76, 255)),
    );
    let image_transform = Transform2D::translation(1030.0, 657.0)
        .compose(Transform2D::rotation(0.16))
        .compose(Transform2D::translation(-1030.0, -657.0));
    builder.push_transform(image_transform);
    builder.image(
        transformed_bounds,
        image,
        ImageStyle {
            fit: ImageFit::Cover,
            corner_radius: 8.0,
            ..ImageStyle::default()
        },
    );
    builder
        .pop_transform()
        .expect("image transform is balanced");
    add_text(
        &mut builder,
        Rect::new(970.0, 568.0, 170.0, 24.0),
        "Transformed image",
        14.0,
        400,
        MUTED,
    );

    builder.finish().expect("gallery scopes are balanced")
}

fn shape_sample(builder: &mut DisplayListBuilder, bounds: Rect, paint: ShapePaint) {
    builder.shape(bounds, paint);
}

fn add_text(
    builder: &mut DisplayListBuilder,
    bounds: Rect,
    text: &str,
    font_size: f32,
    weight: u16,
    color: Color,
) {
    builder.text(
        bounds,
        text.to_owned(),
        TextStyle {
            color,
            font_size,
            line_height: font_size * 1.28,
            weight,
            ..TextStyle::default()
        },
    );
}

fn gradient_image(width: u32, height: u32) -> UiImage {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let checker = (x / 16 + y / 12) % 2 == 0;
            pixels.extend_from_slice(&[
                (x * 255 / (width - 1)) as u8,
                (y * 255 / (height - 1)) as u8,
                if checker { 220 } else { 64 },
                255,
            ]);
        }
    }
    UiImage::from_rgba(width, height, pixels).expect("valid generated RGBA sample")
}

fn wgpu_color(color: Color) -> wgpu::Color {
    let [red, green, blue, alpha] = color.to_linear_rgba();
    wgpu::Color {
        r: f64::from(red),
        g: f64::from(green),
        b: f64::from(blue),
        a: f64::from(alpha),
    }
}

fn write_png(path: &str, pixels: &[[u8; 4]]) {
    let file = File::create(path).expect("create requested renderer gallery PNG");
    let mut encoder = png::Encoder::new(file, FRAME_SIZE.0, FRAME_SIZE.1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write gallery PNG header");
    let rgba = pixels.iter().flatten().copied().collect::<Vec<_>>();
    writer
        .write_image_data(&rgba)
        .expect("write renderer gallery pixels");
}
