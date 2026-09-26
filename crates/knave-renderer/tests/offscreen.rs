use knave_renderer::WgpuPainter;
use knave_ui::{
    BoxShadow, Color, CornerRadii, DisplayCommand, DisplayList, ImageStyle, Rect, ShapePaint,
    TextStyle, Transform2D, UiImage,
};

#[test]
fn offscreen_frame_preserves_order_clips_images_and_renders_real_text() {
    let instance = wgpu::Instance::default();
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("skipping WGPU readback test: no headless adapter: {error}");
            return;
        }
    };
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("request an offscreen rendering device");
    let adapter_info = adapter.get_info();
    eprintln!(
        "offscreen adapter: {} ({:?}, {:?})",
        adapter_info.name, adapter_info.backend, adapter_info.device_type
    );
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let size = (32, 32);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("knave-renderer-offscreen-target"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
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
    let mut painter = WgpuPainter::new(&device, &queue, format);

    let image = UiImage::from_rgba(1, 1, vec![0, 0, 255, 255]).unwrap();
    let list = DisplayList {
        revision: 1,
        clear_color: Color::rgba(0, 0, 0, 255),
        commands: vec![
            DisplayCommand::Shape {
                bounds: Rect::new(1.0, 1.0, 12.0, 12.0),
                paint: ShapePaint {
                    radii: CornerRadii::uniform(2.0),
                    ..ShapePaint::fill(Color::rgba(255, 0, 0, 255))
                },
                transform: Transform2D::IDENTITY,
                clip: None,
            },
            DisplayCommand::Image {
                bounds: Rect::new(3.0, 3.0, 8.0, 8.0),
                image,
                style: ImageStyle::default(),
                transform: Transform2D::IDENTITY,
                clip: None,
            },
            DisplayCommand::Shape {
                bounds: Rect::new(6.0, 6.0, 4.0, 4.0),
                paint: ShapePaint::fill(Color::rgba(0, 255, 0, 255)),
                transform: Transform2D::IDENTITY,
                clip: Some(Rect::new(6.0, 6.0, 2.0, 4.0)),
            },
            DisplayCommand::Text {
                bounds: Rect::new(1.0, 16.0, 29.0, 14.0),
                style: TextStyle {
                    font_size: 11.0,
                    line_height: 13.0,
                    color: Color::rgba(255, 255, 255, 255),
                    ..TextStyle::default()
                },
                text: "Real font".into(),
                transform: Transform2D::IDENTITY,
                clip: None,
            },
            DisplayCommand::Shape {
                bounds: Rect::new(16.0, 16.0, 2.0, 14.0),
                paint: ShapePaint::fill(Color::rgba(255, 255, 0, 255)),
                transform: Transform2D::IDENTITY,
                clip: None,
            },
            DisplayCommand::Text {
                bounds: Rect::new(20.0, 16.0, 11.0, 14.0),
                style: TextStyle {
                    font_size: 11.0,
                    line_height: 13.0,
                    color: Color::rgba(255, 255, 255, 255),
                    ..TextStyle::default()
                },
                text: "B".into(),
                transform: Transform2D::IDENTITY,
                clip: None,
            },
            DisplayCommand::Shape {
                bounds: Rect::new(18.0, 2.0, 5.0, 7.0),
                paint: ShapePaint {
                    shadow: Some(BoxShadow {
                        color: Color::rgba(255, 0, 0, 255),
                        offset_x: 3.0,
                        offset_y: 0.0,
                        blur_radius: 2.0,
                        spread: 1.0,
                    }),
                    ..ShapePaint::fill(Color::rgba(255, 255, 0, 255))
                },
                transform: Transform2D::IDENTITY,
                clip: None,
            },
            DisplayCommand::Shape {
                bounds: Rect::new(1.0, 1.0, 3.0, 3.0),
                paint: ShapePaint::fill(Color::rgba(255, 255, 0, 255)),
                transform: Transform2D::translation(16.0, 10.0),
                clip: None,
            },
        ],
    };
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("knave-renderer-offscreen-encoder"),
    });
    {
        let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("knave-renderer-offscreen-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
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
        .encode(&device, &queue, &mut encoder, &view, size, &list)
        .expect("prepare and encode the offscreen frame");

    let row_pitch = (size.0 * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("knave-renderer-offscreen-readback"),
        size: u64::from(row_pitch) * u64::from(size.1),
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
                rows_per_image: Some(size.1),
            },
        },
        wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = readback.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("wait for offscreen frame completion");
    receiver
        .recv()
        .expect("receive readback callback")
        .expect("map rendered pixels");
    let mapped = slice.get_mapped_range().expect("read mapped pixels");
    let mut pixels = Vec::with_capacity((size.0 * size.1) as usize);
    for y in 0..size.1 {
        let row_start = (y * row_pitch) as usize;
        for x in 0..size.0 {
            let offset = row_start + (x * 4) as usize;
            pixels.push([
                mapped[offset],
                mapped[offset + 1],
                mapped[offset + 2],
                mapped[offset + 3],
            ]);
        }
    }
    let pixel = |x: usize, y: usize| pixels[y * size.0 as usize + x];

    assert_eq!(pixel(2, 2), [255, 0, 0, 255], "shape should be visible");
    assert_eq!(
        pixel(4, 4),
        [0, 0, 255, 255],
        "image should draw over the first shape"
    );
    assert_eq!(
        pixel(7, 7),
        [0, 255, 0, 255],
        "later shape should draw over the image"
    );
    assert_eq!(
        pixel(9, 7),
        [0, 0, 255, 255],
        "shape clip should preserve the image outside it"
    );
    assert_eq!(
        pixel(31, 30),
        [0, 0, 0, 255],
        "rounded shape should not leak outside bounds"
    );
    assert_eq!(
        pixel(25, 5),
        [255, 0, 0, 255],
        "blurred shadow should render beyond the shape quad"
    );
    let translated = pixel(18, 12);
    assert!(
        translated[0] > 240 && translated[1] > 240 && translated[2] < 8,
        "affine translation should affect GPU geometry: {translated:?}"
    );
    assert!(
        pixels[16 * size.0 as usize..]
            .chunks_exact(size.0 as usize)
            .take(15)
            .any(|row| row[..12]
                .iter()
                .any(|pixel| pixel[0] > 20 && pixel[1] > 20 && pixel[2] > 20)),
        "Cosmic Text/Glyphon should rasterize a real system-font run"
    );
    assert!(
        pixels[16 * size.0 as usize..]
            .chunks_exact(size.0 as usize)
            .take(15)
            .any(|row| row[20..31]
                .iter()
                .any(|pixel| pixel[0] > 20 && pixel[1] > 20 && pixel[2] > 20)),
        "a later text run after a shape should retain its glyph-atlas entries"
    );

    if let Ok(path) = std::env::var("KNAVE_RENDERER_TEST_FRAME") {
        let file = std::fs::File::create(path).expect("create requested frame dump");
        let mut encoder = png::Encoder::new(file, size.0, size.1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("write PNG header");
        let rgba = pixels.iter().flatten().copied().collect::<Vec<_>>();
        writer.write_image_data(&rgba).expect("write PNG pixels");
    }
    drop(mapped);
    readback.unmap();
}
