use super::*;

impl Scene {
    pub fn layout(&mut self, size: [f32; 2], text: &mut dyn TextMeasurer) {
        if size.iter().any(|s| !s.is_finite() || *s <= 0.0) {
            return;
        }
        let viewport = Rect::new(0.0, 0.0, size[0], size[1]);
        if self.viewport != viewport {
            self.viewport = viewport;
            self.invalidate_layout();
        }
        if !self.layout_dirty {
            return;
        }
        if self.measure_dirty {
            self.measure_node(0, size[0], text);
        }
        self.arrange(
            0,
            self.nodes[0].element.layout.margin.apply(viewport),
            Some(viewport),
        );
        let popups: Vec<_> = self.scopes.iter().map(|(id, _)| self.indices[id]).collect();
        for i in popups {
            if self.measure_dirty {
                self.measure_node(i, size[0], text);
            }
            let n = &self.nodes[i];
            let l = n.element.layout;
            let area = l.margin.apply(viewport);
            let w = resolve(l.width, n.natural[0], area.width).min(area.width);
            let h = resolve(l.height, n.natural[1], area.height).min(area.height);
            self.arrange(
                i,
                Rect::new(
                    area.x + (area.width - w) / 2.0,
                    area.y + (area.height - h) / 2.0,
                    w,
                    h,
                ),
                Some(viewport),
            );
        }
        self.order = self.build_paint_order();
        self.measure_dirty = false;
        self.layout_dirty = false;
        self.paint_dirty = true;
        self.stats.layouts += 1;
        self.update_menu_bounds();
    }
    fn measure_node(&mut self, i: usize, width: f32, text: &mut dyn TextMeasurer) -> [f32; 2] {
        if !self.nodes[i].element.visible {
            self.nodes[i].natural = [0.0; 2];
            return [0.0; 2];
        }
        let layout = self.nodes[i].element.layout;
        let available = match layout.width {
            Length::Px(w) => w,
            _ => (width - layout.margin.horizontal()).max(0.0),
        };
        let padding = layout.insets();
        let inner = (available - padding.horizontal()).max(1.0);
        let mut intrinsic = match &self.nodes[i].element.widget {
            Widget::Text(value) => {
                let value = value.clone();
                let style = self.nodes[i].element.style.text.clone();
                self.nodes[i].text = text.measure(&value, &style, inner);
                self.stats.measured_texts += 1;
                self.nodes[i].text.size
            }
            Widget::Paragraph(spans) => {
                let spans = spans.clone();
                let style = self.nodes[i].element.style.text.clone();
                self.nodes[i].text = text.measure_spans(&spans, &style, inner);
                self.stats.measured_texts += 1;
                self.nodes[i].text.size
            }
            Widget::TextInput(edit) => {
                let value = edit.text().to_owned();
                let mut style = self.nodes[i].element.style.text.clone();
                style.wrap = crate::TextWrap::None;
                self.nodes[i].text = text.measure(&value, &style, 16384.0);
                self.stats.measured_texts += 1;
                [160.0, 36.0]
            }
            widget
                if widget.focusable()
                    && self.nodes[i].element.style.text.wrap != crate::TextWrap::None
                    && !matches!(widget, Widget::Slider(_)) =>
            {
                let value = widget.display_label().to_owned();
                let reserve = widget.label_reserve();
                let area = label_content(Rect::new(0.0, 0.0, available, 0.0), layout);
                let style = self.nodes[i].element.style.text.clone();
                self.nodes[i].text = text.measure(&value, &style, (area.width - reserve).max(1.0));
                self.stats.measured_texts += 1;
                let size = self.nodes[i].text.size;
                // Uniform/explicit padding is added below; default label insets are horizontal only.
                let implicit_padding = if layout.padding_edges.is_none() && layout.padding == 0.0 {
                    20.0
                } else {
                    0.0
                };
                [
                    (size[0] + reserve + implicit_padding).max(120.0),
                    size[1].max(36.0),
                ]
            }
            Widget::Separator => [1.0, 1.0],
            Widget::Image(..) => [64.0, 64.0],
            Widget::Panel | Widget::Scroll | Widget::Popup { .. } => [0.0, 0.0],
            _ => [120.0, 36.0],
        };
        let children = self.nodes[i].children.clone();
        let active: Vec<_> = children
            .into_iter()
            .filter(|c| {
                self.nodes[*c].element.visible
                    && !matches!(self.nodes[*c].element.widget, Widget::Popup { .. })
            })
            .collect();
        let child_width = if let Flow::Grid { columns } = layout.flow {
            ((inner - layout.gap * (columns - 1) as f32) / columns as f32).max(1.0)
        } else {
            inner
        };
        let mut sizes: Vec<_> = active
            .iter()
            .map(|c| self.measure_node(*c, child_width, text))
            .collect();
        // Fill children must shape at the width they will actually receive.
        if layout.flow == Flow::Row {
            let fixed: f32 = active
                .iter()
                .zip(&sizes)
                .filter(|(c, _)| self.nodes[**c].element.layout.width != Length::Fill)
                .map(|(_, size)| size[0])
                .sum();
            let fills = active
                .iter()
                .filter(|c| self.nodes[**c].element.layout.width == Length::Fill)
                .count();
            if fills > 0 {
                let share = ((inner - fixed - layout.gap * active.len().saturating_sub(1) as f32)
                    / fills as f32)
                    .max(0.0);
                for (position, c) in active.iter().copied().enumerate() {
                    if self.nodes[c].element.layout.width == Length::Fill {
                        sizes[position] = self.measure_node(c, share, text);
                        sizes[position][0] = share;
                    }
                }
            }
        }
        if !sizes.is_empty() {
            intrinsic = match layout.flow {
                Flow::Column => [
                    sizes.iter().map(|s| s[0]).fold(0.0, f32::max),
                    sizes.iter().map(|s| s[1]).sum::<f32>() + layout.gap * (sizes.len() - 1) as f32,
                ],
                Flow::Row => [
                    sizes.iter().map(|s| s[0]).sum::<f32>() + layout.gap * (sizes.len() - 1) as f32,
                    sizes.iter().map(|s| s[1]).fold(0.0, f32::max),
                ],
                Flow::Overlay => [
                    sizes.iter().map(|s| s[0]).fold(0.0, f32::max),
                    sizes.iter().map(|s| s[1]).fold(0.0, f32::max),
                ],
                Flow::Grid { columns } => [
                    inner,
                    sizes
                        .chunks(columns)
                        .map(|row| row.iter().map(|s| s[1]).fold(0.0, f32::max))
                        .sum::<f32>()
                        + layout.gap * (sizes.len().div_ceil(columns) - 1) as f32,
                ],
            };
        }
        intrinsic[0] += padding.horizontal();
        intrinsic[1] += padding.vertical();
        if let Length::Px(v) = layout.width {
            intrinsic[0] = v;
        }
        if let Length::Px(v) = layout.height {
            intrinsic[1] = v;
        }
        self.nodes[i].natural = intrinsic;
        [
            intrinsic[0] + layout.margin.horizontal(),
            intrinsic[1] + layout.margin.vertical(),
        ]
    }
    fn arrange(&mut self, i: usize, mut bounds: Rect, parent_clip: Option<Rect>) {
        let l = self.nodes[i].element.layout;
        bounds.x += l.offset[0];
        bounds.y += l.offset[1];
        self.nodes[i].bounds = bounds;
        self.nodes[i].clip = parent_clip.and_then(|c| c.intersection(bounds));
        let inner = l.insets().apply(bounds);
        let children = self.nodes[i].children.clone();
        let active: Vec<_> = children
            .into_iter()
            .filter(|c| {
                self.nodes[*c].element.visible
                    && !matches!(self.nodes[*c].element.widget, Widget::Popup { .. })
            })
            .collect();
        if active.is_empty() {
            return;
        }
        let scroll = matches!(self.nodes[i].element.widget, Widget::Scroll);
        let content_h = match l.flow {
            Flow::Column => {
                active
                    .iter()
                    .map(|c| outer(&self.nodes[*c], 1))
                    .sum::<f32>()
                    + l.gap * (active.len() - 1) as f32
            }
            Flow::Grid { columns } => {
                active
                    .chunks(columns)
                    .map(|row| {
                        row.iter()
                            .map(|c| outer(&self.nodes[*c], 1))
                            .fold(0.0, f32::max)
                    })
                    .sum::<f32>()
                    + l.gap * (active.len().div_ceil(columns) - 1) as f32
            }
            _ => active
                .iter()
                .map(|c| outer(&self.nodes[*c], 1))
                .fold(0.0, f32::max),
        };
        self.nodes[i].scroll_max = if scroll {
            (content_h - inner.height).max(0.0)
        } else {
            0.0
        };
        self.nodes[i].scroll = self.nodes[i].scroll.clamp(0.0, self.nodes[i].scroll_max);
        let clip = if scroll || l.clip == ClipMode::Content {
            parent_clip.and_then(|c| c.intersection(inner))
        } else if l.clip == ClipMode::Bounds {
            self.nodes[i].clip
        } else {
            parent_clip
        };
        let horizontal = l.flow == Flow::Row;
        let axis = usize::from(!horizontal);
        let space = if horizontal {
            inner.width
        } else {
            inner.height
        };
        let fixed: f32 = active
            .iter()
            .filter(|c| scroll || dimension(&self.nodes[**c], axis) != Length::Fill)
            .map(|c| outer(&self.nodes[*c], axis))
            .sum();
        let fills = active
            .iter()
            .filter(|c| !scroll && dimension(&self.nodes[**c], axis) == Length::Fill)
            .count();
        let share = if fills > 0 {
            ((space - fixed - l.gap * (active.len() - 1) as f32) / fills as f32).max(0.0)
        } else {
            0.0
        };
        let mut cursor = if horizontal {
            inner.x
        } else {
            inner.y - self.nodes[i].scroll
        };
        let mut grid_y = inner.y - self.nodes[i].scroll;
        let grid_columns = if let Flow::Grid { columns } = l.flow {
            columns
        } else {
            1
        };
        for (position, c) in active.iter().copied().enumerate() {
            let node = &self.nodes[c];
            let cl = node.element.layout;
            let margin = cl.margin;
            let available_w = (inner.width - margin.horizontal()).max(0.0);
            let available_h = (inner.height - margin.vertical()).max(0.0);
            let mut w = resolve(cl.width, node.natural[0], available_w);
            let mut h = resolve(cl.height, node.natural[1], available_h);
            if scroll && matches!(cl.height, Length::Auto | Length::Fill) {
                h = node.natural[1];
            }
            let r = match l.flow {
                Flow::Overlay => Rect::new(
                    inner.x + margin.left + align(l.align, available_w, w),
                    inner.y + margin.top + align(l.align, available_h, h) - self.nodes[i].scroll,
                    w,
                    h,
                ),
                Flow::Grid { columns } => {
                    let cell =
                        ((inner.width - l.gap * (columns - 1) as f32) / columns as f32).max(0.0);
                    w = resolve(
                        cl.width,
                        node.natural[0],
                        (cell - margin.horizontal()).max(0.0),
                    )
                    .min((cell - margin.horizontal()).max(0.0));
                    if cl.height == Length::Fill {
                        h = node.natural[1];
                    }
                    let r = Rect::new(
                        inner.x + (position % columns) as f32 * (cell + l.gap) + margin.left,
                        grid_y + margin.top,
                        w,
                        h,
                    );
                    if position % columns == columns - 1 {
                        let start = position + 1 - grid_columns;
                        grid_y += active[start..=position]
                            .iter()
                            .map(|j| outer(&self.nodes[*j], 1))
                            .fold(0.0, f32::max)
                            + l.gap;
                    }
                    r
                }
                Flow::Row => {
                    if cl.width == Length::Fill {
                        w = (share - margin.horizontal()).max(0.0);
                    }
                    let r = Rect::new(
                        cursor + margin.left,
                        inner.y + margin.top + align(l.align, available_h, h)
                            - self.nodes[i].scroll,
                        w,
                        h,
                    );
                    cursor += w + margin.horizontal() + l.gap;
                    r
                }
                Flow::Column => {
                    if cl.height == Length::Fill {
                        h = if scroll {
                            node.natural[1]
                        } else {
                            (share - margin.vertical()).max(0.0)
                        };
                    }
                    let r = Rect::new(
                        inner.x + margin.left + align(l.align, available_w, w),
                        cursor + margin.top,
                        w,
                        h,
                    );
                    cursor += h + margin.vertical() + l.gap;
                    r
                }
            };
            self.arrange(c, r, clip);
        }
    }
}
fn dimension(n: &Node, axis: usize) -> Length {
    if axis == 0 {
        n.element.layout.width
    } else {
        n.element.layout.height
    }
}
fn resolve(length: Length, natural: f32, available: f32) -> f32 {
    match length {
        Length::Px(v) => v,
        Length::Auto => natural.min(available),
        Length::Fill => available,
    }
}
fn align(a: Align, available: f32, size: f32) -> f32 {
    match a {
        Align::Center => (available - size).max(0.0) / 2.0,
        Align::End => (available - size).max(0.0),
        _ => 0.0,
    }
}

fn outer(node: &Node, axis: usize) -> f32 {
    node.natural[axis]
        + if axis == 0 {
            node.element.layout.margin.horizontal()
        } else {
            node.element.layout.margin.vertical()
        }
}
