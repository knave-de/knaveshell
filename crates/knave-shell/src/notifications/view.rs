use super::*;
use knave_ui::{Border, CornerRadii, ShapePaint, TextAlign, TextWrap};

pub(super) fn card_id(id: u64) -> u64 {
    1000 + id * 32
}
struct Palette {
    surface: Color,
    card: Color,
    text: Color,
    muted: Color,
    accent: Color,
    border: Color,
    critical: Color,
}
impl Palette {
    fn new(light: bool, contrast: bool) -> Self {
        if light {
            Self {
                surface: Color::rgba(244, 247, 251, 255),
                card: Color::rgba(255, 255, 255, 255),
                text: Color::rgba(18, 28, 43, 255),
                muted: Color::rgba(65, 80, 100, 255),
                accent: Color::rgba(18, 91, 158, 255),
                border: if contrast {
                    Color::rgba(18, 28, 43, 255)
                } else {
                    Color::rgba(180, 193, 210, 255)
                },
                critical: Color::rgba(170, 35, 49, 255),
            }
        } else {
            Self {
                surface: Color::rgba(20, 29, 42, 255),
                card: Color::rgba(31, 43, 60, 255),
                text: Color::TEXT,
                muted: Color::rgba(178, 193, 213, 255),
                accent: Color::rgba(125, 198, 255, 255),
                border: if contrast {
                    Color::TEXT
                } else {
                    Color::rgba(75, 96, 122, 255)
                },
                critical: Color::rgba(255, 157, 162, 255),
            }
        }
    }
    fn style(&self, e: &mut Element) {
        e.style.text.color = self.text;
        e.style.text.wrap = TextWrap::WordOrGlyph;
        e.style.hover = self.surface;
        e.style.pressed = self.card;
        e.style.focus = self.accent;
        e.style.radii = Some(CornerRadii::uniform(0.0));
    }
    fn panel(&self, id: u64) -> Element {
        let mut e = Element::new(id, Widget::Panel);
        self.style(&mut e);
        e
    }
    fn text(&self, id: u64, value: String, muted: bool) -> Element {
        let mut e = Element::new(id, Widget::Text(value));
        self.style(&mut e);
        e.style.text.color = if muted { self.muted } else { self.text };
        e
    }
    fn close_button(&self, id: u64, keyboard_focus: bool) -> Element {
        let mut e = Element::new(id, Widget::Button("×".into())).layout(Layout {
            width: Length::Px(32.0),
            height: Length::Px(32.0),
            ..Default::default()
        });
        self.style(&mut e);
        e.style.text.align = TextAlign::Center;
        e.style.hover = Color::rgba(128, 128, 128, 40);
        e.style.pressed = Color::rgba(128, 128, 128, 70);
        e.style.focus = if keyboard_focus {
            self.muted
        } else {
            Color::rgba(0, 0, 0, 0)
        };
        e.style.background = Some(ShapePaint {
            radii: CornerRadii::uniform(0.0),
            ..ShapePaint::fill(Color::rgba(0, 0, 0, 0))
        });
        e
    }
    fn page_button(&self, id: u64, value: &str) -> Element {
        let mut e = self.button(id, value);
        e.layout.width = Length::Px(36.0);
        e.layout.height = Length::Px(36.0);
        e.layout.padding = 0.0;
        e.style.text.align = TextAlign::Center;
        e
    }
    fn button(&self, id: u64, value: &str) -> Element {
        let mut e = Element::new(id, Widget::Button(value.into())).layout(Layout {
            padding: 8.0,
            ..Default::default()
        });
        self.style(&mut e);
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
}
fn short(s: &str, max: usize) -> String {
    let mut result: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        result.push('…');
    }
    result
}
fn panel_geometry(width: f32) -> (f32, f32) {
    let gap = if width < 360.0 { 6.0 } else { 12.0 };
    (gap, (width - gap * 2.0).clamp(1.0, 400.0))
}
impl Notifications {
    pub(super) fn compose(&mut self) -> Element {
        let p = Palette::new(self.state.light, self.state.high_contrast);
        self.targets.clear();
        let width = self.size[0].max(1.0);
        let height = self.size[1].max(1.0);
        let (gap, panel_width) = panel_geometry(width);
        let top = if height > 160.0 { 44.0 } else { 0.0 };
        let mut root = p.panel(0);
        root.layout.flow = Flow::Overlay;
        root.layout.height = Length::Fill;
        let mut panel = p.panel(PANEL.0).layout(Layout {
            width: Length::Px(panel_width),
            height: if self.state.center_open {
                Length::Px((height - top - gap).max(1.0))
            } else {
                Length::Auto
            },
            offset: [(width - panel_width - gap).max(0.0), top],
            padding: 12.0,
            gap: 8.0,
            ..Default::default()
        });
        if self.state.center_open {
            panel.style.background = Some(ShapePaint {
                border: Some(Border {
                    color: p.border,
                    width: 1.0,
                }),
                radii: CornerRadii::uniform(0.0),
                ..ShapePaint::fill(p.surface)
            });
            let mut heading = p.panel(10).layout(Layout {
                flow: Flow::Row,
                height: Length::Auto,
                gap: 8.0,
                ..Default::default()
            });
            let mut title = p.text(
                11,
                format!("Notifications · {}", self.state.history().count()),
                false,
            );
            title.layout.height = Length::Fill;
            title.style.text.weight = 600;
            heading.children.push(title);
            let mut close = p.close_button(CLOSE, self.keyboard_focus_visible);
            close.tooltip = Some("Close notification center".into());
            heading.children.push(close);
            panel.children.push(heading);
            let mut dnd = p.button(
                DND,
                if self.state.dnd {
                    "Do not disturb: On"
                } else {
                    "Do not disturb: Off"
                },
            );
            dnd.enabled = self.state.owned;
            dnd.tooltip = Some("Hide popups from all apps. Notifications remain in history; turning this off does not replay them.".into());
            panel.children.push(dnd);
            let mut controls = p.panel(12).layout(Layout {
                flow: if panel_width < 290.0 {
                    Flow::Column
                } else {
                    Flow::Row
                },
                ..Default::default()
            });
            for (id, label) in [(READ, "Mark all read"), (CLEAR, "Clear all")] {
                let mut b = p.button(id, label);
                b.enabled = self.state.history().next().is_some();
                controls.children.push(b);
            }
            panel.children.push(controls);
            if !self.state.owned {
                panel.children.push(p.text(13,
                    "An external service delivers new notifications; saved Knave history is below.".into(),
                    true,
                ));
            }
            if let Some(error) = &self.error {
                panel.children.push(p.text(14, short(error, 180), true));
            }
            if let Some(error) = &self.state.error {
                panel.children.push(p.text(15, short(error, 180), true));
            }
            let mut scroll = Element::new(SCROLL, Widget::Scroll).layout(Layout {
                height: Length::Fill,
                ..Default::default()
            });
            let compact_controls = if height < 480.0 {
                panel.children.split_off(1)
            } else {
                Vec::new()
            };
            let mut entries = p.panel(16).layout(Layout {
                gap: 12.0,
                ..Default::default()
            });
            entries.children.extend(compact_controls);
            let notifications: Vec<_> = self
                .state
                .history()
                .skip(self.page * PAGE)
                .take(PAGE)
                .cloned()
                .collect();
            if notifications.is_empty() {
                entries
                    .children
                    .push(p.text(17, "You're all caught up".into(), true));
            }
            let mut groups: Vec<(String, Vec<Notification>)> = Vec::new();
            for n in notifications {
                if let Some((_, entries)) = groups.iter_mut().find(|(app, _)| *app == n.app_id) {
                    entries.push(n)
                } else {
                    groups.push((n.app_id.clone(), vec![n]))
                }
            }
            for (_, group) in groups {
                for n in group {
                    entries.children.push(self.card(&n, &p, false));
                }
            }
            scroll.children.push(entries);
            panel.children.push(scroll);
            let mut footer = p.panel(18).layout(Layout {
                flow: Flow::Row,
                gap: 6.0,
                ..Default::default()
            });
            let mut previous = p.page_button(PREVIOUS, "‹");
            previous.enabled = self.page > 0;
            previous.tooltip = Some("Previous notifications".into());
            footer.children.push(previous);
            let pages = self.state.history().count().div_ceil(PAGE).max(1);
            let mut page_label = p.text(
                19,
                format!(
                    "{} / {} · {} days",
                    self.page + 1,
                    pages,
                    self.state.history_days
                ),
                true,
            );
            page_label.layout.height = Length::Fill;
            footer.children.push(page_label);
            let mut next = p.page_button(NEXT, "›");
            next.enabled = self.page + 1 < pages;
            next.tooltip = Some("Older notifications".into());
            footer.children.push(next);
            panel.children.push(footer);
        } else {
            panel.layout.padding = 0.0;
            panel.layout.height = Length::Px((height - top - gap).max(0.0));
            let notifications: Vec<_> = self
                .state
                .notifications
                .iter()
                .filter(|n| n.banner)
                .take(3)
                .cloned()
                .collect();
            let mut scroll = Element::new(SCROLL, Widget::Scroll).layout(Layout {
                height: Length::Fill,
                ..Default::default()
            });
            let mut stack = p.panel(16);
            for n in notifications {
                stack.children.push(self.card(&n, &p, true));
            }
            scroll.children.push(stack);
            panel.children.push(scroll);
        }
        root.children.push(panel);
        root
    }
    fn card(&mut self, n: &Notification, p: &Palette, banner: bool) -> Element {
        let id = card_id(n.id);
        let mut card = p.panel(id).layout(Layout {
            padding: 12.0,
            gap: 7.0,
            ..Default::default()
        });
        card.style.background = Some(ShapePaint {
            border: Some(Border {
                color: if n.urgency == 2 {
                    p.critical
                } else if n.unread {
                    p.accent
                } else {
                    p.border
                },
                width: if n.unread || n.urgency == 2 { 2.0 } else { 1.0 },
            }),
            radii: CornerRadii::uniform(0.0),
            ..ShapePaint::fill(p.card)
        });
        let mut head = p.panel(id + 1).layout(Layout {
            flow: Flow::Row,
            gap: 8.0,
            ..Default::default()
        });
        let icon = if !n.icon.is_empty() {
            n.icon.as_str()
        } else if !n.app_id.is_empty() {
            n.app_id.strip_suffix(".desktop").unwrap_or(&n.app_id)
        } else if !n.app.is_empty() {
            n.app.as_str()
        } else {
            crate::overview::icons::FALLBACK
        };
        if self.icons.needs_request(icon)
            && let Some(loader) = &self.loader
            && loader.submit(Job::Icon(icon.into()))
        {
            self.icons.requested(icon);
        }
        if let Some(image) = self.icons.get(icon) {
            head.children.push(
                Element::new(id + 2, Widget::Image(image.clone(), Default::default())).layout(
                    Layout {
                        width: Length::Px(24.0),
                        height: Length::Px(24.0),
                        ..Default::default()
                    },
                ),
            );
        }
        let age = crate::notifications::age(n.created_ms);
        head.children.push(p.text(
            id + 3,
            format!(
                "{} · {}{}",
                if n.app.is_empty() {
                    "Application"
                } else {
                    &n.app
                },
                age,
                if n.unread { " · Unread" } else { "" }
            ),
            true,
        ));
        let mut dismiss = p.close_button(id + 4, self.keyboard_focus_visible);
        dismiss.tooltip = Some("Dismiss notification".into());
        self.targets.insert(dismiss.id, Target::Dismiss(n.id));
        head.children.push(dismiss);
        card.children.push(head);
        let summary = short(&n.summary, if banner { 150 } else { 512 });
        let mut title = p.text(id + 5, summary, false);
        title.style.text.weight = 600;
        card.children.push(title);
        let expanded = self.expanded.contains(&n.id);
        let body = short(
            &n.body,
            if expanded {
                4096
            } else if banner {
                180
            } else {
                360
            },
        );
        if !body.is_empty() {
            card.children.push(p.text(id + 6, body, false));
        }
        if let Some(source) = &n.image {
            if !self.previews.contains_key(&n.id)
                && let Some(image) = knave_ui::UiImage::from_rgba(
                    source.width(),
                    source.height(),
                    source.pixels().to_vec(),
                )
            {
                self.previews.insert(n.id, (source.clone(), image));
            }
            if let Some((_, image)) = self.previews.get(&n.id) {
                let (_, panel_width) = panel_geometry(self.size[0]);
                let available = (panel_width - if banner { 24.0 } else { 48.0 }).max(1.0);
                let height = (available * source.height() as f32 / source.width() as f32)
                    .min(if banner { 180.0 } else { 220.0 });
                card.children.push(
                    Element::new(
                        id + 11,
                        Widget::Image(
                            image.clone(),
                            knave_ui::ImageStyle {
                                fit: knave_ui::ImageFit::Contain,
                                ..Default::default()
                            },
                        ),
                    )
                    .layout(Layout {
                        height: Length::Px(height),
                        ..Default::default()
                    }),
                );
            }
        }
        if !banner && n.body.chars().count() > 360 {
            let more = p.button(id + 7, if expanded { "Show less" } else { "Show more" });
            self.targets.insert(more.id, Target::Expand(n.id));
            card.children.push(more);
        }
        if let Some(value) = n.progress {
            card.children
                .push(p.text(id + 8, format!("Progress: {value}%"), true));
            let mut track = p.panel(id + 9).layout(Layout {
                height: Length::Px(4.0),
                flow: Flow::Overlay,
                ..Default::default()
            });
            track.style.background = Some(ShapePaint::fill(p.border));
            // Percentage is also printed, so it remains clear without relying on color.
            let (_, panel_width) = panel_geometry(self.size[0]);
            let available = (panel_width - if banner { 24.0 } else { 48.0 }).max(0.0);
            let mut fill = p.panel(id + 10).layout(Layout {
                width: Length::Px(available * f32::from(value) / 100.0),
                height: Length::Fill,
                ..Default::default()
            });
            fill.style.background = Some(ShapePaint::fill(p.accent));
            track.children.push(fill);
            card.children.push(track);
        }
        if n.live {
            for (index, action) in n.actions.iter().take(MAX_ACTIONS).enumerate() {
                let mut button = p.button(id + 12 + index as u64, &short(&action.label, 100));
                button.tooltip = Some(action.label.clone());
                self.targets
                    .insert(button.id, Target::Invoke(n.id, action.key.clone()));
                button.layout.padding = 8.0;
                card.children.push(button);
            }
        }
        if !banner && !n.app_id.is_empty() && self.state.owned {
            let enabled = *self.state.app_banners.get(&n.app_id).unwrap_or(&true);
            let mut button = p.button(
                id + 21,
                if enabled {
                    "Hide this app’s popups"
                } else {
                    "Show this app’s popups"
                },
            );
            button.tooltip = Some("Controls popups from this app only. Notifications remain in history and the unread count; missed popups are not replayed.".into());
            self.targets
                .insert(button.id, Target::AppBanners(n.app_id.clone(), !enabled));
            card.children.push(button);
        }
        card
    }
}
