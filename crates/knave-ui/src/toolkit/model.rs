use crate::{Color, ImageStyle, ShapePaint, TextStyle, UiImage};

pub const MAX_ELEMENTS: usize = 2048;
pub const MAX_DEPTH: usize = 48;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_MENU_ITEMS: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ElementId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    Px(f32),
    #[default]
    Fill,
    Auto,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Flow {
    Row,
    #[default]
    Column,
    Grid {
        columns: usize,
    },
    Overlay,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}
impl Insets {
    pub const fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
    pub const fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
    pub(super) fn horizontal(self) -> f32 {
        self.left + self.right
    }
    pub(super) fn vertical(self) -> f32 {
        self.top + self.bottom
    }
    pub(super) fn apply(self, r: crate::Rect) -> crate::Rect {
        crate::Rect::new(
            r.x + self.left,
            r.y + self.top,
            (r.width - self.horizontal()).max(0.0),
            (r.height - self.vertical()).max(0.0),
        )
    }
    pub(super) fn valid(self) -> bool {
        [self.top, self.right, self.bottom, self.left]
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
    }
}
/// Controls child overflow. Scroll containers always clip to their content area.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ClipMode {
    #[default]
    Content,
    Bounds,
    Visible,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub width: Length,
    pub height: Length,
    pub flow: Flow,
    pub padding: f32,
    /// Overrides uniform padding when present.
    pub padding_edges: Option<Insets>,
    pub margin: Insets,
    pub clip: ClipMode,
    pub gap: f32,
    pub align: Align,
    pub offset: [f32; 2],
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            width: Length::Fill,
            height: Length::Auto,
            flow: Flow::Column,
            padding: 0.0,
            padding_edges: None,
            margin: Insets::default(),
            clip: ClipMode::Content,
            gap: 8.0,
            align: Align::Stretch,
            offset: [0.0; 2],
        }
    }
}

impl Layout {
    pub fn with_padding(mut self, padding: Insets) -> Self {
        self.padding_edges = Some(padding);
        self
    }
    pub fn with_margin(mut self, margin: Insets) -> Self {
        self.margin = margin;
        self
    }
    pub fn insets(self) -> Insets {
        self.padding_edges.unwrap_or(Insets::all(self.padding))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub background: Option<ShapePaint>,
    pub text: TextStyle,
    pub hover: Color,
    pub pressed: Color,
    pub selected: Color,
    pub focus: Color,
    pub radius: f32,
    pub radii: Option<crate::CornerRadii>,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            background: None,
            text: TextStyle::default(),
            hover: Color::rgba(49, 64, 83, 255),
            pressed: Color::rgba(28, 40, 58, 255),
            selected: Color::rgba(42, 88, 112, 255),
            focus: Color::ACCENT,
            radius: 6.0,
            radii: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuItem {
    Option {
        id: u64,
        label: String,
        enabled: bool,
    },
    Separator,
}
impl MenuItem {
    pub fn option(id: u64, label: impl Into<String>) -> Self {
        Self::Option {
            id,
            label: label.into(),
            enabled: true,
        }
    }
    pub fn selectable(&self) -> bool {
        matches!(self, Self::Option { enabled: true, .. })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Orientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Pixels of wheel/trackpad delta per keyboard-sized increment. Disabled by default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderScroll {
    pub pixels_per_step: f64,
    pub inverted: bool,
}
impl Default for SliderScroll {
    fn default() -> Self {
        Self {
            pixels_per_step: 40.0,
            inverted: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slider {
    min: f64,
    max: f64,
    value: f64,
    step: Option<f64>,
    scroll: Option<SliderScroll>,
    scroll_remainder: f64,
    pub orientation: Orientation,
}
impl Slider {
    pub fn new(min: f64, max: f64, value: f64, step: Option<f64>) -> Result<Self, UiError> {
        if !min.is_finite()
            || !max.is_finite()
            || !(max - min).is_finite()
            || max <= min
            || !value.is_finite()
            || step.is_some_and(|s| !s.is_finite() || s <= 0.0)
        {
            return Err(UiError::InvalidRange);
        }
        let mut result = Self {
            min,
            max,
            value: min,
            step,
            scroll: None,
            scroll_remainder: 0.0,
            orientation: Orientation::Horizontal,
        };
        result.set(value);
        Ok(result)
    }
    pub fn with_scroll(mut self, options: Option<SliderScroll>) -> Result<Self, UiError> {
        if options.is_some_and(|o| !o.pixels_per_step.is_finite() || o.pixels_per_step <= 0.0) {
            return Err(UiError::InvalidRange);
        }
        self.scroll = options;
        self.scroll_remainder = 0.0;
        Ok(self)
    }
    pub(super) fn scroll_by(&mut self, delta: f32) -> Option<bool> {
        let options = self.scroll?;
        let direction = if options.inverted { 1.0 } else { -1.0 };
        let units = (f64::from(delta) * direction / options.pixels_per_step).clamp(-1e6, 1e6);
        if self.step.is_some() {
            if self.scroll_remainder.signum() != units.signum() {
                self.scroll_remainder = 0.0;
            }
            self.scroll_remainder += units;
            let whole = self.scroll_remainder.trunc();
            self.scroll_remainder -= whole;
            Some(self.increment(whole))
        } else {
            Some(self.increment(units))
        }
    }
    pub fn value(&self) -> f64 {
        self.value
    }
    pub fn fraction(&self) -> f32 {
        ((self.value - self.min) / (self.max - self.min)) as f32
    }
    pub fn set(&mut self, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        let mut next = value.clamp(self.min, self.max);
        if next != self.max
            && next != self.min
            && let Some(step) = self.step
        {
            let units = (next - self.min) / step;
            if units.is_finite() {
                next = (self.min + units.round() * step).clamp(self.min, self.max);
            }
        }
        let changed = next != self.value;
        self.value = next;
        changed
    }
    pub fn set_fraction(&mut self, fraction: f32) -> bool {
        self.set(self.min + f64::from(fraction.clamp(0.0, 1.0)) * (self.max - self.min))
    }
    pub fn increment(&mut self, direction: f64) -> bool {
        self.set(self.value + direction * self.step.unwrap_or((self.max - self.min) / 100.0))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    Panel,
    Text(String),
    Paragraph(Vec<crate::TextSpan>),
    Image(UiImage, ImageStyle),
    Button(String),
    Toggle {
        label: String,
        checked: bool,
    },
    Checkbox {
        label: String,
        checked: bool,
    },
    Slider(Slider),
    TextInput(super::TextEdit),
    Dropdown {
        label: String,
        items: Vec<MenuItem>,
        selected: Option<u64>,
    },
    Menu {
        label: String,
        items: Vec<MenuItem>,
    },
    Selectable {
        label: String,
        selected: bool,
    },
    Draggable(String),
    Separator,
    Scroll,
    Popup {
        modal: bool,
    },
}
impl Widget {
    pub(crate) fn focusable(&self) -> bool {
        matches!(
            self,
            Self::Button(_)
                | Self::Toggle { .. }
                | Self::Checkbox { .. }
                | Self::Slider(_)
                | Self::TextInput(_)
                | Self::Dropdown { .. }
                | Self::Menu { .. }
                | Self::Selectable { .. }
                | Self::Draggable(_)
        )
    }
    pub(crate) fn label(&self) -> &str {
        match self {
            Self::Text(s) | Self::Button(s) | Self::Draggable(s) => s,
            Self::Toggle { label, .. }
            | Self::Checkbox { label, .. }
            | Self::Dropdown { label, .. }
            | Self::Menu { label, .. }
            | Self::Selectable { label, .. } => label,
            Self::TextInput(edit) => edit.text(),
            _ => "",
        }
    }
    pub(crate) fn menu(&self) -> Option<&[MenuItem]> {
        match self {
            Self::Dropdown { items, .. } | Self::Menu { items, .. } => Some(items),
            _ => None,
        }
    }
}

/// Semantic pointer appearance; the platform host supplies the cursor image.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CursorShape {
    #[default]
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    Move,
    EwResize,
    NsResize,
    NwseResize,
    NeswResize,
    Crosshair,
    Wait,
    Progress,
    NotAllowed,
    Hidden,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub id: ElementId,
    pub widget: Widget,
    pub layout: Layout,
    pub style: Style,
    pub enabled: bool,
    pub visible: bool,
    pub children: Vec<Element>,
    pub tooltip: Option<String>,
    /// None selects the widget default; explicit shapes also apply to its passive children.
    pub cursor: Option<CursorShape>,
}
impl Element {
    pub fn new(id: u64, widget: Widget) -> Self {
        Self {
            id: ElementId(id),
            widget,
            layout: Layout::default(),
            style: Style::default(),
            enabled: true,
            visible: true,
            children: Vec::new(),
            tooltip: None,
            cursor: None,
        }
    }
    pub fn cursor(mut self, cursor: CursorShape) -> Self {
        self.cursor = Some(cursor);
        self
    }
    pub fn children(mut self, children: Vec<Self>) -> Self {
        self.children = children;
        self
    }
    pub fn layout(mut self, layout: Layout) -> Self {
        self.layout = layout;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiError {
    DuplicateId(ElementId),
    TooManyElements,
    TooDeep,
    InvalidLayout,
    InvalidRange,
    TextTooLong,
    TooManyMenuItems,
    MissingElement(ElementId),
}
impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UI validation failed: {self:?}")
    }
}
impl std::error::Error for UiError {}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InteractionState {
    pub hovered: bool,
    pub focused: bool,
    pub pressed: bool,
    pub captured: bool,
    pub disabled: bool,
    pub selected: bool,
    pub expanded: bool,
}

/// Glyph-derived caret positions in logical coordinates. Byte offsets are UTF-8 boundaries.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Caret {
    pub byte: usize,
    pub x: f32,
    pub y: f32,
    pub height: f32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextLayout {
    pub size: [f32; 2],
    pub carets: Vec<Caret>,
}
impl TextLayout {
    pub fn caret(&self, byte: usize) -> Option<Caret> {
        self.carets.iter().find(|c| c.byte == byte).copied()
    }
    pub fn hit(&self, x: f32) -> usize {
        self.carets
            .iter()
            .min_by(|a, b| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
            .map_or(0, |c| c.byte)
    }
}
/// Implemented by the renderer's text engine; layout and painting use the same fonts.
pub trait TextMeasurer {
    fn measure(&mut self, text: &str, style: &TextStyle, width: f32) -> TextLayout;
    fn measure_spans(
        &mut self,
        spans: &[crate::TextSpan],
        style: &TextStyle,
        width: f32,
    ) -> TextLayout;
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextInputState {
    pub id: ElementId,
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
    pub cursor_rectangle: crate::Rect,
}
