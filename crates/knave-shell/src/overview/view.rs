use super::*;
use knave_ui::{
    Border, Color, CornerRadii, ImageFit, ImageStyle, Rect, ShapePaint, TextAlign, TextWrap,
    UiImage,
};

const INK: Color = Color::rgba(17, 23, 33, 255);
const PANEL: Color = Color::rgba(32, 42, 56, 255);
const MUTED: Color = Color::rgba(165, 180, 200, 255);
const BLUE: Color = Color::rgba(139, 184, 255, 255);

fn placed(mut e: Element, r: Rect) -> Element {
    e.layout = Layout {
        width: Length::Px(r.width.max(0.0)),
        height: Length::Px(r.height.max(0.0)),
        flow: Flow::Overlay,
        offset: [r.x, r.y],
        gap: 0.0,
        ..Default::default()
    };
    e
}
fn paint(fill: Color, radius: f32, border: Option<Color>) -> ShapePaint {
    ShapePaint {
        radii: CornerRadii::uniform(radius),
        border: border.map(|color| Border { color, width: 1.0 }),
        ..ShapePaint::fill(fill)
    }
}
fn panel(id: u64, rect: Rect, fill: Color, radius: f32) -> Element {
    let mut e = placed(Element::new(id, Widget::Panel), rect);
    e.style.background = Some(paint(fill, radius, None));
    e
}
fn text(id: u64, value: impl Into<String>, r: Rect, size: f32, color: Color) -> Element {
    let mut e = placed(Element::new(id, Widget::Text(value.into())), r);
    e.style.text.font_size = size;
    e.style.text.line_height = size * 1.35;
    e.style.text.color = color;
    e.style.text.wrap = TextWrap::None;
    e
}
fn control(id: u64, value: impl Into<String>, r: Rect) -> Element {
    let mut e = placed(Element::new(id, Widget::Button(value.into())), r);
    e.style.background = Some(paint(PANEL, 12.0, Some(Color::rgba(66, 80, 99, 255))));
    e.style.hover = Color::rgba(47, 64, 87, 255);
    e.style.focus = BLUE;
    e.layout = e.layout.with_padding(Insets::symmetric(10.0, 14.0));
    e
}
fn short(value: &str, limit: usize) -> String {
    let mut s: String = value.chars().take(limit).collect();
    if value.chars().count() > limit {
        s.push('…');
    }
    s
}
impl Overview {
    pub(super) fn compose(&mut self) -> Element {
        self.targets.clear();
        self.panes.clear();
        let [width, height] = self.size;
        let mut root = Element::new(0, Widget::Panel).layout(Layout {
            height: Length::Fill,
            flow: Flow::Overlay,
            gap: 0.0,
            ..Default::default()
        });
        root.style.background = Some(ShapePaint::fill(INK));
        let pad = (width * 0.045).clamp(12.0, 64.0);
        let search_width = (width - pad * 2.0).clamp(1.0, 760.0);
        let search_rect = Rect::new(
            (width - search_width) / 2.0,
            28.0f32.min(height * 0.04),
            search_width,
            54.0f32.min(height * 0.15),
        );
        let mut search = placed(
            Element::new(SEARCH.0, Widget::TextInput(self.query.clone())),
            search_rect,
        );
        search.layout = search.layout.with_padding(Insets {
            top: 10.0,
            right: 22.0,
            bottom: 10.0,
            left: 22.0,
        });
        search.style.background = Some(paint(
            Color::rgba(44, 55, 69, 255),
            27.0,
            Some(Color::rgba(76, 91, 110, 255)),
        ));
        search.style.text.font_size = 16.0;
        search.style.text.line_height = 22.0;
        if self.query.text().is_empty() {
            search.children.push(text(
                3,
                "Search applications…",
                Rect::new(22.0, 16.0, (search_width - 44.0).max(0.0), 22.0),
                16.0,
                MUTED,
            ));
        }
        // Child offsets are relative to the content area; search decorations use the border box.
        search.layout.clip = ClipMode::Bounds;
        // Padding applies to control text; overlay children are placed relative to that padding.
        for child in &mut search.children {
            child.layout.offset[0] -= 22.0;
            child.layout.offset[1] -= 10.0;
        }
        root.children.push(search);
        if !self.query.text().is_empty() {
            let clear = control(
                4,
                "×",
                Rect::new(
                    search_rect.x + search_rect.width - 42.0,
                    search_rect.y + 8.0,
                    34.0,
                    38.0,
                ),
            );
            self.targets.insert(ElementId(4), Target::ClearSearch);
            root.children.push(clear);
        }
        if self.query.text().is_empty()
            && let (Some(snapshot), Some(ws)) = (&self.snapshot, self.workspace)
        {
            let index = snapshot
                .workspaces
                .iter()
                .position(|w| w.workspace == ws)
                .unwrap_or(0);
            let top = search_rect.y + search_rect.height + 42.0f32.min(height * 0.04);
            let bottom_room = 100.0f32.min(height * 0.18);
            let available_h = (height - top - bottom_room).max(40.0);
            let preview_width = ((width * 0.82).min(available_h * 16.0 / 9.0)).max(1.0);
            let preview_height = (preview_width * 9.0 / 16.0).min(available_h);
            let x = (width - preview_width) / 2.0;
            let y = top + (available_h - preview_height) / 2.0;
            let rect = Rect::new(x, y, preview_width, preview_height);
            let spaces: Vec<_> = snapshot
                .workspaces
                .iter()
                .enumerate()
                .filter(|(i, _)| i.abs_diff(index) <= 1)
                .map(|(i, w)| (i, w.clone()))
                .collect();
            for (i, w) in spaces.iter().filter(|(i, _)| *i != index) {
                let scale = 0.86;
                let small_w = preview_width * scale;
                let small_h = preview_height * scale;
                let side_x = if *i < index {
                    x - small_w - 24.0
                } else {
                    x + preview_width + 24.0
                };
                root.children.push(self.workspace_card(
                    w.workspace,
                    Rect::new(
                        side_x,
                        y + (preview_height - small_h) / 2.0,
                        small_w,
                        small_h,
                    ),
                    false,
                ));
            }
            root.children.push(self.workspace_card(ws, rect, true));
            let count = self.snapshot.as_ref().unwrap().workspaces.len();
            let first = index.saturating_sub(4).min(count.saturating_sub(9));
            let dots: Vec<_> = self
                .snapshot
                .as_ref()
                .unwrap()
                .workspaces
                .iter()
                .skip(first)
                .take(9)
                .map(|w| w.workspace)
                .collect();
            let dot_width = dots.len() as f32 * 24.0;
            for (n, workspace) in dots.into_iter().enumerate() {
                let id = 0x200_0000_0000 + u64::from(workspace.0);
                let mut dot = control(
                    id,
                    "",
                    Rect::new(
                        (width - dot_width) / 2.0 + n as f32 * 24.0,
                        y + preview_height + 20.0,
                        16.0,
                        16.0,
                    ),
                );
                dot.layout.padding_edges = None;
                dot.style.background = Some(paint(
                    if workspace == ws {
                        BLUE
                    } else {
                        Color::rgba(71, 86, 109, 255)
                    },
                    8.0,
                    None,
                ));
                dot.tooltip = Some(format!("Workspace {}", workspace.0));
                self.targets
                    .insert(ElementId(id), Target::Workspace(workspace));
                root.children.push(dot);
            }
            let prev = index
                .checked_sub(1)
                .and_then(|i| self.snapshot.as_ref()?.workspaces.get(i))
                .map(|w| w.workspace);
            let next = self
                .snapshot
                .as_ref()
                .unwrap()
                .workspaces
                .get(index + 1)
                .map(|w| w.workspace);
            for (id, label, target, bx) in [
                (20, "‹", prev, 12.0),
                (21, "›", next, (width - 48.0).max(0.0)),
            ] {
                if let Some(target) = target {
                    let mut b = control(
                        id,
                        label,
                        Rect::new(bx, y + preview_height / 2.0 - 24.0, 36.0, 48.0),
                    );
                    b.style.text.font_size = 24.0;
                    self.targets
                        .insert(ElementId(id), Target::Workspace(target));
                    root.children.push(b);
                }
            }
        } else if self.query.text().is_empty() {
            let message = if self.snapshot.is_some() {
                "No workspaces available"
            } else {
                "Connecting to desktop…"
            };
            let mut e = text(
                11,
                message,
                Rect::new(pad, height * 0.4, (width - pad * 2.0).max(0.0), 40.0),
                22.0,
                MUTED,
            );
            e.style.text.align = TextAlign::Center;
            root.children.push(e);
        }
        if !self.query.text().is_empty() {
            root.children.push(self.search_results(search_rect));
        }
        let footer = if self.pending {
            Some("Opening…".to_owned())
        } else {
            self.error.as_deref().map(|error| short(error, 140))
        };
        if let Some(footer) = footer {
            let mut status = text(
                12,
                footer,
                Rect::new(
                    pad,
                    (height - 26.0).max(0.0),
                    (width - pad * 2.0).max(0.0),
                    22.0,
                ),
                12.0,
                if self.error.is_some() {
                    Color::rgba(255, 179, 166, 255)
                } else {
                    MUTED
                },
            );
            status.style.text.align = TextAlign::Center;
            root.children.push(status);
        }
        root
    }
    fn workspace_card(&mut self, ws: WorkspaceId, rect: Rect, selected: bool) -> Element {
        let id = 0x100_0000_0000 + u64::from(ws.0) * 32;
        let mut card = placed(Element::new(id, Widget::Button(String::new())), rect);
        card.style.background = Some(paint(
            PANEL,
            24.0,
            Some(if selected {
                BLUE
            } else {
                Color::rgba(77, 94, 115, 255)
            }),
        ));
        card.style.hover = PANEL;
        card.style.pressed = PANEL;
        card.style.focus = BLUE;
        card.layout.clip = ClipMode::Bounds;
        self.targets.insert(
            ElementId(id),
            if selected {
                Target::EnterWorkspace(ws)
            } else {
                Target::PreviewWorkspace(ws)
            },
        );
        let strip_h = 88.0f32.min(rect.height * 0.35);
        let top = 20.0;
        let bottom = if selected { strip_h + 20.0 } else { 20.0 };
        let pane_height = (rect.height - top - bottom).max(0.0);
        if pane_height >= 1.0
            && rect.width > 40.0
            && rect.x + rect.width > 0.0
            && rect.x < self.size[0]
        {
            self.panes.push(OverviewPane {
                workspace: ws,
                x: (rect.x + 20.0).round() as i32,
                y: (rect.y + top).round() as i32,
                width: (rect.width - 40.0).round() as u32,
                height: pane_height.round() as u32,
            });
        }
        if self.snapshot.as_ref().is_some_and(|snapshot| {
            !snapshot
                .windows
                .iter()
                .any(|window| window.workspace == ws && !window.minimized)
        }) {
            let mut label = text(
                id + 2,
                "No visible windows",
                Rect::new(16.0, rect.height * 0.35, (rect.width - 32.0).max(0.0), 50.0),
                18.0,
                MUTED,
            );
            label.style.text.align = TextAlign::Center;
            card.children.push(label);
        }
        if selected {
            let min: Vec<_> = self.minimized(ws).into_iter().cloned().collect();
            let capacity = (((rect.width - 112.0) / 170.0).floor() as usize).clamp(1, PAGE_SIZE);
            self.page = self.page.min(min.len().saturating_sub(1) / capacity);
            let first = self.page * capacity;
            let shown = min.len().saturating_sub(first).min(capacity);
            let strip_h = 88.0f32.min(rect.height * 0.35);
            let strip_y = (rect.height - strip_h - 14.0).max(0.0);
            let mut shelf = panel(
                id + 3,
                Rect::new(14.0, strip_y, (rect.width - 28.0).max(0.0), strip_h),
                Color::rgba(14, 21, 30, 238),
                14.0,
            );
            shelf.children.push(text(
                id + 4,
                format!("Minimized · {}", min.len()),
                Rect::new(12.0, 6.0, (rect.width - 64.0).max(0.0), 19.0),
                12.0,
                MUTED,
            ));
            if min.is_empty() {
                shelf.children.push(text(
                    id + 5,
                    "No minimized windows",
                    Rect::new(12.0, 32.0, (rect.width - 64.0).max(0.0), 24.0),
                    14.0,
                    MUTED,
                ));
            }
            let item_width = ((rect.width - 112.0).max(30.0) / capacity as f32).min(230.0);
            for (n, window) in min.iter().skip(first).take(shown).enumerate() {
                let wid = self.window_ids[&window.id];
                let title = if window.title.is_empty() {
                    &window.app_id
                } else {
                    &window.title
                };
                let mut chip = control(
                    wid.0,
                    format!("{}\n{}", short(&window.app_id, 18), short(title, 28)),
                    Rect::new(
                        12.0 + n as f32 * item_width,
                        28.0,
                        (item_width - 8.0).max(1.0),
                        (strip_h - 34.0).max(1.0),
                    ),
                );
                chip.style.text.font_size = 12.0;
                chip.style.text.line_height = 17.0;
                chip.layout = chip.layout.with_padding(Insets::symmetric(3.0, 10.0));
                chip.tooltip = Some(format!("Restore {}", short(title, 120)));
                chip.enabled = !self.pending;
                self.targets.insert(wid, Target::Window(window.id));
                shelf.children.push(chip);
            }
            if min.len() > capacity {
                for (bid, label, next, y) in [(30, "‹", false, 24.0), (31, "›", true, 49.0)] {
                    let mut b = control(
                        bid,
                        label,
                        Rect::new((rect.width - 70.0).max(0.0), y, 28.0, 23.0),
                    );
                    b.layout.padding_edges = Some(Insets::symmetric(1.0, 8.0));
                    b.enabled = if next {
                        first + capacity < min.len()
                    } else {
                        self.page > 0
                    };
                    self.targets.insert(ElementId(bid), Target::Page(next));
                    shelf.children.push(b);
                }
            }
            card.children.push(shelf);
        }
        card
    }
    fn search_results(&mut self, search: Rect) -> Element {
        let matches = self.search();
        let page_size = self.search_page_size();
        self.result_page = self
            .result_page
            .min(matches.len().saturating_sub(1) / page_size);
        let start = self.result_page * page_size;
        let shown = matches.len().saturating_sub(start).min(page_size);
        let visible: Vec<(usize, String, String)> = match &self.apps {
            Apps::Ready(catalog) => matches
                .iter()
                .skip(start)
                .take(shown)
                .filter_map(|&index| {
                    let app = catalog.get(index)?;
                    let icon = app.icon.clone().unwrap_or_else(|| icons::FALLBACK.into());
                    let label = if app.description.is_empty() {
                        short(&app.name, 70)
                    } else {
                        format!("{}\n{}", short(&app.name, 70), short(&app.description, 90))
                    };
                    Some((index, label, icon))
                })
                .collect(),
            Apps::Unloaded | Apps::Loading | Apps::Unavailable => Vec::new(),
        };
        for (_, _, icon) in &visible {
            // Failed submits retry on the next frame, which the worker's results trigger.
            if self.icons.needs_request(icon)
                && self.loader().submit(loader::Job::Icon(icon.clone()))
            {
                self.icons.requested(icon);
            }
        }
        let rows: Vec<(usize, String, Option<UiImage>)> = visible
            .into_iter()
            .map(|(index, label, icon)| (index, label, self.icons.get(&icon).cloned()))
            .collect();
        let mut panel = panel(
            40,
            Rect::new(
                search.x,
                search.y + search.height + 12.0,
                search.width,
                66.0 + rows.len() as f32 * 60.0,
            ),
            INK,
            18.0,
        );
        if matches!(self.apps, Apps::Ready(_)) && !matches.is_empty() {
            panel.children.push(text(
                41,
                match matches.len() {
                    1 => "1 application".to_owned(),
                    n => format!("{n} applications"),
                },
                Rect::new(18.0, 10.0, (search.width - 150.0).max(0.0), 22.0),
                13.0,
                MUTED,
            ));
        }
        for (n, (index, label, image)) in rows.into_iter().enumerate() {
            let id = app_element(index);
            let mut result = control(
                id.0,
                label,
                Rect::new(
                    12.0,
                    38.0 + n as f32 * 60.0,
                    (search.width - 24.0).max(0.0),
                    54.0,
                ),
            );
            // Text keeps a fixed column whether or not the icon resolved.
            let pad = Insets {
                top: 6.0,
                right: 12.0,
                bottom: 6.0,
                left: 58.0,
            };
            result.layout = result.layout.with_padding(pad);
            if let Some(image) = image {
                // Child offsets are relative to the padded content area.
                let icon = placed(
                    Element::new(
                        0x400_0000_0000 + index as u64,
                        Widget::Image(
                            image,
                            ImageStyle {
                                fit: ImageFit::Contain,
                                ..Default::default()
                            },
                        ),
                    ),
                    Rect::new(14.0 - pad.left, 11.0 - pad.top, 32.0, 32.0),
                );
                // Without this the row clips children to its padded content area.
                result.layout.clip = ClipMode::Bounds;
                result.children.push(icon);
            }
            result.enabled = !self.pending;
            self.targets.insert(id, Target::App(index));
            panel.children.push(result);
        }
        if matches.is_empty() {
            panel.children.push(text(
                42,
                match self.apps {
                    Apps::Unavailable => "Application list unavailable",
                    Apps::Unloaded | Apps::Loading => "Loading applications…",
                    Apps::Ready(_) => "No applications found",
                },
                Rect::new(18.0, 36.0, (search.width - 36.0).max(0.0), 24.0),
                13.0,
                MUTED,
            ));
        }
        if matches.len() > page_size {
            for (id, label, next, x) in [
                (30, "‹", false, search.width - 86.0),
                (31, "›", true, search.width - 48.0),
            ] {
                let mut b = control(id, label, Rect::new(x, 6.0, 32.0, 26.0));
                b.layout.padding_edges = Some(Insets::symmetric(2.0, 10.0));
                b.enabled = if next {
                    start + page_size < matches.len()
                } else {
                    self.result_page > 0
                };
                self.targets.insert(ElementId(id), Target::Page(next));
                panel.children.push(b);
            }
        }
        panel
    }
}
