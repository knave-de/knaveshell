use super::*;
use knave_ui::{
    Color, CornerRadii, ImageFit, ImageStyle, Rect, ShapePaint, TextAlign, TextWrap, UiImage,
};

pub(super) const OVERVIEW_STYLESHEET: &str = "\
    .overview-muted { color: var(--text-secondary); } \
    .overview-error { color: var(--danger); } \
    .overview-search { background-color: var(--surface-sunken); outline: 1px solid var(--border-strong); border-radius: 27px; } \
    .overview-results { background-color: var(--surface-menu); border-radius: 18px; } \
    .overview-tray { background-color: var(--surface-menu); outline: 1px solid var(--border-strong); border-radius: 11px; } \
    .overview-card-selected, .overview-card-idle { border-radius: 24px; } \
    .overview-card-selected { outline: 1px solid var(--accent); } \
    .overview-card-idle { outline: 1px solid var(--border-strong); } \
    .overview-current-dot, .overview-other-dot { border-radius: 8px; } \
    .overview-current-dot { background-color: var(--accent); } \
    .overview-other-dot { background-color: var(--surface-hover); }";

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
fn panel(id: u64, rect: Rect) -> Element {
    placed(Element::new(id, Widget::Panel), rect)
}
fn text(id: u64, value: impl Into<String>, r: Rect, size: f32, class: &str) -> Element {
    let mut e = placed(Element::new(id, Widget::Text(value.into())), r);
    e.style.text.font_size = size;
    e.style.text.line_height = size * 1.35;
    e.classes.push(class.into());
    e.style.text.wrap = TextWrap::None;
    e
}
fn control(id: u64, value: impl Into<String>, r: Rect) -> Element {
    let mut e = placed(Element::new(id, Widget::Button(value.into())), r);
    // Let the bundled CSS own surface, hover, pressed and keyboard-focus paint.
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

/// One reserved element-id base keeps projected bar nodes apart from the
/// overview's own elements regardless of what the bar component produces.
const BAR_ELEMENT_ID_BASE: u64 = 0x800_0000_0000;
/// Derived per-tray-icon element id for a window's allocated control id.
const TRAY_ICON_ELEMENT_BASE: u64 = 0x500_0000_0000;

/// Icon theme name for a window's `app_id`, resolved from the loaded catalog.
/// Wayland app ids may be the desktop file id with or without its suffix, or
/// the displayed application name; the caller supplies a generic icon if no
/// entry or icon matches.
fn catalog_icon(catalog: &Catalog, app_id: &str) -> Option<String> {
    let key = app_id.to_lowercase();
    let file_id = |id: &str| id.strip_suffix(".desktop").unwrap_or(id).to_lowercase();
    let mut named = None;
    for index in 0..catalog.len() {
        let app = catalog.get(index)?;
        if file_id(&app.id) == key {
            return app.icon.clone();
        }
        if named.is_none() && app.name.to_lowercase() == key {
            named = app.icon.clone();
        }
    }
    named
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
        root.style.background = Some(ShapePaint::fill(Color::rgba(0, 0, 0, 0)));
        let pad = (width * 0.045).clamp(12.0, 64.0);
        let search_width = (width - pad * 2.0).clamp(1.0, 760.0);
        let search_rect = Rect::new(
            (width - search_width) / 2.0,
            STATUS_BAR_HEIGHT + 28.0f32.min(height * 0.04),
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
        search.classes.push("overview-search".into());
        search.style.text.font_size = 16.0;
        search.style.text.line_height = 22.0;
        if self.query.text().is_empty() {
            search.children.push(text(
                3,
                "Search applications…",
                Rect::new(22.0, 16.0, (search_width - 44.0).max(0.0), 22.0),
                16.0,
                "overview-muted",
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
        if self.query.text().is_empty()
            && let (Some(snapshot), Some(ws)) = (&self.snapshot, self.workspace)
        {
            let index = snapshot
                .workspaces
                .iter()
                .position(|w| w.workspace == ws)
                .unwrap_or(0);
            let top = search_rect.y + search_rect.height + 28.0f32.min(height * 0.04);
            let bottom_room = 64.0f32.min(height * 0.12);
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
                dot.classes.push(if workspace == ws {
                    "overview-current-dot".into()
                } else {
                    "overview-other-dot".into()
                });
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
                "overview-muted",
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
                    "overview-error"
                } else {
                    "overview-muted"
                },
            );
            status.style.text.align = TextAlign::Center;
            root.children.push(status);
        }
        // Drawn last so the status bar strip stays on top of overview content.
        root.children.push(self.status_bar(width));
        root
    }
    // Retained controls route the shared component's hit targets; all pixels
    // come directly from its display list in frame().
    fn status_bar(&mut self, width: f32) -> Element {
        let component = self.status_bar_component(width);
        let mut bar = placed(
            Element::new(BAR_ELEMENT_ID_BASE, Widget::Panel),
            Rect::new(0.0, 0.0, width, STATUS_BAR_HEIGHT),
        );
        bar.style.background = Some(ShapePaint::fill(Color::rgba(0, 0, 0, 0)));
        for (index, (bounds, action)) in component.scene.hit_targets().enumerate() {
            let id = BAR_ELEMENT_ID_BASE + 10 + index as u64;
            self.targets
                .insert(ElementId(id), Target::StatusBar(action));
            let mut button = placed(Element::new(id, Widget::Button(String::new())), bounds);
            let transparent = Color::rgba(0, 0, 0, 0);
            button.style.background = Some(ShapePaint::fill(transparent));
            if let knave_shell_ui::UiAction::Applet(id) = action {
                button.tooltip = self
                    .applets
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| item.title.clone());
            }
            button.enabled = !self.pending;
            bar.children.push(button);
        }
        bar
    }
    /// Icon theme name for a minimized window's application; apps missing
    /// from the catalog (or with no catalog at all) reuse the generic
    /// executable icon, with a built-in glyph if theme lookup also fails.
    fn window_icon_name(&self, app_id: &str) -> String {
        match &self.apps {
            Apps::Ready(catalog) => catalog_icon(catalog, app_id),
            _ => None,
        }
        .unwrap_or_else(|| icons::FALLBACK.to_owned())
    }
    fn workspace_card(&mut self, ws: WorkspaceId, rect: Rect, selected: bool) -> Element {
        let id = 0x100_0000_0000 + u64::from(ws.0) * 32;
        let mut card = placed(Element::new(id, Widget::Button(String::new())), rect);
        card.classes.push(if selected {
            "overview-card-selected".into()
        } else {
            "overview-card-idle".into()
        });
        // The compositor supplies each workspace preview through this window.
        card.style.background = Some(ShapePaint::fill(Color::rgba(0, 0, 0, 0)));
        card.layout.clip = ClipMode::Bounds;
        self.targets.insert(
            ElementId(id),
            if selected {
                Target::EnterWorkspace(ws)
            } else {
                Target::Workspace(ws)
            },
        );
        // The compositor fills the card; shell borders and controls remain above it.
        if rect.height > 4.0
            && rect.width > 4.0
            && rect.x + rect.width > 0.0
            && rect.x < self.size[0]
        {
            self.panes.push(OverviewPane {
                workspace: ws,
                x: (rect.x + 2.0).round() as i32,
                y: (rect.y + 2.0).round() as i32,
                width: (rect.width - 4.0).round() as u32,
                height: (rect.height - 4.0).round() as u32,
            });
        }
        if selected && !self.minimized(ws).is_empty() {
            let min: Vec<_> = self.minimized(ws).into_iter().cloned().collect();
            let tray_h = 50.0f32.min(rect.height * 0.35);
            let icon_size = (tray_h - 12.0).max(16.0);
            let pitch = icon_size + 5.0;
            let max_width = (rect.width - 24.0).max(0.0);
            // Room for two paging controls when several pages exist.
            let paging_w = 2.0 * 28.0 + 6.0;
            let capacity =
                (((max_width - 12.0 - paging_w) / pitch).floor() as usize).clamp(1, PAGE_SIZE);
            self.page = self.page.min(min.len().saturating_sub(1) / capacity);
            let first = self.page * capacity;
            let shown = min.len().saturating_sub(first).min(capacity);
            let span = (shown as f32 - 1.0) * pitch + icon_size;
            let tray_w = if min.len() > capacity {
                12.0 + span + paging_w
            } else {
                12.0 + span
            }
            .min(max_width);
            let mut tray = panel(
                id + 3,
                Rect::new(12.0, (rect.height - tray_h - 12.0).max(0.0), tray_w, tray_h),
            );
            tray.classes.push("overview-tray".into());
            tray.style.background = Some(ShapePaint::fill(Color::rgba(0, 0, 0, 0)));
            tray.layout.clip = ClipMode::Bounds;
            for (n, window) in min.iter().skip(first).take(shown).enumerate() {
                let wid = self.window_ids[&window.id];
                let title = if window.title.is_empty() {
                    window.app_id.as_str()
                } else {
                    window.title.as_str()
                };
                let icon_name = self.window_icon_name(&window.app_id);
                // Failed submits retry on the next frame, which the worker's results trigger.
                if self.icons.needs_request(&icon_name)
                    && self.loader().submit(loader::Job::Icon(icon_name.clone()))
                {
                    self.icons.requested(&icon_name);
                }
                let mut icon = placed(
                    Element::new(wid.0, Widget::Button(String::new())),
                    Rect::new(6.0 + n as f32 * pitch, 6.0, icon_size, icon_size),
                );
                icon.style.background = Some(ShapePaint::fill(Color::rgba(0, 0, 0, 0)));
                icon.style.radii = Some(CornerRadii::uniform(7.0));
                // Hover and accessibility name the exact window, not its application.
                icon.tooltip = Some(format!("Restore {}", short(title, 120)));
                icon.enabled = !self.pending;
                if let Some(image) = self.icons.get(&icon_name).cloned() {
                    let side = (icon_size - 8.0).max(10.0);
                    let inset = (icon_size - side) / 2.0;
                    icon.children.push(placed(
                        Element::new(
                            TRAY_ICON_ELEMENT_BASE + wid.0,
                            Widget::Image(
                                image,
                                ImageStyle {
                                    fit: ImageFit::Contain,
                                    ..Default::default()
                                },
                            ),
                        ),
                        Rect::new(inset, inset, side, side),
                    ));
                } else {
                    let mut fallback = text(
                        TRAY_ICON_ELEMENT_BASE + wid.0,
                        "▣",
                        Rect::new(0.0, 0.0, icon_size, icon_size),
                        icon_size * 0.72,
                        "overview-muted",
                    );
                    fallback.style.text.align = TextAlign::Center;
                    icon.children.push(fallback);
                }
                self.targets.insert(wid, Target::Window(window.id));
                tray.children.push(icon);
            }
            if min.len() > capacity {
                for (bid, label, next, bx) in [(30, "‹", false, 6.0), (31, "›", true, 34.0)] {
                    let mut b = control(
                        bid,
                        label,
                        Rect::new(
                            (tray_w - 6.0 - paging_w + bx).max(0.0),
                            (tray_h - 23.0) / 2.0,
                            28.0,
                            23.0,
                        ),
                    );
                    b.layout.padding_edges = Some(Insets::symmetric(1.0, 8.0));
                    b.enabled = if next {
                        first + capacity < min.len()
                    } else {
                        self.page > 0
                    };
                    self.targets.insert(ElementId(bid), Target::Page(next));
                    tray.children.push(b);
                }
            }
            card.children.push(tray);
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
        );
        panel.classes.push("overview-results".into());
        if matches!(self.apps, Apps::Ready(_)) && !matches.is_empty() {
            panel.children.push(text(
                41,
                match matches.len() {
                    1 => "1 application".to_owned(),
                    n => format!("{n} applications"),
                },
                Rect::new(18.0, 10.0, (search.width - 150.0).max(0.0), 22.0),
                13.0,
                "overview-muted",
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
                "overview-muted",
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
