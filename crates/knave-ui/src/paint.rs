/// An 8-bit sRGB color, with straight alpha.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const BACKGROUND: Self = Self::rgba(21, 29, 40, 255);
    pub const ACCENT: Self = Self::rgba(93, 173, 226, 255);
    pub const TEXT: Self = Self::rgba(240, 244, 248, 255);

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    /// Convert sRGB channels to linear-light RGBA for GPU rendering.
    pub fn to_linear_rgba(self) -> [f32; 4] {
        [
            srgb_to_linear(self.red),
            srgb_to_linear(self.green),
            srgb_to_linear(self.blue),
            f32::from(self.alpha) / 255.0,
        ]
    }
}

// Decode sRGB so GPU blending and shader math use linear-light values.
fn srgb_to_linear(channel: u8) -> f32 {
    let encoded = f32::from(channel) / 255.0;
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub color: Color,
    pub width: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornerRadii {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadii {
    pub const fn uniform(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxShadow {
    pub color: Color,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur_radius: f32,
    pub spread: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShapePaint {
    pub fill: Option<Color>,
    pub border: Option<Border>,
    pub radii: CornerRadii,
    pub opacity: f32,
    pub shadow: Option<BoxShadow>,
}

impl ShapePaint {
    pub fn fill(color: Color) -> Self {
        Self {
            fill: Some(color),
            border: None,
            radii: CornerRadii::default(),
            opacity: 1.0,
            shadow: None,
        }
    }
}

impl Default for ShapePaint {
    fn default() -> Self {
        Self::fill(Color::BACKGROUND)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextWrap {
    None,
    #[default]
    Word,
    WordOrGlyph,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub color: Color,
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub weight: u16,
    pub wrap: TextWrap,
    pub align: TextAlign,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: Color::TEXT,
            font_family: "sans-serif".into(),
            font_size: 14.0,
            line_height: 18.0,
            weight: 400,
            wrap: TextWrap::Word,
            align: TextAlign::Start,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImageFit {
    #[default]
    Stretch,
    Contain,
    Cover,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageStyle {
    pub fit: ImageFit,
    pub opacity: f32,
    pub corner_radius: f32,
}

impl Default for ImageStyle {
    fn default() -> Self {
        Self {
            fit: ImageFit::Stretch,
            opacity: 1.0,
            corner_radius: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_style_uses_logical_sizes_and_real_family_name() {
        let style = TextStyle::default();
        assert_eq!(style.font_family, "sans-serif");
        assert_eq!(style.font_size, 14.0);
        assert_eq!(style.line_height, 18.0);
        assert_eq!(style.wrap, TextWrap::Word);
    }

    #[test]
    fn shape_and_image_styles_have_explicit_defaults() {
        let shape = ShapePaint::fill(Color::ACCENT);
        assert_eq!(shape.fill, Some(Color::ACCENT));
        assert_eq!(shape.opacity, 1.0);
        assert_eq!(shape.radii, CornerRadii::default());
        assert_eq!(ImageStyle::default().fit, ImageFit::Stretch);
    }

    #[test]
    fn color_converts_srgb_rgb_to_linear_but_keeps_alpha_linear() {
        let channels = Color::rgba(10, 128, 255, 128).to_linear_rgba();
        assert!((channels[0] - 0.003_035).abs() < 0.000_001);
        assert!((channels[1] - 0.215_861).abs() < 0.000_01);
        assert_eq!(channels[2], 1.0);
        assert!((channels[3] - 128.0 / 255.0).abs() < 0.000_001);
    }
}

/// A styled inline run; the paragraph supplies wrapping and alignment.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TextSpan {
    pub text: String,
    pub color: Color,
    pub weight: u16,
}
