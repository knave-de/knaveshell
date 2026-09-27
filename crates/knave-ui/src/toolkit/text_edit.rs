use super::{MAX_TEXT_BYTES, UiError};
use unicode_segmentation::UnicodeSegmentation;

/// Single-line UTF-8 editing state. Selection endpoints are grapheme boundaries.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit {
    text: String,
    cursor: usize,
    anchor: usize,
    pub(crate) preedit: String,
}
impl TextEdit {
    pub fn new(text: impl Into<String>) -> Result<Self, UiError> {
        let text = text.into();
        if text.len() > MAX_TEXT_BYTES {
            return Err(UiError::TextTooLong);
        }
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        Ok(Self {
            cursor: text.len(),
            anchor: text.len(),
            text,
            preedit: String::new(),
        })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn anchor(&self) -> usize {
        self.anchor
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }
    pub fn selection(&self) -> std::ops::Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection()]
    }
    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }
    pub fn place(&mut self, byte: usize, extend: bool) {
        self.cursor = self
            .text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([self.text.len()])
            .take_while(|i| *i <= byte)
            .last()
            .unwrap_or(0);
        if !extend {
            self.anchor = self.cursor;
        }
    }
    pub fn move_cursor(&mut self, right: bool, extend: bool) {
        let range = self.selection();
        if !extend && !range.is_empty() {
            self.place(if right { range.end } else { range.start }, false);
            return;
        }
        let next = if right {
            self.text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|i| *i > self.cursor)
                .unwrap_or(self.text.len())
        } else {
            self.text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|i| *i < self.cursor)
                .last()
                .unwrap_or(0)
        };
        self.place(next, extend);
    }
    pub fn insert(&mut self, text: &str) -> bool {
        let filtered: String = text.chars().filter(|c| !c.is_control()).collect();
        let range = self.selection();
        if self.text.len() - range.len() + filtered.len() > MAX_TEXT_BYTES {
            return false;
        }
        if filtered.is_empty() && range.is_empty() {
            return false;
        }
        self.text.replace_range(range.clone(), &filtered);
        self.cursor = range.start + filtered.len();
        self.anchor = self.cursor;
        self.preedit.clear();
        true
    }
    pub fn delete(&mut self, forward: bool) -> bool {
        if self.selection().is_empty() {
            self.move_cursor(forward, true);
        }
        self.insert("")
    }
    pub fn delete_surrounding(&mut self, before: u32, after: u32) -> bool {
        let Some(start) = self.cursor.checked_sub(before as usize) else {
            return false;
        };
        let Some(end) = self
            .cursor
            .checked_add(after as usize)
            .filter(|end| *end <= self.text.len())
        else {
            return false;
        };
        if !self.text.is_char_boundary(start) || !self.text.is_char_boundary(end) || start == end {
            return false;
        }
        self.text.replace_range(start..end, "");
        self.cursor = start;
        self.anchor = start;
        true
    }
    pub fn set_preedit(&mut self, text: &str) -> Result<(), UiError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(UiError::TextTooLong);
        }
        self.preedit = text.chars().filter(|c| !c.is_control()).collect();
        Ok(())
    }
}
