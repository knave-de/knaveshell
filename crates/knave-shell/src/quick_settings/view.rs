use super::*;
use knave_ui::{Border, CornerRadii, ShapePaint, TextAlign, TextWrap, UiImage};
use std::sync::OnceLock;
struct Palette {
    surface: Color,
    card: Color,
    text: Color,
    muted: Color,
    accent: Color,
    border: Color,
}
impl Palette {
    fn new(s: &QuickSettingsSnapshot) -> Self {
        if s.light {
            Self {
                surface: Color::rgba(244, 247, 251, 255),
                card: Color::rgba(255, 255, 255, 255),
                text: Color::rgba(18, 28, 43, 255),
                muted: Color::rgba(65, 80, 100, 255),
                accent: Color::rgba(18, 91, 158, 255),
                border: Color::rgba(120, 140, 160, 255),
            }
        } else {
            Self {
                surface: Color::rgba(32, 36, 39, 255),
                card: Color::rgba(41, 47, 51, 255),
                text: Color::TEXT,
                muted: Color::rgba(146, 155, 163, 255),
                accent: Color::rgba(149, 185, 238, 255),
                border: if s.high_contrast {
                    Color::TEXT
                } else {
                    Color::rgba(255, 255, 255, 18)
                },
            }
        }
    }
    fn element(&self, id: u64, w: Widget) -> Element {
        let mut e = Element::new(id, w);
        e.style.text.color = self.text;
        e.style.text.font_size = 12.0;
        e.style.text.line_height = 16.0;
        e.style.text.wrap = TextWrap::WordOrGlyph;
        e.style.hover = self.card;
        e.style.pressed = self.surface;
        e.style.focus = self.accent;
        e.style.radii = Some(CornerRadii::uniform(7.0));
        e
    }
    fn text(&self, id: u64, s: impl Into<String>, muted: bool) -> Element {
        let mut e = self.element(id, Widget::Text(s.into()));
        if muted {
            e.style.text.color = self.muted;
            e.style.text.font_size = 10.0;
            e.style.text.line_height = 14.0;
        }
        e
    }
    fn panel(&self, id: u64) -> Element {
        self.element(id, Widget::Panel)
    }
    fn button(&self, id: u64, label: impl Into<String>, enabled: bool) -> Element {
        let mut e = self.element(id, Widget::Button(label.into()));
        e.layout.padding = 8.0;
        e.layout.height = Length::Px(44.0);
        e.enabled = enabled;
        e.style.background = Some(ShapePaint {
            border: Some(Border {
                color: self.border,
                width: 1.0,
            }),
            radii: CornerRadii::uniform(7.0),
            ..ShapePaint::fill(self.card)
        });
        e
    }
    fn header_button(&self, id: u64, label: &str, width: f32) -> Element {
        let mut e = self.button(id, label, true);
        e.layout.width = Length::Px(width);
        e.layout.height = Length::Px(30.0);
        e.layout.padding = 0.0;
        e.layout.padding_edges = Some(Insets::all(0.0));
        e.style.text.align = TextAlign::Center;
        e
    }
    fn slider(&self, id: u64, min: f64, max: f64, value: f64, step: f64, enabled: bool) -> Element {
        let mut e = self.element(
            id,
            Widget::Slider(Slider::new(min, max, value, Some(step)).unwrap()),
        );
        e.enabled = enabled;
        e.layout.height = Length::Px(30.0);
        e
    }
}
fn clock(minute: u16) -> String {
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

fn profile_name(profile: &str) -> &str {
    match profile {
        "power-saver" => "Power saver",
        "balanced" => "Balanced",
        "performance" => "Performance",
        other => other,
    }
}

#[derive(Clone, Copy)]
enum Icon {
    Power,
    Close,
    Network,
    Bluetooth,
    Speaker,
    Microphone,
    Brightness,
    Battery,
}

fn raster_icon(icon: Icon, color: Color) -> UiImage {
    const SIZE: usize = 24;
    let mut pixels = vec![0; SIZE * SIZE * 4];
    let mut segments: Vec<([f32; 2], [f32; 2])> = Vec::new();
    let mut add_path = |points: &[[f32; 2]]| {
        segments.extend(points.windows(2).map(|w| (w[0], w[1])));
    };
    match icon {
        Icon::Power => {
            let arc: Vec<_> = (0..=28)
                .map(|i| {
                    let a = (-45.0 + (270.0 * i as f32 / 28.0)).to_radians();
                    [12.0 + 8.0 * a.cos(), 12.0 + 8.0 * a.sin()]
                })
                .collect();
            add_path(&arc);
            segments.push(([12.0, 3.0], [12.0, 12.0]));
        }
        Icon::Close => {
            segments.push(([6.0, 6.0], [18.0, 18.0]));
            segments.push(([18.0, 6.0], [6.0, 18.0]));
        }
        Icon::Network => {
            add_path(&[
                [4.0, 9.0],
                [7.0, 6.5],
                [10.0, 5.0],
                [12.0, 4.5],
                [14.0, 5.0],
                [17.0, 6.5],
                [20.0, 9.0],
            ]);
            add_path(&[
                [7.0, 12.0],
                [9.0, 10.5],
                [12.0, 9.5],
                [15.0, 10.5],
                [17.0, 12.0],
            ]);
            add_path(&[[10.0, 15.0], [12.0, 14.0], [14.0, 15.0]]);
            segments.push(([12.0, 18.0], [12.0, 18.2]));
        }
        Icon::Bluetooth => {
            add_path(&[
                [7.0, 7.0],
                [17.0, 17.0],
                [12.0, 22.0],
                [12.0, 2.0],
                [17.0, 7.0],
                [7.0, 17.0],
            ]);
        }
        Icon::Speaker => {
            add_path(&[
                [4.0, 9.0],
                [8.0, 9.0],
                [13.0, 5.0],
                [13.0, 19.0],
                [8.0, 15.0],
                [4.0, 15.0],
                [4.0, 9.0],
            ]);
            add_path(&[[16.0, 9.0], [18.0, 12.0], [16.0, 15.0]]);
            add_path(&[[18.0, 6.0], [21.0, 12.0], [18.0, 18.0]]);
        }
        Icon::Microphone => {
            add_path(&[
                [9.0, 6.0],
                [9.0, 5.0],
                [10.0, 4.0],
                [14.0, 4.0],
                [15.0, 5.0],
                [15.0, 12.0],
                [14.0, 14.0],
                [10.0, 14.0],
                [9.0, 12.0],
                [9.0, 6.0],
            ]);
            add_path(&[
                [6.0, 10.0],
                [6.0, 14.0],
                [8.0, 17.0],
                [12.0, 19.0],
                [16.0, 17.0],
                [18.0, 14.0],
                [18.0, 10.0],
            ]);
            add_path(&[[12.0, 19.0], [12.0, 22.0], [9.0, 22.0]]);
        }
        Icon::Brightness => {
            add_path(&[[12.0, 4.0], [12.0, 6.0]]);
            add_path(&[[12.0, 18.0], [12.0, 20.0]]);
            add_path(&[[4.0, 12.0], [6.0, 12.0]]);
            add_path(&[[18.0, 12.0], [20.0, 12.0]]);
            add_path(&[[6.3, 6.3], [7.8, 7.8]]);
            add_path(&[[16.2, 16.2], [17.7, 17.7]]);
            add_path(&[[17.7, 6.3], [16.2, 7.8]]);
            add_path(&[[7.8, 16.2], [6.3, 17.7]]);
            for i in 0..32 {
                let a = std::f32::consts::TAU * i as f32 / 32.0;
                segments.push((
                    [12.0 + 4.0 * a.cos(), 12.0 + 4.0 * a.sin()],
                    [12.0 + 4.0 * (a + 0.2).cos(), 12.0 + 4.0 * (a + 0.2).sin()],
                ));
            }
        }
        Icon::Battery => {
            add_path(&[
                [4.0, 7.0],
                [18.0, 7.0],
                [18.0, 9.0],
                [20.0, 9.0],
                [20.0, 15.0],
                [18.0, 15.0],
                [18.0, 17.0],
                [4.0, 17.0],
                [4.0, 7.0],
            ]);
            segments.push(([7.0, 10.0], [7.0, 14.0]));
            segments.push(([10.0, 10.0], [10.0, 14.0]));
        }
    }
    for x in 0..SIZE {
        for y in 0..SIZE {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let covered = segments.iter().any(|(a, b)| distance(point, *a, *b) <= 0.8);
            if covered {
                let i = (y * SIZE + x) * 4;
                pixels[i..i + 4].copy_from_slice(&[
                    color.red,
                    color.green,
                    color.blue,
                    color.alpha,
                ]);
            }
        }
    }
    UiImage::from_rgba(SIZE as u32, SIZE as u32, pixels).expect("icon dimensions are valid")
}

fn icon_image(icon: Icon, light: bool) -> UiImage {
    static DARK: OnceLock<[UiImage; 8]> = OnceLock::new();
    static LIGHT: OnceLock<[UiImage; 8]> = OnceLock::new();
    let cache = if light { &LIGHT } else { &DARK };
    cache.get_or_init(|| {
        let color = if light {
            Color::rgba(65, 80, 100, 255)
        } else {
            Color::rgba(185, 192, 198, 255)
        };
        [
            Icon::Power,
            Icon::Close,
            Icon::Network,
            Icon::Bluetooth,
            Icon::Speaker,
            Icon::Microphone,
            Icon::Brightness,
            Icon::Battery,
        ]
        .map(|i| raster_icon(i, color))
    })[icon as usize]
        .clone()
}

fn distance(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let delta = [b[0] - a[0], b[1] - a[1]];
    let length = delta[0] * delta[0] + delta[1] * delta[1];
    let t = if length == 0.0 {
        0.0
    } else {
        (((point[0] - a[0]) * delta[0] + (point[1] - a[1]) * delta[1]) / length).clamp(0.0, 1.0)
    };
    let dx = point[0] - (a[0] + t * delta[0]);
    let dy = point[1] - (a[1] + t * delta[1]);
    (dx * dx + dy * dy).sqrt()
}
impl QuickSettings {
    fn id(&mut self) -> u64 {
        let id = self.next;
        self.next += 1;
        id
    }
    fn label(&mut self, p: &Palette, s: impl Into<String>, muted: bool) -> Element {
        let id = self.id();
        p.text(id, s, muted)
    }
    fn button(
        &mut self,
        p: &Palette,
        s: impl Into<String>,
        target: Target,
        enabled: bool,
    ) -> Element {
        let id = self.id();
        let label = s.into();
        self.targets.insert(ElementId(id), target);
        let mut e = p.button(id, label.clone(), enabled);
        if label.contains('\n') {
            e.layout.height = Length::Px(56.0);
        }
        e
    }
    fn group(&mut self, p: &Palette) -> Element {
        let id = self.id();
        let mut group = p.panel(id).layout(Layout {
            padding: 12.0,
            gap: 8.0,
            ..Default::default()
        });
        group.style.background = Some(ShapePaint {
            border: Some(Border {
                color: p.border,
                width: 1.0,
            }),
            radii: CornerRadii::uniform(7.0),
            ..ShapePaint::fill(p.card)
        });
        group
    }
    fn stack(&mut self, p: &Palette) -> Element {
        let id = self.id();
        p.panel(id).layout(Layout {
            gap: 8.0,
            ..Default::default()
        })
    }
    fn row(&mut self, p: &Palette) -> Element {
        let mut e = self.stack(p);
        e.layout.flow = Flow::Row;
        e
    }
    fn icon(&mut self, icon: Icon) -> Element {
        Element::new(
            self.id(),
            Widget::Image(icon_image(icon, self.state.light), Default::default()),
        )
        .layout(Layout {
            width: Length::Px(18.0),
            height: Length::Px(18.0),
            ..Default::default()
        })
    }
    fn icon_control(
        &mut self,
        p: &Palette,
        mut control: Element,
        icon: Icon,
        label: &str,
    ) -> Element {
        control.layout.flow = Flow::Overlay;
        control.layout.padding = 6.0;
        control.layout.padding_edges = None;
        control.style.background = None;
        control.tooltip = Some(label.into());
        control.style.text.color = p.text;
        control.children.push(self.icon(icon));
        control
    }
    fn separator(&mut self, p: &Palette) -> Element {
        let mut e = p.panel(self.id()).layout(Layout {
            height: Length::Px(1.0),
            ..Default::default()
        });
        e.style.background = Some(ShapePaint::fill(p.border));
        e
    }
    fn tile(
        &mut self,
        p: &Palette,
        icon: Icon,
        title: &str,
        subtitle: String,
        target: Target,
        state: (bool, bool),
    ) -> Element {
        let (enabled, active) = state;
        let mut b = self.button(p, "", target, enabled);
        b.tooltip = Some(format!("{title}: {subtitle}"));
        b.layout.flow = Flow::Row;
        b.layout.gap = 8.0;
        b.layout.height = Length::Px(64.0);
        b.children.push(self.icon(icon));
        let mut copy = self.stack(p);
        copy.layout.gap = 2.0;
        copy.children.push(self.label(p, title, false));
        let mut sub = self.label(p, subtitle, true);
        sub.style.text.wrap = TextWrap::None;
        sub.layout.clip = ClipMode::Bounds;
        copy.children.push(sub);
        b.children.push(copy);
        if active {
            b.style.background = Some(ShapePaint {
                border: Some(Border {
                    color: p.accent,
                    width: 1.0,
                }),
                radii: CornerRadii::uniform(7.0),
                ..ShapePaint::fill(if self.state.light {
                    Color::rgba(220, 232, 248, 255)
                } else {
                    Color::rgba(49, 62, 77, 255)
                })
            });
        }
        b
    }
    fn volume(&mut self, p: &Palette, input: bool) -> Element {
        let state = if input {
            self.state.input
        } else {
            self.state.output
        };
        let mut group = self.stack(p);
        group.layout.gap = 4.0;
        let title = if input { "Microphone" } else { "Output volume" };
        let mute_label = if state.is_some_and(|s| s.muted) {
            format!("Unmute {title}")
        } else {
            format!("Mute {title}")
        };
        let mut heading = self.row(p);
        let mut mute = self.button(
            p,
            "",
            Target::Command(QuickSettingsCommand::SetMute {
                input,
                muted: !state.is_some_and(|v| v.muted),
            }),
            state.is_some(),
        );
        mute.layout.width = Length::Px(30.0);
        mute.layout.height = Length::Px(30.0);
        let mute = self.icon_control(
            p,
            mute,
            if input {
                Icon::Microphone
            } else {
                Icon::Speaker
            },
            &mute_label,
        );
        heading.children.push(mute);
        let mut name = self.label(p, title, false);
        name.layout.height = Length::Px(30.0);
        name.layout.padding_edges = Some(Insets::symmetric(7.0, 0.0));
        heading.children.push(name);
        let mut value = self.label(
            p,
            state.map_or("Unavailable".into(), |v| {
                format!("{}%{}", v.percent, if v.muted { " · Muted" } else { "" })
            }),
            true,
        );
        value.layout.width = Length::Px(90.0);
        value.layout.height = Length::Px(30.0);
        value.layout.padding_edges = Some(Insets::symmetric(8.0, 0.0));
        value.style.text.align = TextAlign::Right;
        heading.children.push(value);
        group.children.push(heading);
        group.children.push(p.slider(
            if input { INPUT } else { OUTPUT },
            0.0,
            100.0,
            state.map_or(0.0, |v| f64::from(v.percent)),
            1.0,
            state.is_some(),
        ));
        group
    }
    pub(super) fn compose(&mut self) -> Element {
        let p = Palette::new(&self.state);
        self.targets.clear();
        self.next = 100;
        let gap = if self.size[0] < 360.0 { 8.0 } else { 12.0 };
        let width = (self.size[0] - 2.0 * gap).clamp(1.0, 340.0);
        let top = if self.size[1] > 160.0 { 44.0 } else { 0.0 };
        let mut root = p.panel(0).layout(Layout {
            flow: Flow::Overlay,
            height: Length::Fill,
            ..Default::default()
        });
        let mut panel = p.panel(PANEL.0).layout(Layout {
            width: Length::Px(width),
            height: Length::Px((self.size[1] - top - gap).max(1.0)),
            offset: [(self.size[0] - width - gap).max(0.0), top],
            padding: 14.0,
            gap: 10.0,
            ..Default::default()
        });
        panel.style.background = Some(ShapePaint {
            border: Some(Border {
                color: p.border,
                width: 1.0,
            }),
            radii: CornerRadii::uniform(7.0),
            ..ShapePaint::fill(p.surface)
        });
        let mut header = self.row(&p);
        if self.page != Page::Main {
            header.children.push(p.header_button(BACK, "‹", 30.0));
        }
        let title = match self.page {
            Page::Main => "Quick Settings",
            Page::Network => "Network",

            Page::Night => "Night Light",
            Page::Battery => "Battery charging",
        };
        let mut title = self.label(&p, title, false);
        title.layout.height = Length::Px(30.0);
        title.layout.padding_edges = Some(Insets::symmetric(6.0, 0.0));
        title.style.text.font_size = 17.0;
        title.style.text.line_height = 22.0;
        header.children.push(title);
        if self.page == Page::Main {
            let mut power = p.header_button(POWER, "", 30.0);
            power.widget = Widget::Menu {
                label: "⏻".into(),
                items: vec![
                    MenuItem::option(1, "Suspend"),
                    MenuItem::option(2, "Restart…"),
                    MenuItem::option(3, "Shut down…"),
                ],
            };
            let power = self.icon_control(&p, power, Icon::Power, "Power menu");
            header.children.push(power);
        }
        let close = p.header_button(CLOSE, "", 30.0);
        header
            .children
            .push(self.icon_control(&p, close, Icon::Close, "Close Quick Settings"));
        panel.children.push(header);
        let mut scroll = p.element(20, Widget::Scroll).layout(Layout {
            height: Length::Fill,
            ..Default::default()
        });
        let mut content = self.stack(&p);
        content.layout.gap = 9.0;
        match self.page {
            Page::Main => {
                let mut summary = self.row(&p);
                summary.children.push(self.icon(Icon::Battery));
                let mut battery = self.stack(&p);
                battery.layout.gap = 2.0;
                if let Some(b) = self.state.battery.clone() {
                    battery.children.push(self.label(
                        &p,
                        format!("{}% · {}", b.percent, b.status),
                        false,
                    ));
                } else {
                    battery
                        .children
                        .push(self.label(&p, "No battery detected", true));
                }
                summary.children.push(battery);
                let profiles = self
                    .state
                    .power
                    .as_ref()
                    .map(|s| s.available.clone())
                    .unwrap_or_default();
                let mut profile = p.button(
                    PROFILE,
                    self.state
                        .power
                        .as_ref()
                        .map_or("Unavailable", |s| profile_name(&s.active)),
                    !profiles.is_empty(),
                );
                profile.layout.width = Length::Px(112.0);
                profile.layout.height = Length::Px(32.0);
                profile.style.text.font_size = 10.0;
                profile.tooltip = Some("Power mode".into());
                profile.widget = Widget::Menu {
                    label: self
                        .state
                        .power
                        .as_ref()
                        .map_or("Unavailable", |s| profile_name(&s.active))
                        .into(),
                    items: profiles
                        .iter()
                        .enumerate()
                        .map(|(i, s)| MenuItem::option(i as u64 + 1, profile_name(s)))
                        .collect(),
                };
                summary.children.push(profile);
                content.children.push(summary);
                let mut tiles = self.row(&p);
                let network = self
                    .state
                    .network
                    .as_ref()
                    .map_or("Unavailable".into(), Network::summary);
                let network_active = self
                    .state
                    .network
                    .as_ref()
                    .is_some_and(|n| n.ethernet.is_some() || n.wireless.is_some());
                tiles.children.push(self.tile(
                    &p,
                    Icon::Network,
                    "Network",
                    network,
                    Target::Page(Page::Network),
                    (true, network_active),
                ));
                tiles.children.push(
                    self.tile(
                        &p,
                        Icon::Bluetooth,
                        "Bluetooth",
                        self.state
                            .bluetooth
                            .map_or("Unavailable", |v| if v { "On" } else { "Off" })
                            .into(),
                        Target::Command(QuickSettingsCommand::SetBluetooth {
                            enabled: self.state.bluetooth != Some(true),
                        }),
                        (
                            self.state.bluetooth.is_some(),
                            self.state.bluetooth == Some(true),
                        ),
                    ),
                );
                content.children.push(tiles);
                let mut audio = self.group(&p);
                audio.children.push(self.volume(&p, false));
                audio.children.push(self.separator(&p));
                audio.children.push(self.volume(&p, true));
                audio.children.push(self.separator(&p));
                let mut devices = self.button(
                    &p,
                    "Audio devices & apps  ↗",
                    Target::Command(QuickSettingsCommand::OpenAudio),
                    true,
                );
                devices.style.background = None;
                devices.layout.height = Length::Px(32.0);
                audio.children.push(devices);
                content.children.push(audio);
                let mut display = self.group(&p);
                let mut brightness = self.row(&p);
                brightness.children.push(self.icon(Icon::Brightness));
                brightness
                    .children
                    .push(self.label(&p, "Brightness", false));
                let mut value = self.label(
                    &p,
                    self.state
                        .brightness
                        .map_or("Unavailable".into(), |v| format!("{v}%")),
                    true,
                );
                value.layout.width = Length::Px(80.0);
                value.style.text.align = TextAlign::Right;
                brightness.children.push(value);
                display.children.push(brightness);
                display.children.push(p.slider(
                    BRIGHTNESS,
                    1.0,
                    100.0,
                    self.state.brightness.map_or(1.0, f64::from),
                    1.0,
                    self.state.brightness.is_some(),
                ));
                display.children.push(self.separator(&p));
                let state = if self.state.night.active {
                    "On"
                } else if self.state.night.enabled {
                    "Scheduled"
                } else {
                    "Off"
                };
                let mut night = self.button(
                    &p,
                    format!("Night Light · {state}    ›"),
                    Target::Page(Page::Night),
                    true,
                );
                night.style.background = None;
                night.layout.height = Length::Px(34.0);
                display.children.push(night);
                content.children.push(display);
                if let Some(b) = self.state.battery.clone() {
                    content.children.push(self.button(
                        &p,
                        format!(
                            "Charge limit · {}    ›",
                            b.limit.map_or("Unsupported".into(), |v| format!("{v}%"))
                        ),
                        Target::Page(Page::Battery),
                        true,
                    ));
                }
            }
            Page::Network => {
                content.children.push(self.button(
                    &p,
                    "Manage networks ↗",
                    Target::Command(QuickSettingsCommand::OpenNetwork),
                    true,
                ));
                if let Some(n) = self.state.network.clone() {
                    content.children.push(self.label(&p, n.summary(), false));
                    if n.ethernet.is_some() {
                        content.children.push(self.button(
                            &p,
                            "Disconnect Ethernet",
                            Target::Command(QuickSettingsCommand::DisconnectEthernet),
                            true,
                        ))
                    }
                    content.children.push(self.button(
                        &p,
                        if n.wifi {
                            "Wi-Fi · On"
                        } else {
                            "Wi-Fi · Off"
                        },
                        Target::Command(QuickSettingsCommand::SetWifi { enabled: !n.wifi }),
                        true,
                    ));
                    content
                        .children
                        .push(self.label(&p, "Saved connections", false));
                    for c in n.connections.into_iter().take(64) {
                        content.children.push(self.button(
                            &p,
                            format!(
                                "{} · {}",
                                c.name,
                                if c.wired { "Ethernet" } else { "Wi-Fi" }
                            ),
                            Target::Command(QuickSettingsCommand::Connect { uuid: c.uuid }),
                            true,
                        ))
                    }
                } else {
                    content
                        .children
                        .push(self.label(&p, "NetworkManager is unavailable.", true))
                }
            }
            Page::Night => {
                content.children.push(self.button(
                    &p,
                    if self.night.enabled {
                        "Night Light · On"
                    } else {
                        "Night Light · Off"
                    },
                    Target::NightToggle,
                    true,
                ));
                content.children.push(self.label(
                    &p,
                    format!("Color temperature · {} K", self.night.temperature),
                    false,
                ));
                content.children.push(p.slider(
                    TEMPERATURE,
                    2500.0,
                    6500.0,
                    f64::from(self.night.temperature),
                    100.0,
                    true,
                ));
                content.children.push(self.button(
                    &p,
                    if self.night.start.is_some() {
                        "Schedule · Custom hours"
                    } else {
                        "Schedule · Manual"
                    },
                    Target::Schedule,
                    true,
                ));
                if let (Some(start), Some(end)) = (self.night.start, self.night.end) {
                    for (id, label, v) in [(START, "From", start), (END, "Until", end)] {
                        content.children.push(self.label(
                            &p,
                            format!("{label} · {}", clock(v)),
                            false,
                        ));
                        content
                            .children
                            .push(p.slider(id, 0.0, 1439.0, f64::from(v), 15.0, true))
                    }
                }
            }
            Page::Battery => {
                if let Some(b) = self.state.battery.clone() {
                    content.children.push(self.label(
                        &p,
                        format!("Battery · {}% · {}", b.percent, b.status),
                        false,
                    ));
                    content.children.push(self.label(
                        &p,
                        format!("Charge limit · {}%", self.limit),
                        false,
                    ));
                    content.children.push(p.slider(
                        LIMIT,
                        50.0,
                        100.0,
                        f64::from(self.limit),
                        5.0,
                        b.limit.is_some(),
                    ));
                    content.children.push(self.label(
                        &p,
                        "Applying a limit may request administrator authentication.",
                        true,
                    ));
                    content.children.push(self.button(
                        &p,
                        format!("Apply {}% limit", self.limit),
                        Target::ApplyLimit,
                        b.limit.is_some(),
                    ));
                    if b.limit.is_none() {
                        content.children.push(self.label(
                            &p,
                            "This battery does not expose a charge threshold.",
                            true,
                        ))
                    }
                }
            }
        }

        if let Some(error) = self.error.clone() {
            content
                .children
                .push(self.label(&p, error.chars().take(250).collect::<String>(), true))
        }
        if !self.state.errors.is_empty() {
            content.children.push(
                self.label(
                    &p,
                    self.state
                        .errors
                        .join("\n")
                        .chars()
                        .take(600)
                        .collect::<String>(),
                    true,
                ),
            )
        }
        scroll.children.push(content);
        panel.children.push(scroll);
        root.children.push(panel);
        let mut dialog = p
            .element(CONFIRMATION, Widget::Popup { modal: true })
            .layout(Layout {
                width: Length::Px((self.size[0] - 24.0).clamp(1.0, 340.0)),
                padding: 16.0,
                gap: 12.0,
                ..Default::default()
            });
        dialog.visible = self.confirmation.is_some();
        dialog.style.background = Some(ShapePaint {
            border: Some(Border {
                color: p.border,
                width: 1.0,
            }),
            radii: CornerRadii::uniform(7.0),
            ..ShapePaint::fill(p.surface)
        });
        if let Some(command) = self.confirmation.clone() {
            let (title, body, action) = match command {
                QuickSettingsCommand::Suspend => {
                    ("Suspend?", "Put the computer to sleep?", "Suspend")
                }
                QuickSettingsCommand::Reboot => {
                    ("Restart?", "Unsaved work may be lost.", "Restart")
                }
                _ => ("Shut down?", "Unsaved work may be lost.", "Shut down"),
            };
            dialog.children.push(self.label(&p, title, false));
            dialog.children.push(self.label(&p, body, true));
            let mut actions = self.row(&p);
            actions
                .children
                .push(p.button(CANCEL_CONFIRMATION, "Cancel", true));
            let mut confirm = p.button(CONFIRM_ACTION, action, true);
            confirm.style.text.align = TextAlign::Center;
            actions.children.push(confirm);
            dialog.children.push(actions);
        }
        root.children.push(dialog);
        root
    }
}
