//! Retained shell UI scenes. Coordinates are logical pixels; no desktop IPC is performed here.
mod input;
mod layout;
mod model;
mod paint;
mod text_edit;

use crate::{Color, DisplayList, Rect};
pub use input::{Action, EventResult, Input, Key, KeyModifiers};
pub use model::*;
use std::collections::BTreeMap;
pub use text_edit::TextEdit;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SceneStats {
    pub layouts: u64,
    pub paints: u64,
    pub measured_texts: u64,
    pub visible_elements: usize,
}
#[derive(Clone, Debug)]
struct Node {
    element: Element,
    parent: Option<usize>,
    children: Vec<usize>,
    bounds: Rect,
    clip: Option<Rect>,
    natural: [f32; 2],
    scroll: f32,
    scroll_max: f32,
    text: TextLayout,
    text_offset: f32,
}
#[derive(Clone, Copy, Debug)]
struct Capture {
    id: ElementId,
    origin: [f32; 2],
    offset: [f32; 2],
    value: f64,
    started: bool,
}
#[derive(Clone, Debug)]
struct MenuState {
    owner: ElementId,
    highlighted: Option<usize>,
    pressed: Option<usize>,
    scroll: f32,
    bounds: Rect,
}

/// Owns scene structure, widget values and input state. Storage is bounded by `MAX_ELEMENTS`.
pub struct Scene {
    nodes: Vec<Node>,
    order: Vec<usize>,
    indices: BTreeMap<ElementId, usize>,
    viewport: Rect,
    focus: Option<ElementId>,
    surface_focused: bool,
    hover: Option<ElementId>,
    capture: Option<Capture>,
    key_pressed: Option<ElementId>,
    pointer: [f32; 2],
    menu: Option<MenuState>,
    scopes: Vec<(ElementId, Option<ElementId>)>,
    measure_dirty: bool,
    layout_dirty: bool,
    paint_dirty: bool,
    list: DisplayList,
    stats: SceneStats,
    pub inspect: bool,
}
impl Scene {
    pub fn new(root: Element) -> Result<Self, UiError> {
        let mut result = Self {
            nodes: Vec::new(),
            order: Vec::new(),
            indices: BTreeMap::new(),
            viewport: Rect::default(),
            focus: None,
            surface_focused: true,
            hover: None,
            capture: None,
            key_pressed: None,
            pointer: [0.0; 2],
            menu: None,
            scopes: Vec::new(),
            measure_dirty: true,
            layout_dirty: true,
            paint_dirty: true,
            list: DisplayList::default(),
            stats: SceneStats::default(),
            inspect: false,
        };
        result.add(root, None, 0)?;
        Ok(result)
    }
    fn add(
        &mut self,
        mut element: Element,
        parent: Option<usize>,
        depth: usize,
    ) -> Result<usize, UiError> {
        if self.nodes.len() >= MAX_ELEMENTS {
            return Err(UiError::TooManyElements);
        }
        if depth >= MAX_DEPTH {
            return Err(UiError::TooDeep);
        }
        validate(&element)?;
        if self.indices.contains_key(&element.id) {
            return Err(UiError::DuplicateId(element.id));
        }
        let children = std::mem::take(&mut element.children);
        let index = self.nodes.len();
        self.indices.insert(element.id, index);
        self.nodes.push(Node {
            element,
            parent,
            children: Vec::new(),
            bounds: Rect::default(),
            clip: None,
            natural: [0.0; 2],
            scroll: 0.0,
            scroll_max: 0.0,
            text: TextLayout::default(),
            text_offset: 0.0,
        });
        for child in children {
            let i = self.add(child, Some(index), depth + 1)?;
            self.nodes[index].children.push(i);
        }
        Ok(index)
    }
    pub fn text_input_state(&self) -> Option<TextInputState> {
        if !self.surface_focused {
            return None;
        }
        let id = self.focus?;
        let node = &self.nodes[self.indices[&id]];
        let Widget::TextInput(edit) = &node.element.widget else {
            return None;
        };
        let x = node.text.caret(edit.cursor()).map_or(0.0, |c| c.x);
        Some(TextInputState {
            id,
            text: edit.text().to_owned(),
            cursor: edit.cursor(),
            anchor: edit.anchor(),
            cursor_rectangle: Rect::new(
                node.bounds.x + 10.0 + x - node.text_offset,
                node.bounds.y + 5.0,
                2.0,
                (node.bounds.height - 10.0).max(1.0),
            ),
        })
    }
    pub fn stats(&self) -> SceneStats {
        self.stats
    }
    pub fn focus(&self) -> Option<ElementId> {
        self.focus
    }
    pub fn capture(&self) -> Option<ElementId> {
        self.capture.map(|c| c.id)
    }
    pub fn bounds(&self, id: ElementId) -> Option<Rect> {
        self.indices.get(&id).map(|i| self.nodes[*i].bounds)
    }
    pub fn widget(&self, id: ElementId) -> Option<&Widget> {
        self.indices
            .get(&id)
            .map(|i| &self.nodes[*i].element.widget)
    }
    pub fn needs_frame(&self) -> bool {
        self.layout_dirty || self.paint_dirty
    }
    pub fn set_layout(&mut self, id: ElementId, layout: Layout) -> Result<(), UiError> {
        let i = *self.indices.get(&id).ok_or(UiError::MissingElement(id))?;
        let mut next = self.nodes[i].element.clone();
        next.layout = layout;
        validate(&next)?;
        self.nodes[i].element.layout = layout;
        self.invalidate_layout();
        Ok(())
    }
    pub fn set_style(&mut self, id: ElementId, style: Style) -> Result<(), UiError> {
        let i = *self.indices.get(&id).ok_or(UiError::MissingElement(id))?;
        let mut next = self.nodes[i].element.clone();
        next.style = style;
        validate(&next)?;
        self.nodes[i].element.style = next.style;
        self.invalidate_layout();
        Ok(())
    }
    pub fn set_widget(&mut self, id: ElementId, widget: Widget) -> Result<(), UiError> {
        let i = *self.indices.get(&id).ok_or(UiError::MissingElement(id))?;
        let mut next = self.nodes[i].element.clone();
        next.widget = widget;
        validate(&next)?;
        self.dismiss_within(i);
        self.cancel_if_within(i);
        self.nodes[i].element = next;
        self.invalidate_layout();
        Ok(())
    }
    pub fn set_enabled(&mut self, id: ElementId, enabled: bool) -> Result<(), UiError> {
        let i = *self.indices.get(&id).ok_or(UiError::MissingElement(id))?;
        if !enabled {
            self.dismiss_within(i);
            self.cancel_if_within(i);
        }
        self.nodes[i].element.enabled = enabled;
        self.paint_dirty = true;
        Ok(())
    }
    pub fn set_visible(&mut self, id: ElementId, visible: bool) -> Result<(), UiError> {
        let i = *self.indices.get(&id).ok_or(UiError::MissingElement(id))?;
        if !visible {
            self.dismiss_within(i);
            self.cancel_if_within(i);
        }
        self.nodes[i].element.visible = visible;
        self.invalidate_layout();
        Ok(())
    }
    /// Removing a subtree rebuilds the bounded index and drops all of its retained data.
    pub fn remove(&mut self, id: ElementId) -> Result<(), UiError> {
        let i = *self.indices.get(&id).ok_or(UiError::MissingElement(id))?;
        if i == 0 {
            return Err(UiError::InvalidLayout);
        }
        self.dismiss_within(i);
        self.cancel_if_within(i);
        let root = self.export_tree(0, Some(i), None);
        let next = Self::new(root)?;
        self.adopt(next);
        Ok(())
    }
    /// Atomically append a subtree, preserving state for existing element IDs.
    pub fn insert(&mut self, parent: ElementId, child: Element) -> Result<(), UiError> {
        let i = *self
            .indices
            .get(&parent)
            .ok_or(UiError::MissingElement(parent))?;
        let root = self.export_tree(0, None, Some((i, &child)));
        let next = Self::new(root)?;
        self.adopt(next);
        Ok(())
    }
    fn export_tree(
        &self,
        i: usize,
        removed: Option<usize>,
        added: Option<(usize, &Element)>,
    ) -> Element {
        let mut e = self.nodes[i].element.clone();
        e.children = self.nodes[i]
            .children
            .iter()
            .filter(|c| Some(**c) != removed)
            .map(|c| self.export_tree(*c, removed, added))
            .collect();
        if let Some((parent, child)) = added
            && parent == i
        {
            e.children.push(child.clone());
        }
        e
    }
    fn adopt(&mut self, mut next: Self) {
        for node in &mut next.nodes {
            if let Some(previous) = self.indices.get(&node.element.id).map(|i| &self.nodes[*i]) {
                node.scroll = previous.scroll;
                node.text_offset = previous.text_offset;
            }
        }
        next.focus = self.focus;
        next.surface_focused = self.surface_focused;
        next.hover = self.hover;
        next.capture = self.capture;
        next.key_pressed = self.key_pressed;
        next.pointer = self.pointer;
        next.menu = self.menu.take();
        next.scopes = std::mem::take(&mut self.scopes);
        next.stats = self.stats;
        next.inspect = self.inspect;
        *self = next;
    }
    pub fn open_popup(&mut self, id: ElementId) -> Result<(), UiError> {
        let i = *self.indices.get(&id).ok_or(UiError::MissingElement(id))?;
        if !matches!(self.nodes[i].element.widget, Widget::Popup { .. }) {
            return Err(UiError::InvalidLayout);
        }
        if self.scopes.iter().any(|(scope, _)| *scope == id) {
            return Ok(());
        }
        self.cancel_capture();
        self.hover = None;
        self.key_pressed = None;
        self.menu = None;
        self.scopes.push((id, self.focus));
        self.nodes[i].element.visible = true;
        self.focus = self.focus_candidates(Some(i)).first().copied();
        self.invalidate_layout();
        Ok(())
    }
    pub fn close_popup(&mut self) -> Option<ElementId> {
        let (id, previous) = self.scopes.pop()?;
        let i = self.indices[&id];
        self.cancel_if_within(i);
        self.nodes[i].element.visible = false;
        self.focus = previous.filter(|id| self.indices.get(id).is_some_and(|i| self.eligible(*i)));
        self.invalidate_layout();
        Some(id)
    }
    pub fn interaction(&self, id: ElementId) -> Option<InteractionState> {
        let i = *self.indices.get(&id)?;
        let node = &self.nodes[i];
        let selected = matches!(
            node.element.widget,
            Widget::Toggle { checked: true, .. }
                | Widget::Checkbox { checked: true, .. }
                | Widget::Selectable { selected: true, .. }
        );
        Some(InteractionState {
            hovered: self.hover == Some(id),
            focused: self.surface_focused && self.focus == Some(id),
            pressed: (self.capture.map(|c| c.id) == Some(id) && self.hover == Some(id))
                || self.key_pressed == Some(id),
            captured: self.capture.map(|c| c.id) == Some(id),
            disabled: !self.eligible(i),
            selected,
            expanded: self.menu.as_ref().is_some_and(|m| m.owner == id),
        })
    }
    fn invalidate_layout(&mut self) {
        self.measure_dirty = true;
        self.invalidate_positions();
    }
    fn invalidate_positions(&mut self) {
        self.layout_dirty = true;
        self.paint_dirty = true;
    }
    fn within(&self, mut child: usize, parent: usize) -> bool {
        loop {
            if child == parent {
                return true;
            }
            match self.nodes[child].parent {
                Some(i) => child = i,
                None => return false,
            }
        }
    }
    fn eligible(&self, mut i: usize) -> bool {
        loop {
            let e = &self.nodes[i].element;
            if !e.visible || !e.enabled {
                return false;
            }
            match self.nodes[i].parent {
                Some(p) => i = p,
                None => return true,
            }
        }
    }
    fn visible(&self, mut i: usize) -> bool {
        loop {
            if !self.nodes[i].element.visible {
                return false;
            }
            match self.nodes[i].parent {
                Some(p) => i = p,
                None => return true,
            }
        }
    }
    fn modal_root(&self) -> Option<usize> {
        self.scopes.iter().rev().find_map(|(id, _)| {
            let i = *self.indices.get(id)?;
            matches!(self.nodes[i].element.widget, Widget::Popup { modal: true }).then_some(i)
        })
    }
    fn focus_candidates(&self, root: Option<usize>) -> Vec<ElementId> {
        let root = root.or_else(|| self.modal_root());
        self.nodes
            .iter()
            .enumerate()
            .filter(|(i, n)| {
                n.element.widget.focusable()
                    && self.eligible(*i)
                    && root.is_none_or(|r| self.within(*i, r))
            })
            .map(|(_, n)| n.element.id)
            .collect()
    }
    fn dismiss_within(&mut self, parent: usize) {
        if let Some(first) = self
            .scopes
            .iter()
            .position(|(id, _)| self.within(self.indices[id], parent))
        {
            while self.scopes.len() > first {
                self.close_popup();
            }
        }
    }
    fn cancel_if_within(&mut self, parent: usize) {
        if self.capture.is_some_and(|c| {
            self.indices
                .get(&c.id)
                .is_some_and(|i| self.within(*i, parent))
        }) {
            self.cancel_capture();
        }
        if self
            .focus
            .is_some_and(|id| self.within(self.indices[&id], parent))
        {
            self.focus = None;
        }
        if self
            .hover
            .is_some_and(|id| self.within(self.indices[&id], parent))
        {
            self.hover = None;
        }
        if self
            .key_pressed
            .is_some_and(|id| self.within(self.indices[&id], parent))
        {
            self.key_pressed = None;
        }
        if self
            .menu
            .as_ref()
            .is_some_and(|m| self.within(self.indices[&m.owner], parent))
        {
            self.menu = None;
        }
    }
    fn build_paint_order(&self) -> Vec<usize> {
        let mut order = Vec::with_capacity(self.nodes.len());
        fn walk(s: &Scene, i: usize, order: &mut Vec<usize>) {
            if !s.nodes[i].element.visible {
                return;
            }
            order.push(i);
            for c in &s.nodes[i].children {
                if !matches!(s.nodes[*c].element.widget, Widget::Popup { .. }) {
                    walk(s, *c, order);
                }
            }
        }
        walk(self, 0, &mut order);
        for (id, _) in &self.scopes {
            walk(self, self.indices[id], &mut order);
        }
        order
    }
}
fn validate(e: &Element) -> Result<(), UiError> {
    let l = e.layout;
    if !e.style.radius.is_finite()
        || e.style.radius < 0.0
        || !e.style.text.font_size.is_finite()
        || e.style.text.font_size <= 0.0
        || !e.style.text.line_height.is_finite()
        || e.style.text.line_height <= 0.0
        || e.style.text.font_family.len() > 256
    {
        return Err(UiError::InvalidLayout);
    }
    if [l.padding, l.gap]
        .into_iter()
        .any(|v| !v.is_finite() || v < 0.0)
        || l.offset.iter().any(|v| !v.is_finite())
        || [l.width, l.height]
            .into_iter()
            .any(|v| matches!(v,Length::Px(x) if !x.is_finite()||x<0.0))
        || matches!(l.flow, Flow::Grid { columns: 0 })
    {
        return Err(UiError::InvalidLayout);
    }
    if e.tooltip.as_ref().is_some_and(|s| s.len() > MAX_TEXT_BYTES)
        || e.widget.label().len() > MAX_TEXT_BYTES
    {
        return Err(UiError::TextTooLong);
    }
    if let Widget::Paragraph(spans) = &e.widget
        && spans.iter().map(|s| s.text.len()).sum::<usize>() > MAX_TEXT_BYTES
    {
        return Err(UiError::TextTooLong);
    }
    if let Some(items) = e.widget.menu() {
        if items.len() > MAX_MENU_ITEMS {
            return Err(UiError::TooManyMenuItems);
        }
        let mut ids = std::collections::BTreeSet::new();
        for item in items {
            if let MenuItem::Option { id, label, .. } = item {
                if label.len() > MAX_TEXT_BYTES {
                    return Err(UiError::TextTooLong);
                }
                if !ids.insert(*id) {
                    return Err(UiError::InvalidLayout);
                }
            }
        }
    }
    Ok(())
}
fn contains(r: Rect, p: [f32; 2]) -> bool {
    r.is_finite_positive()
        && p[0] >= r.x
        && p[0] < r.x + r.width
        && p[1] >= r.y
        && p[1] < r.y + r.height
}
fn inset(r: Rect, p: f32) -> Rect {
    Rect::new(
        r.x + p,
        r.y + p,
        (r.width - 2.0 * p).max(0.0),
        (r.height - 2.0 * p).max(0.0),
    )
}
