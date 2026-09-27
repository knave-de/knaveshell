use super::*;
use crate::{Border, CornerRadii, DisplayCommand, ShapePaint, TextStyle, TextWrap, Transform2D};

impl Scene {
    /// Reuses the previous display list when neither layout nor visual state changed.
    pub fn display_list(&mut self, size: [f32; 2], text: &mut dyn TextMeasurer) -> &DisplayList {
        self.layout(size, text);
        if !self.paint_dirty {
            return &self.list;
        }
        self.list.commands.clear();
        self.list.revision = self.list.revision.wrapping_add(1);
        self.list.clear_color = Color::BACKGROUND;
        self.stats.visible_elements = 0;
        let order = self.order.clone();
        for i in order {
            if self.nodes[i].clip.is_none() {
                continue;
            }
            self.stats.visible_elements += 1;
            self.paint_node(i);
        }
        self.paint_menu();
        if self.menu.is_none()
            && self.capture.is_none()
            && let Some(id) = self.hover
        {
            let node = &self.nodes[self.indices[&id]];
            if let Some(label) = &node.element.tooltip {
                let w = 280.0f32.min(self.viewport.width);
                let b = Rect::new(
                    node.bounds.x.clamp(0.0, (self.viewport.width - w).max(0.0)),
                    (node.bounds.y + node.bounds.height + 6.0)
                        .min((self.viewport.height - 52.0).max(0.0)),
                    w,
                    52.0,
                );
                shape(
                    &mut self.list,
                    b,
                    Some(self.viewport),
                    ShapePaint::fill(Color::rgba(45, 61, 80, 255)),
                );
                text_cmd(
                    &mut self.list,
                    inset(b, 8.0),
                    Some(b),
                    label.clone(),
                    node.element.style.text.clone(),
                );
            }
        }
        self.paint_dirty = false;
        self.stats.paints += 1;
        &self.list
    }
    fn paint_node(&mut self, i: usize) {
        let node = &self.nodes[i];
        let e = &node.element;
        let b = node.bounds;
        let clip = node.clip;
        let state = self.interaction(e.id).unwrap();
        let mut style = e.style.clone();
        if state.disabled {
            style.text.color.alpha = 105;
        }
        if matches!(e.widget, Widget::Popup { modal: true }) {
            shape(
                &mut self.list,
                self.viewport,
                Some(self.viewport),
                ShapePaint::fill(Color::rgba(0, 0, 0, 135)),
            );
        }
        let control = e.widget.focusable() || matches!(e.widget, Widget::Popup { .. });
        let mut background = style
            .background
            .clone()
            .or_else(|| control.then(|| ShapePaint::fill(Color::rgba(33, 44, 60, 255))));
        if let Some(paint) = &mut background {
            paint.radii = CornerRadii::uniform(style.radius);
            if state.selected {
                paint.fill = Some(style.selected);
            }
            if state.hovered && !state.disabled {
                paint.fill = Some(style.hover);
            }
            if state.pressed && !state.disabled {
                paint.fill = Some(style.pressed);
            }
            if state.disabled {
                paint.opacity *= 0.5;
            }
            if state.focused {
                paint.border = Some(Border {
                    color: style.focus,
                    width: 2.0,
                });
            }
            shape(&mut self.list, b, clip, paint.clone());
        }
        let content = inset(b, 10.0);
        let mut label = None;
        match &e.widget {
            Widget::Text(value) => {
                text_cmd(&mut self.list, b, clip, value.clone(), style.text.clone())
            }
            Widget::Paragraph(spans) => self.list.commands.push(DisplayCommand::RichText {
                bounds: b,
                spans: spans.clone(),
                style: style.text.clone(),
                transform: Transform2D::IDENTITY,
                clip,
            }),
            Widget::Image(image, image_style) => self.list.commands.push(DisplayCommand::Image {
                bounds: b,
                image: image.clone(),
                style: *image_style,
                transform: Transform2D::IDENTITY,
                clip,
            }),
            Widget::Button(value) | Widget::Draggable(value) => label = Some(value.clone()),
            Widget::Selectable { label: value, .. } => label = Some(value.clone()),
            Widget::Checkbox {
                label: value,
                checked,
            }
            | Widget::Toggle {
                label: value,
                checked,
            } => {
                let toggle = matches!(e.widget, Widget::Toggle { .. });
                let w = if toggle { 32.0 } else { 18.0 };
                let rect = Rect::new(content.x, b.y + (b.height - 18.0) / 2.0, w, 18.0);
                let mut paint = ShapePaint::fill(if *checked {
                    Color::ACCENT
                } else {
                    Color::rgba(72, 85, 102, 255)
                });
                paint.radii = CornerRadii::uniform(if toggle { 9.0 } else { 3.0 });
                shape(&mut self.list, rect, clip, paint);
                if *checked || toggle {
                    shape(
                        &mut self.list,
                        Rect::new(
                            rect.x + if toggle && *checked { 17.0 } else { 3.0 },
                            rect.y + 3.0,
                            12.0,
                            12.0,
                        ),
                        clip,
                        ShapePaint::fill(Color::TEXT),
                    );
                }
                text_cmd(
                    &mut self.list,
                    Rect::new(
                        content.x + w + 8.0,
                        b.y,
                        (content.width - w - 8.0).max(0.0),
                        b.height,
                    ),
                    clip,
                    value.clone(),
                    style.text.clone(),
                );
            }
            Widget::Dropdown {
                label: placeholder,
                items,
                selected,
            } => {
                label = Some(
                    items
                        .iter()
                        .find_map(|item| match item {
                            MenuItem::Option { id, label, .. } if Some(*id) == *selected => {
                                Some(label.clone())
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| placeholder.clone()),
                );
            }
            Widget::Menu { label: value, .. } => label = Some(value.clone()),
            Widget::Slider(slider) => {
                let vertical = slider.orientation == Orientation::Vertical;
                let f = slider.fraction();
                let track = if vertical {
                    Rect::new(b.x + b.width / 2.0 - 2.0, content.y, 4.0, content.height)
                } else {
                    Rect::new(content.x, b.y + b.height / 2.0 - 2.0, content.width, 4.0)
                };
                shape(
                    &mut self.list,
                    track,
                    clip,
                    ShapePaint::fill(Color::rgba(80, 92, 111, 255)),
                );
                let fill = if vertical {
                    Rect::new(
                        track.x,
                        track.y + track.height * (1.0 - f),
                        track.width,
                        track.height * f,
                    )
                } else {
                    Rect::new(track.x, track.y, track.width * f, track.height)
                };
                if fill.is_finite_positive() {
                    shape(&mut self.list, fill, clip, ShapePaint::fill(Color::ACCENT));
                }
                let center = if vertical {
                    [b.x + b.width / 2.0, content.y + content.height * (1.0 - f)]
                } else {
                    [content.x + content.width * f, b.y + b.height / 2.0]
                };
                let mut paint = ShapePaint::fill(Color::TEXT);
                paint.radii = CornerRadii::uniform(9.0);
                shape(
                    &mut self.list,
                    Rect::new(center[0] - 9.0, center[1] - 9.0, 18.0, 18.0),
                    clip,
                    paint,
                );
            }
            Widget::TextInput(edit) => {
                let cursor = edit.cursor();
                let selection = edit.selection();
                let value = edit.text().to_owned();
                let preedit = edit.preedit.clone();
                let metrics = node.text.clone();
                let x = metrics.caret(cursor).map_or(0.0, |c| c.x);
                let mut offset = node.text_offset;
                if x - offset > content.width - 3.0 {
                    offset = (x - content.width + 3.0).max(0.0);
                } else if x < offset {
                    offset = x;
                }
                self.nodes[i].text_offset = offset;
                let text_bounds = Rect::new(content.x - offset, b.y, 16384.0, b.height);
                let text_clip = clip.and_then(|c| {
                    c.intersection(Rect::new(content.x, b.y, content.width, b.height))
                });
                if !selection.is_empty() {
                    for pair in metrics.carets.windows(2) {
                        if pair[0].byte >= selection.start && pair[0].byte < selection.end {
                            let left = pair[0].x.min(pair[1].x);
                            let width = (pair[1].x - pair[0].x).abs();
                            shape(
                                &mut self.list,
                                Rect::new(
                                    content.x + left - offset,
                                    b.y + 6.0,
                                    width,
                                    b.height - 12.0,
                                ),
                                text_clip,
                                ShapePaint::fill(Color::rgba(58, 107, 151, 255)),
                            );
                        }
                    }
                }
                let mut ts = style.text.clone();
                ts.wrap = TextWrap::None;
                text_cmd(&mut self.list, text_bounds, text_clip, value, ts.clone());
                if state.focused {
                    shape(
                        &mut self.list,
                        Rect::new(
                            content.x + x - offset,
                            b.y + 6.0,
                            1.5,
                            (b.height - 12.0).max(1.0),
                        ),
                        text_clip,
                        ShapePaint::fill(Color::TEXT),
                    );
                    if !preedit.is_empty() {
                        text_cmd(
                            &mut self.list,
                            Rect::new(content.x + x - offset, b.y, content.width, b.height),
                            text_clip,
                            preedit,
                            ts,
                        );
                    }
                }
            }
            Widget::Separator => shape(
                &mut self.list,
                Rect::new(b.x, b.y + b.height / 2.0, b.width, 1.0),
                clip,
                ShapePaint::fill(Color::rgba(80, 92, 111, 255)),
            ),
            Widget::Scroll if node.scroll_max > 0.0 => {
                let area = inset(b, e.layout.padding);
                let h = (area.height * area.height / (area.height + node.scroll_max))
                    .max(12.0)
                    .min(area.height);
                let y = area.y + (area.height - h) * node.scroll / node.scroll_max;
                shape(
                    &mut self.list,
                    Rect::new(b.x + b.width - 5.0, y, 3.0, h),
                    clip,
                    ShapePaint::fill(Color::rgba(106, 128, 153, 255)),
                );
            }
            _ => {}
        }
        if let Some(label) = label {
            let mut ts = style.text;
            ts.wrap = TextWrap::None;
            text_cmd(
                &mut self.list,
                Rect::new(content.x, b.y, content.width, b.height),
                clip,
                label,
                ts,
            );
        }
        if self.inspect {
            let mut paint = ShapePaint::fill(Color::rgba(0, 0, 0, 0));
            paint.border = Some(Border {
                color: if state.captured {
                    Color::rgba(255, 150, 60, 255)
                } else {
                    Color::rgba(70, 210, 150, 160)
                },
                width: 1.0,
            });
            shape(&mut self.list, b, clip, paint);
        }
    }
    fn paint_menu(&mut self) {
        let Some(menu) = &self.menu else {
            return;
        };
        let node = &self.nodes[self.indices[&menu.owner]];
        let b = menu.bounds;
        let clip = Some(b);
        shape(
            &mut self.list,
            b,
            clip,
            ShapePaint::fill(Color::rgba(28, 39, 55, 255)),
        );
        let Some(items) = node.element.widget.menu() else {
            return;
        };
        let mut y = b.y - menu.scroll;
        // Row backgrounds precede labels; rows are disjoint and clipped to the menu.
        let mut labels = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let h = super::input::row_height(item);
            let row = Rect::new(b.x, y, b.width, h);
            y += h;
            if row.intersection(b).is_none() {
                continue;
            }
            match item {
                MenuItem::Separator => shape(
                    &mut self.list,
                    Rect::new(b.x + 8.0, row.y + h / 2.0, (b.width - 16.0).max(0.0), 1.0),
                    clip,
                    ShapePaint::fill(Color::rgba(80, 92, 111, 255)),
                ),
                MenuItem::Option { label, enabled, .. } => {
                    if menu.highlighted == Some(index) {
                        shape(
                            &mut self.list,
                            row,
                            clip,
                            ShapePaint::fill(node.element.style.hover),
                        );
                    }
                    let mut style = node.element.style.text.clone();
                    style.wrap = TextWrap::None;
                    if !enabled {
                        style.color.alpha = 100;
                    }
                    labels.push((
                        Rect::new(row.x + 10.0, row.y, row.width - 20.0, row.height),
                        label.clone(),
                        style,
                    ));
                }
            }
        }
        for (bounds, label, style) in labels {
            text_cmd(&mut self.list, bounds, clip, label, style);
        }
    }
}
fn shape(list: &mut DisplayList, bounds: Rect, clip: Option<Rect>, paint: ShapePaint) {
    if clip.is_none() || !bounds.is_finite_positive() {
        return;
    }
    list.commands.push(DisplayCommand::Shape {
        bounds,
        paint,
        transform: Transform2D::IDENTITY,
        clip,
    });
}
fn text_cmd(
    list: &mut DisplayList,
    bounds: Rect,
    clip: Option<Rect>,
    text: String,
    style: TextStyle,
) {
    if clip.is_none() || text.is_empty() {
        return;
    }
    list.commands.push(DisplayCommand::Text {
        bounds,
        text,
        style,
        transform: Transform2D::IDENTITY,
        clip,
    });
}
