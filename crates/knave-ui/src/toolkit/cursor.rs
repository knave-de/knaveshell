use super::*;

impl Scene {
    /// Resolve from current geometry, including stationary pointers after a layout change.
    /// Capture keeps text selection, slider and drag cursors stable outside their bounds.
    pub fn cursor(&self) -> CursorShape {
        if let Some(capture) = self.capture {
            return self.node_cursor(self.indices[&capture.id], true);
        }
        if !self.pointer_inside {
            return CursorShape::Default;
        }
        if let Some(menu) = &self.menu {
            if !contains(menu.bounds, self.pointer) {
                return CursorShape::Default;
            }
            let items = self.nodes[self.indices[&menu.owner]]
                .element
                .widget
                .menu()
                .unwrap_or_default();
            let mut y = menu.bounds.y - menu.scroll;
            for item in items {
                let height = super::input::row_height(item);
                if self.pointer[1] >= y && self.pointer[1] < y + height {
                    return match item {
                        MenuItem::Option { enabled: true, .. } => CursorShape::Pointer,
                        MenuItem::Option { enabled: false, .. } => CursorShape::NotAllowed,
                        MenuItem::Separator => CursorShape::Default,
                    };
                }
                y += height;
            }
            return CursorShape::Default;
        }
        // A popup's outside press only dismisses it, never operates the background.
        if let Some((id, _)) = self.scopes.last()
            && !contains(self.nodes[self.indices[id]].bounds, self.pointer)
        {
            return CursorShape::Default;
        }
        self.top_node(self.pointer)
            .map_or(CursorShape::Default, |i| self.node_cursor(i, false))
    }

    pub fn set_cursor(
        &mut self,
        id: ElementId,
        cursor: Option<CursorShape>,
    ) -> Result<(), UiError> {
        let i = *self.indices.get(&id).ok_or(UiError::MissingElement(id))?;
        if self.nodes[i].element.cursor != cursor {
            self.nodes[i].element.cursor = cursor;
            self.paint_dirty = true;
        }
        Ok(())
    }

    fn node_cursor(&self, mut i: usize, captured: bool) -> CursorShape {
        loop {
            if !self.eligible(i) {
                return CursorShape::NotAllowed;
            }
            let node = &self.nodes[i];
            if let Some(cursor) = node.element.cursor {
                return cursor;
            }
            match &node.element.widget {
                Widget::TextInput(_) => return CursorShape::Text,
                Widget::Draggable(_) => {
                    return if captured {
                        CursorShape::Grabbing
                    } else {
                        CursorShape::Grab
                    };
                }
                Widget::Slider(slider) => {
                    return match slider.orientation {
                        Orientation::Horizontal => CursorShape::EwResize,
                        Orientation::Vertical => CursorShape::NsResize,
                    };
                }
                widget if widget.focusable() => return CursorShape::Pointer,
                _ => {}
            }
            let Some(parent) = node.parent else {
                return CursorShape::Default;
            };
            i = parent;
        }
    }
}
