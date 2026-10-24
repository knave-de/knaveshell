//! The official Knave organization mark, bundled as a PNG asset.
//!
//! The bar renders the mark icon-only; it never paints a wordmark next to it.
//! Decode once per process so repeated bar compositions never reparse the file.

use std::sync::OnceLock;

use crate::UiImage;

/// Exact 128×128 pixels of the knave-de organization mark, no wordmark.
const LOGO_PNG: &[u8] = include_bytes!("../assets/knave-logo.png");

const MAX_BRAND_PIXELS: usize = 1024 * 1024;

/// Bound decoder growth instead of trusting the bundled asset implicitly.
fn bounded_buffer_size(info: &png::Info<'_>) -> Option<usize> {
    let pixel_count = u64::from(info.width) * u64::from(info.height);
    if pixel_count == 0 || pixel_count > u64::try_from(MAX_BRAND_PIXELS).ok()? {
        return None;
    }
    usize::try_from(pixel_count.checked_mul(4)?).ok()
}

/// The bundled brand mark as RGBA, or `None` when the asset cannot decode.
pub fn image() -> Option<UiImage> {
    static IMAGE: OnceLock<Option<UiImage>> = OnceLock::new();
    IMAGE
        .get_or_init(|| {
            let mut decoder = png::Decoder::new(std::io::Cursor::new(LOGO_PNG));
            decoder.set_limits(png::Limits {
                bytes: MAX_BRAND_PIXELS,
            });
            decoder
                .set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
            let mut reader = decoder.read_info().ok()?;
            let buffer_size = reader.output_buffer_size().filter(|size| {
                bounded_buffer_size(reader.info()).is_some_and(|bounded| *size <= bounded)
            })?;
            let mut buffer = vec![0; buffer_size];
            let info = reader.next_frame(&mut buffer).ok()?;
            let data = &buffer[..info.buffer_size()];
            let rgba: Vec<u8> = match info.color_type {
                png::ColorType::Rgba => {
                    buffer.truncate(info.buffer_size());
                    buffer
                }
                png::ColorType::Rgb => data
                    .chunks_exact(3)
                    .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                    .collect(),
                png::ColorType::Grayscale => data
                    .iter()
                    .flat_map(|value| [*value, *value, *value, 255])
                    .collect(),
                png::ColorType::GrayscaleAlpha => data
                    .chunks_exact(2)
                    .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
                    .collect(),
                png::ColorType::Indexed => return None,
            };
            UiImage::from_rgba(info.width, info.height, rgba)
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_brand_mark_decodes_rgba() {
        let image = image().expect("bundled brand asset decodes");
        assert_eq!(image.width(), 128);
        assert_eq!(image.height(), 128);
        assert_eq!(image.pixels().len(), 128 * 128 * 4);
        assert_eq!(image.cache_key(), super::image().unwrap().cache_key());
    }

    #[test]
    fn bounded_buffer_size_caps_dimensional_growth() {
        let mut info = png::Info::default();
        info.width = u32::MAX;
        info.height = u32::MAX;
        assert_eq!(bounded_buffer_size(&info), None);
    }
}
