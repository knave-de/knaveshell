use super::*;
use knave_ui::{Border, CornerRadii, ShapePaint, TextAlign, TextWrap};
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
                surface: Color::rgba(20, 29, 42, 255),
                card: Color::rgba(31, 43, 60, 255),
                text: Color::TEXT,
                muted: Color::rgba(178, 193, 213, 255),
                accent: Color::rgba(125, 198, 255, 255),
                border: if s.high_contrast {
                    Color::TEXT
                } else {
                    Color::rgba(75, 96, 122, 255)
                },
            }
        }
    }
    fn element(&self, id: u64, w: Widget) -> Element {
        let mut e = Element::new(id, w);
        e.style.text.color = self.text;
        e.style.text.wrap = TextWrap::WordOrGlyph;
        e.style.hover = self.card;
        e.style.pressed = self.surface;
        e.style.focus = self.accent;
        e.style.radii = Some(CornerRadii::uniform(0.0));
        e
    }
    fn text(&self, id: u64, s: impl Into<String>, muted: bool) -> Element {
        let mut e = self.element(id, Widget::Text(s.into()));
        if muted {
            e.style.text.color = self.muted
        }
        e
    }
    fn panel(&self, id: u64) -> Element {
        self.element(id, Widget::Panel)
    }
    fn button(&self, id: u64, label: impl Into<String>, enabled: bool) -> Element {
        let mut e = self.element(id, Widget::Button(label.into()));
        e.layout.padding = 8.0;
        e.layout.height = Length::Px(40.0);
        e.enabled = enabled;
        e.style.background = Some(ShapePaint {
            border: Some(Border {
                color: self.border,
                width: 1.0,
            }),
            radii: CornerRadii::uniform(0.0),
            ..ShapePaint::fill(self.card)
        });
        e
    }
    fn header_button(&self, id: u64, label: &str, width: f32) -> Element {
        let mut e = self.button(id, label, true);
        e.layout.width = Length::Px(width);
        e.layout.height = Length::Px(32.0);
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
        p.panel(id).layout(Layout {
            gap: 8.0,
            ..Default::default()
        })
    }
    fn row(&mut self, p: &Palette) -> Element {
        let mut e = self.group(p);
        e.layout.flow = Flow::Row;
        e
    }
    fn volume(&mut self, p: &Palette, input: bool) -> Element {
        let state = if input {
            self.state.input
        } else {
            self.state.output
        };
        let mut group = self.group(p);
        let title = if input { "Microphone" } else { "Output volume" };
        group.children.push(self.label(
            p,
            format!(
                "{title} · {}",
                state.map_or("Unavailable".into(), |v| format!(
                    "{}%{}",
                    v.percent,
                    if v.muted { " · Muted" } else { "" }
                ))
            ),
            false,
        ));
        let mut row = self.row(p);
        let mut mute = self.button(
            p,
            if state.is_some_and(|v| v.muted) {
                "Unmute"
            } else {
                "Mute"
            },
            Target::Command(QuickSettingsCommand::SetMute {
                input,
                muted: !state.is_some_and(|v| v.muted),
            }),
            state.is_some(),
        );
        mute.layout.width = Length::Px(72.0);
        row.children.push(mute);
        row.children.push(p.slider(
            if input { INPUT } else { OUTPUT },
            0.0,
            100.0,
            state.map_or(0.0, |v| f64::from(v.percent)),
            1.0,
            state.is_some(),
        ));
        group.children.push(row);
        group
    }
    pub(super) fn compose(&mut self) -> Element {
        let p = Palette::new(&self.state);
        self.targets.clear();
        self.next = 100;
        let gap = if self.size[0] < 360.0 { 6.0 } else { 12.0 };
        let width = (self.size[0] - 2.0 * gap).clamp(1.0, 400.0);
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
            padding: 12.0,
            gap: 8.0,
            ..Default::default()
        });
        panel.style.background = Some(ShapePaint {
            border: Some(Border {
                color: p.border,
                width: 1.0,
            }),
            radii: CornerRadii::uniform(0.0),
            ..ShapePaint::fill(p.surface)
        });
        let mut header = self.row(&p);
        if self.page != Page::Main {
            header.children.push(p.header_button(BACK, "‹", 32.0));
        }
        let title = match self.page {
            Page::Main => "Quick settings",
            Page::Network => "Network",

            Page::Night => "Night Light",
            Page::Battery => "Battery charging",
        };
        let mut title = self.label(&p, title, false);
        title.layout.height = Length::Px(32.0);
        title.layout.padding_edges = Some(Insets::symmetric(6.0, 0.0));
        header.children.push(title);
        if self.page == Page::Main {
            let mut power = p.header_button(POWER, "Power", 72.0);
            power.widget = Widget::Menu {
                label: "Power".into(),
                items: vec![
                    MenuItem::option(1, "Suspend"),
                    MenuItem::option(2, "Restart…"),
                    MenuItem::option(3, "Shut down…"),
                ],
            };
            header.children.push(power);
        }
        header.children.push(p.header_button(CLOSE, "×", 32.0));
        panel.children.push(header);
        let mut scroll = p.element(20, Widget::Scroll).layout(Layout {
            height: Length::Fill,
            ..Default::default()
        });
        let mut content = self.group(&p);
        content.layout.gap = 12.0;
        match self.page {
            Page::Main => {
                let label = self
                    .state
                    .network
                    .as_ref()
                    .map_or("Network unavailable".into(), Network::summary);
                content.children.push(self.button(
                    &p,
                    format!("Network\n{label}     ›"),
                    Target::Page(Page::Network),
                    true,
                ));
                let mut row = self.row(&p);
                row.children.push(self.button(
                    &p,
                    format!(
                        "Bluetooth\n{}",
                        self.state.bluetooth.map_or("Unavailable", |v| if v {
                            "On"
                        } else {
                            "Off"
                        })
                    ),
                    Target::Command(QuickSettingsCommand::SetBluetooth {
                        enabled: self.state.bluetooth != Some(true),
                    }),
                    self.state.bluetooth.is_some(),
                ));
                row.children.push(self.button(
                    &p,
                    format!(
                        "Night Light\n{}     ›",
                        if self.state.night.active {
                            "On"
                        } else if self.state.night.enabled {
                            "Scheduled"
                        } else {
                            "Off"
                        }
                    ),
                    Target::Page(Page::Night),
                    true,
                ));
                content.children.push(row);
                content.children.push(self.volume(&p, false));
                content.children.push(self.volume(&p, true));
                content.children.push(self.button(
                    &p,
                    "Audio devices & apps ↗  · pavucontrol",
                    Target::Command(QuickSettingsCommand::OpenAudio),
                    true,
                ));
                let mut brightness = self.group(&p);
                brightness.children.push(self.label(
                    &p,
                    format!(
                            "Brightness · {}",
                            self.state
                                .brightness
                                .map_or("Unavailable".into(), |v| format!("{v}%"))
                        ),
                    false,
                ));
                brightness.children.push(p.slider(
                    BRIGHTNESS,
                    1.0,
                    100.0,
                    self.state.brightness.map_or(1.0, f64::from),
                    1.0,
                    self.state.brightness.is_some(),
                ));
                content.children.push(brightness);
                let mut power = self.group(&p);
                power.children.push(self.label(&p, "Power profile", false));
                let mut row = self.row(&p);
                let profiles = self
                    .state
                    .power
                    .as_ref()
                    .map(|p| p.available.clone())
                    .unwrap_or_default();
                if profiles.is_empty() {
                    power
                        .children
                        .push(self.label(&p, "Power profiles unavailable", true))
                }
                for profile in profiles {
                    let active = self
                        .state
                        .power
                        .as_ref()
                        .is_some_and(|p| p.active == profile);
                    let title = match profile.as_str() {
                        "power-saver" => "Power saver",
                        "performance" => "Performance",
                        _ => "Balanced",
                    };
                    let mut b = self.button(
                        &p,
                        if active {
                            format!("• {title}")
                        } else {
                            title.into()
                        },
                        Target::Command(QuickSettingsCommand::SetPowerProfile { profile }),
                        true,
                    );
                    if active {
                        b.style.background = Some(ShapePaint::fill(p.card));
                        b.style.text.color = p.accent
                    }
                    row.children.push(b)
                }
                power.children.push(row);
                content.children.push(power);
                if let Some(b) = self.state.battery.clone() {
                    let mut battery = self.group(&p);
                    battery.children.push(self.label(
                        &p,
                        format!("Battery · {}% · {}", b.percent, b.status),
                        false,
                    ));
                    battery.children.push(self.button(
                        &p,
                        format!(
                            "Charge limit · {}     ›",
                            b.limit.map_or("Unsupported".into(), |v| format!("{v}%"))
                        ),
                        Target::Page(Page::Battery),
                        true,
                    ));
                    content.children.push(battery)
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
            radii: CornerRadii::uniform(0.0),
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
