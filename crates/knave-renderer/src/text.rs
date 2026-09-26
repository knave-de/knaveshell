use std::{collections::HashMap, sync::Arc};

use glyphon::{
    Attrs, Buffer, Cache, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache, TextArea,
    TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use knave_ui::{Rect, TextAlign, TextStyle, TextWrap, Transform2D};

const MAX_CACHED_LAYOUTS: usize = 128;
const MAX_LAYOUT_CACHE_BYTES: usize = 4 * 1024 * 1024;
const MAX_CACHED_TEXT_BYTES: usize = 16 * 1024;
const MAX_TEXT_RENDERERS: usize = 128;
const MAX_FRAME_TEXT_BYTES: usize = 512 * 1024;
const MAX_FONT_FAMILY_BYTES: usize = 256;

#[derive(Debug)]
pub(crate) enum TextPassError {
    Glyphon(glyphon::PrepareError),
    TooManyRuns,
    UnsupportedTransform,
    TextBytesExceeded,
    FontFamilyTooLong,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LayoutKey {
    text: String,
    family: String,
    width: u32,
    height: u32,
    font_size: u32,
    line_height: u32,
    weight: u16,
    wrap: TextWrap,
    align: TextAlign,
}

impl LayoutKey {
    fn new(text: &str, style: &TextStyle, bounds: Rect) -> Self {
        Self {
            text: text.to_owned(),
            family: style.font_family.clone(),
            width: sane_dimension(bounds.width).to_bits(),
            height: sane_dimension(bounds.height).to_bits(),
            font_size: sane_font_metric(style.font_size).to_bits(),
            line_height: sane_font_metric(style.line_height).to_bits(),
            weight: style.weight.clamp(100, 900),
            wrap: style.wrap,
            align: style.align,
        }
    }

    fn estimated_bytes(&self) -> usize {
        self.text
            .len()
            .saturating_mul(64)
            .saturating_add(self.family.len())
            .saturating_add(std::mem::size_of::<Buffer>())
    }
}

struct CachedLayout {
    buffer: Arc<Buffer>,
    bytes: usize,
    last_used: u64,
}

/// Glyphon/Cosmic Text integration. One font database and glyph atlas are kept
/// for the lifetime of the renderer; text areas are cached within a fixed budget.
pub(super) struct TextPass {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    viewport: Viewport,
    renderers: Vec<TextRenderer>,
    layouts: HashMap<LayoutKey, CachedLayout>,
    layout_bytes: usize,
    clock: u64,
    active_runs: usize,
    frame_text_bytes: usize,
}

pub(super) struct TextItem<'a> {
    pub bounds: Rect,
    pub text: &'a str,
    pub style: &'a TextStyle,
    pub transform: Transform2D,
    pub clip: Option<Rect>,
}

impl TextPass {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let cache = Cache::new(device);
        let atlas = TextAtlas::new(device, queue, &cache, format);
        let viewport = Viewport::new(device, &cache);
        Self {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            atlas,
            viewport,
            renderers: Vec::new(),
            layouts: HashMap::new(),
            layout_bytes: 0,
            clock: 0,
            active_runs: 0,
            frame_text_bytes: 0,
        }
    }

    pub fn begin_frame(&mut self, queue: &wgpu::Queue, size: (u32, u32)) {
        self.atlas.trim();
        self.viewport.update(
            queue,
            Resolution {
                width: size.0.max(1),
                height: size.1.max(1),
            },
        );
        self.active_runs = 0;
        self.frame_text_bytes = 0;
    }

    pub fn prepare_run(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        items: &[TextItem<'_>],
    ) -> Result<usize, TextPassError> {
        if items
            .iter()
            .any(|item| item.style.font_family.len() > MAX_FONT_FAMILY_BYTES)
        {
            return Err(TextPassError::FontFamilyTooLong);
        }
        let run_text_bytes = items
            .iter()
            .try_fold(0usize, |total, item| total.checked_add(item.text.len()))
            .ok_or(TextPassError::TextBytesExceeded)?;
        self.frame_text_bytes = self
            .frame_text_bytes
            .checked_add(run_text_bytes)
            .filter(|bytes| *bytes <= MAX_FRAME_TEXT_BYTES)
            .ok_or(TextPassError::TextBytesExceeded)?;
        let viewport_size = self.viewport.resolution();
        let mut prepared = Vec::with_capacity(items.len());
        for item in items {
            let Some(area) = prepared_area(item, viewport_size.width, viewport_size.height)? else {
                continue;
            };
            let key = LayoutKey::new(item.text, item.style, item.bounds);
            self.clock = self.clock.wrapping_add(1);
            if !self.layouts.contains_key(&key) {
                self.insert_layout(key.clone(), item);
            }
            let buffer_key = if let Some(layout) = self.layouts.get_mut(&key) {
                layout.last_used = self.clock;
                Arc::clone(&layout.buffer)
            } else {
                // Oversized layouts are shaped for this frame without entering the bounded cache.
                let buffer = Arc::new(create_buffer(&mut self.font_system, item));
                prepared.push(PreparedArea {
                    buffer,
                    left: area.left,
                    top: area.top,
                    scale: area.scale,
                    bounds: area.clip,
                    color: glyphon::Color::rgba(
                        item.style.color.red,
                        item.style.color.green,
                        item.style.color.blue,
                        item.style.color.alpha,
                    ),
                });
                continue;
            };
            prepared.push(PreparedArea {
                buffer: buffer_key,
                left: area.left,
                top: area.top,
                scale: area.scale,
                bounds: area.clip,
                color: glyphon::Color::rgba(
                    item.style.color.red,
                    item.style.color.green,
                    item.style.color.blue,
                    item.style.color.alpha,
                ),
            });
        }

        if prepared.is_empty() {
            return Ok(usize::MAX);
        }

        let run_index = self.active_runs;
        if run_index >= MAX_TEXT_RENDERERS {
            return Err(TextPassError::TooManyRuns);
        }
        if run_index == self.renderers.len() {
            self.renderers.push(TextRenderer::new(
                &mut self.atlas,
                device,
                wgpu::MultisampleState::default(),
                None,
            ));
        }
        let Self {
            font_system,
            swash_cache,
            atlas,
            viewport,
            renderers,
            ..
        } = self;
        let areas = prepared.iter().map(|area| TextArea {
            buffer: &area.buffer,
            left: area.left,
            top: area.top,
            scale: area.scale,
            bounds: area.bounds,
            default_color: area.color,
            custom_glyphs: &[],
        });
        renderers[run_index]
            .prepare(
                device,
                queue,
                font_system,
                atlas,
                viewport,
                areas,
                swash_cache,
            )
            .map_err(TextPassError::Glyphon)?;
        self.active_runs += 1;
        Ok(run_index)
    }

    pub fn render<'a>(
        &'a self,
        run: usize,
        pass: &mut wgpu::RenderPass<'a>,
    ) -> Result<(), glyphon::RenderError> {
        self.renderers[run].render(&self.atlas, &self.viewport, pass)
    }

    fn insert_layout(&mut self, key: LayoutKey, item: &TextItem<'_>) {
        let bytes = key.estimated_bytes();
        if key.text.len() > MAX_CACHED_TEXT_BYTES || bytes > MAX_LAYOUT_CACHE_BYTES {
            return;
        }
        while self.layouts.len() >= MAX_CACHED_LAYOUTS
            || self.layout_bytes.saturating_add(bytes) > MAX_LAYOUT_CACHE_BYTES
        {
            let Some(oldest) = self
                .layouts
                .iter()
                .min_by_key(|(_, layout)| layout.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(layout) = self.layouts.remove(&oldest) {
                self.layout_bytes = self.layout_bytes.saturating_sub(layout.bytes);
            }
        }
        let buffer = create_buffer(&mut self.font_system, item);
        self.layout_bytes += bytes;
        self.layouts.insert(
            key,
            CachedLayout {
                buffer: Arc::new(buffer),
                bytes,
                last_used: self.clock,
            },
        );
    }
}

struct PreparedArea {
    buffer: Arc<Buffer>,
    left: f32,
    top: f32,
    scale: f32,
    bounds: TextBounds,
    color: glyphon::Color,
}

struct PreparedGeometry {
    left: f32,
    top: f32,
    scale: f32,
    clip: TextBounds,
}

fn prepared_area(
    item: &TextItem<'_>,
    width: u32,
    height: u32,
) -> Result<Option<PreparedGeometry>, TextPassError> {
    if !item.bounds.is_finite_positive() {
        return Ok(None);
    }
    let transform = item.transform;
    if ![
        transform.a,
        transform.b,
        transform.c,
        transform.d,
        transform.tx,
        transform.ty,
    ]
    .iter()
    .all(|value| value.is_finite())
    {
        return Ok(None);
    }
    let x_scale = (transform.a * transform.a + transform.b * transform.b).sqrt();
    let y_scale = (transform.c * transform.c + transform.d * transform.d).sqrt();
    // Glyphon supports positioned/scaled text but not arbitrary affine glyph rotation.
    if !x_scale.is_finite()
        || x_scale <= 0.0
        || (x_scale - y_scale).abs() > 0.001
        || transform.b.abs() > 0.001
        || transform.c.abs() > 0.001
        || transform.a <= 0.0
        || transform.d <= 0.0
    {
        return Err(TextPassError::UnsupportedTransform);
    }
    let origin = transform.transform_point([item.bounds.x, item.bounds.y]);
    let transformed = transform.transform_rect_bounds(item.bounds);
    if !origin[0].is_finite() || !origin[1].is_finite() || !transformed.is_finite_positive() {
        return Ok(None);
    }
    let visible = match item.clip {
        Some(clip) => match clip.intersection(transformed) {
            Some(visible) => visible,
            None => return Ok(None),
        },
        None => transformed,
    };
    let Some(visible) = visible.intersection(Rect::new(0.0, 0.0, width as f32, height as f32))
    else {
        return Ok(None);
    };
    Ok(Some(PreparedGeometry {
        left: origin[0],
        top: origin[1],
        scale: x_scale,
        clip: TextBounds {
            left: floor_to_i32(visible.x),
            top: floor_to_i32(visible.y),
            right: ceil_to_i32(visible.x + visible.width),
            bottom: ceil_to_i32(visible.y + visible.height),
        },
    }))
}

fn create_buffer(font_system: &mut FontSystem, item: &TextItem<'_>) -> Buffer {
    let font_size = sane_font_metric(item.style.font_size);
    let line_height = sane_font_metric(item.style.line_height);
    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_height));
    buffer.set_size(
        Some(sane_dimension(item.bounds.width)),
        Some(sane_dimension(item.bounds.height)),
    );
    buffer.set_wrap(match item.style.wrap {
        TextWrap::None => Wrap::None,
        TextWrap::Word => Wrap::Word,
        TextWrap::WordOrGlyph => Wrap::WordOrGlyph,
    });
    let family = match item.style.font_family.as_str() {
        "serif" => Family::Serif,
        "sans-serif" => Family::SansSerif,
        "cursive" => Family::Cursive,
        "fantasy" => Family::Fantasy,
        "monospace" => Family::Monospace,
        name => Family::Name(name),
    };
    let attrs = Attrs::new()
        .family(family)
        .weight(Weight(item.style.weight.clamp(100, 900)));
    let align = match item.style.align {
        TextAlign::Start => None,
        TextAlign::Center => Some(glyphon::cosmic_text::Align::Center),
        TextAlign::End => Some(glyphon::cosmic_text::Align::End),
    };
    buffer.set_text(item.text, &attrs, Shaping::Advanced, align);
    buffer.shape_until_scroll(font_system, false);
    buffer
}

fn sane_dimension(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 16_384.0)
    } else {
        0.0
    }
}

fn sane_font_metric(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.5, 512.0)
    } else {
        14.0
    }
}

fn floor_to_i32(value: f32) -> i32 {
    value.floor().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

fn ceil_to_i32(value: f32) -> i32 {
    value.ceil().clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_transform_support_is_explicit() {
        let style = TextStyle::default();
        let item = TextItem {
            bounds: Rect::new(4.0, 5.0, 20.0, 10.0),
            text: "sample",
            style: &style,
            transform: Transform2D::translation(2.0, 3.0).compose(Transform2D::scale(2.0, 2.0)),
            clip: None,
        };
        let area = prepared_area(&item, 100, 100).unwrap().unwrap();
        assert_eq!([area.left, area.top, area.scale], [10.0, 13.0, 2.0]);

        let rotated = TextItem {
            transform: Transform2D::rotation(0.25),
            ..item
        };
        assert!(matches!(
            prepared_area(&rotated, 100, 100),
            Err(TextPassError::UnsupportedTransform)
        ));
    }
}
