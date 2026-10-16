//! Headless UI element tree and queries for Nudge Pack 3 (Local UI).
//! In-memory view tree, element queries, and synthetic event dispatch.

use std::collections::HashMap;

/// A node in the in-memory UI element tree.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiElement {
    pub tag: String,
    pub id: Option<String>,
    pub text: Option<String>,
    pub props: HashMap<String, String>,
    pub children: Vec<UiElement>,
    pub on_click: Option<String>,
    pub on_change: Option<String>,
    pub on_key: Option<String>,
}

impl UiElement {
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            id: None,
            text: None,
            props: HashMap::new(),
            children: Vec::new(),
            on_click: None,
            on_change: None,
            on_key: None,
        }
    }

    /// Recursively find an element with matching `id`.
    pub fn find_by_id(&self, target_id: &str) -> Option<&UiElement> {
        if self.id.as_deref() == Some(target_id) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find_by_id(target_id) {
                return Some(found);
            }
        }
        None
    }

    /// Recursively find a mutable reference to an element with matching `id`.
    pub fn find_by_id_mut(&mut self, target_id: &str) -> Option<&mut UiElement> {
        if self.id.as_deref() == Some(target_id) {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_by_id_mut(target_id) {
                return Some(found);
            }
        }
        None
    }

    /// Recursively find an element whose text equals or contains `query_text`.
    pub fn find_by_text(&self, query_text: &str) -> Option<&UiElement> {
        if let Some(ref t) = self.text {
            if t == query_text || t.contains(query_text) {
                return Some(self);
            }
        }
        for child in &self.children {
            if let Some(found) = child.find_by_text(query_text) {
                return Some(found);
            }
        }
        None
    }

    /// Collect all text content within this element and its descendants.
    pub fn get_text(&self) -> String {
        let mut out = String::new();
        if let Some(ref t) = self.text {
            out.push_str(t);
        }
        for child in &self.children {
            let child_txt = child.get_text();
            if !child_txt.is_empty() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&child_txt);
            }
        }
        out
    }
}

/// Interactive editable text buffer with cursor and selection support.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TextInputState {
    pub text: String,
    pub cursor: usize,
    pub selection_anchor: Option<usize>,
}

impl TextInputState {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.chars().count();
        Self {
            text,
            cursor,
            selection_anchor: None,
        }
    }

    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn has_selection(&self) -> bool {
        match self.selection_anchor {
            Some(anchor) => anchor != self.cursor,
            None => false,
        }
    }

    pub fn selection_range(&self) -> Option<(usize, usize)> {
        match self.selection_anchor {
            Some(anchor) if anchor != self.cursor => {
                Some((anchor.min(self.cursor), anchor.max(self.cursor)))
            }
            _ => None,
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        Some(self.text.chars().skip(start).take(end - start).collect())
    }

    pub fn delete_selection(&mut self) -> bool {
        if let Some((start, end)) = self.selection_range() {
            let prefix: String = self.text.chars().take(start).collect();
            let suffix: String = self.text.chars().skip(end).collect();
            self.text = format!("{prefix}{suffix}");
            self.cursor = start;
            self.selection_anchor = None;
            true
        } else {
            false
        }
    }

    pub fn insert_str(&mut self, s: &str) {
        self.delete_selection();
        let prefix: String = self.text.chars().take(self.cursor).collect();
        let suffix: String = self.text.chars().skip(self.cursor).collect();
        let insert_len = s.chars().count();
        self.text = format!("{prefix}{s}{suffix}");
        self.cursor += insert_len;
        self.selection_anchor = None;
    }

    pub fn insert_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.insert_str(c.encode_utf8(&mut buf));
    }

    pub fn backspace(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        if self.cursor > 0 {
            let start = self.cursor - 1;
            let prefix: String = self.text.chars().take(start).collect();
            let suffix: String = self.text.chars().skip(self.cursor).collect();
            self.text = format!("{prefix}{suffix}");
            self.cursor = start;
            self.selection_anchor = None;
            true
        } else {
            false
        }
    }

    pub fn delete(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        let total = self.char_len();
        if self.cursor < total {
            let prefix: String = self.text.chars().take(self.cursor).collect();
            let suffix: String = self.text.chars().skip(self.cursor + 1).collect();
            self.text = format!("{prefix}{suffix}");
            self.selection_anchor = None;
            true
        } else {
            false
        }
    }

    pub fn move_left(&mut self, extend_selection: bool) {
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
            self.cursor = self.cursor.saturating_sub(1);
        } else {
            if let Some((start, _)) = self.selection_range() {
                self.cursor = start;
            } else {
                self.cursor = self.cursor.saturating_sub(1);
            }
            self.selection_anchor = None;
        }
    }

    pub fn move_right(&mut self, extend_selection: bool) {
        let total = self.char_len();
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
            self.cursor = (self.cursor + 1).min(total);
        } else {
            if let Some((_, end)) = self.selection_range() {
                self.cursor = end;
            } else {
                self.cursor = (self.cursor + 1).min(total);
            }
            self.selection_anchor = None;
        }
    }

    pub fn move_home(&mut self, extend_selection: bool) {
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor = 0;
    }

    pub fn move_end(&mut self, extend_selection: bool) {
        let total = self.char_len();
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor = total;
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor = self.char_len();
    }

    pub fn cut(&mut self) -> Option<String> {
        let txt = self.selected_text();
        if txt.is_some() {
            self.delete_selection();
        }
        txt
    }

    pub fn copy(&self) -> Option<String> {
        self.selected_text()
    }

    pub fn paste(&mut self, s: &str) {
        self.insert_str(s);
    }
}

/// Headless in-memory mount for a window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeadlessMount {
    pub title: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub root: Option<UiElement>,
    pub scroll_x: i64,
    pub scroll_y: i64,
    pub focused_id: Option<String>,
    pub text_inputs: HashMap<String, TextInputState>,
}

impl HeadlessMount {
    pub fn new(title: impl Into<String>, root: Option<UiElement>) -> Self {
        Self {
            title: title.into(),
            width: None,
            height: None,
            root,
            scroll_x: 0,
            scroll_y: 0,
            focused_id: None,
            text_inputs: HashMap::new(),
        }
    }

    pub fn find(&self, id: &str) -> Option<&UiElement> {
        self.root.as_ref().and_then(|r| r.find_by_id(id))
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut UiElement> {
        self.root.as_mut().and_then(|r| r.find_by_id_mut(id))
    }

    pub fn find_text(&self, text: &str) -> Option<&UiElement> {
        self.root.as_ref().and_then(|r| r.find_by_text(text))
    }

    pub fn focus(&mut self, id: impl Into<String>) {
        self.focused_id = Some(id.into());
    }

    pub fn blur(&mut self) {
        self.focused_id = None;
    }

    pub fn scroll(&mut self, dx: i64, dy: i64) {
        self.scroll_x = (self.scroll_x + dx).max(0);
        self.scroll_y = (self.scroll_y + dy).max(0);
    }

    pub fn dispatch_scroll(&mut self, dx: i64, dy: i64) {
        self.scroll(dx, dy);
    }

    pub fn scroll_offset(&self) -> (i64, i64) {
        (self.scroll_x, self.scroll_y)
    }

    /// Ensure text input state exists for the target id (syncing from element text/value).
    pub fn ensure_input_state(&mut self, id: &str) -> &mut TextInputState {
        if !self.text_inputs.contains_key(id) {
            let initial = self
                .find(id)
                .and_then(|e| e.text.clone().or_else(|| e.props.get("value").cloned()))
                .unwrap_or_default();
            self.text_inputs
                .insert(id.to_string(), TextInputState::new(initial));
        }
        self.text_inputs.get_mut(id).unwrap()
    }

    /// Type text into an input element and return (handler_name, new_text) if on_change exists.
    pub fn dispatch_type(&mut self, id: &str, s: &str) -> (Option<String>, String) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.insert_str(s);
        let new_val = state.text.clone();

        let handler = if let Some(elem) = self.find_mut(id) {
            elem.text = Some(new_val.clone());
            elem.props.insert("value".to_string(), new_val.clone());
            elem.on_change.clone()
        } else {
            None
        };
        (handler, new_val)
    }

    /// Send backspace to an input element and return (handler_name, new_text).
    pub fn dispatch_backspace(&mut self, id: &str) -> (Option<String>, String) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.backspace();
        let new_val = state.text.clone();

        let handler = if let Some(elem) = self.find_mut(id) {
            elem.text = Some(new_val.clone());
            elem.props.insert("value".to_string(), new_val.clone());
            elem.on_change.clone()
        } else {
            None
        };
        (handler, new_val)
    }

    /// Dispatch a key event to the focused element or root and return the triggered handler name.
    pub fn dispatch_key(&self, _key: &str) -> Option<String> {
        if let Some(ref fid) = self.focused_id {
            if let Some(elem) = self.find(fid) {
                if let Some(ref h) = elem.on_key {
                    return Some(h.clone());
                }
            }
        }
        // Fall back to root element on_key
        self.root.as_ref().and_then(|r| r.on_key.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_element_tree_queries() {
        let mut col = UiElement::new("column");
        let mut txt = UiElement::new("text");
        txt.text = Some("Count: 0".to_string());
        let mut btn = UiElement::new("button");
        btn.id = Some("btn_inc".to_string());
        btn.text = Some("Increment".to_string());
        btn.on_click = Some("inc".to_string());
        col.children.push(txt);
        col.children.push(btn);

        let mount = HeadlessMount::new("Counter App", Some(col));
        assert!(mount.find("btn_inc").is_some());
        assert_eq!(
            mount.find("btn_inc").unwrap().on_click.as_deref(),
            Some("inc")
        );
        assert!(mount.find_text("Count: 0").is_some());
        assert!(mount.find_text("Nonexistent").is_none());
        assert_eq!(
            mount.root.as_ref().unwrap().get_text(),
            "Count: 0 Increment"
        );
    }

    #[test]
    fn test_text_input_editing_and_selection() {
        let mut input = TextInputState::new("hello");
        assert_eq!(input.cursor, 5);
        assert_eq!(input.char_len(), 5);

        // Insert text at end
        input.insert_str(" world");
        assert_eq!(input.text, "hello world");
        assert_eq!(input.cursor, 11);

        // Move left 6 chars
        for _ in 0..6 {
            input.move_left(false);
        }
        assert_eq!(input.cursor, 5);

        // Backspace
        assert!(input.backspace());
        assert_eq!(input.text, "hell world");
        assert_eq!(input.cursor, 4);

        // Move home
        input.move_home(false);
        assert_eq!(input.cursor, 0);

        // Selection: select first 4 chars ("hell")
        for _ in 0..4 {
            input.move_right(true);
        }
        assert!(input.has_selection());
        assert_eq!(input.selected_text().as_deref(), Some("hell"));

        // Copy and cut
        let copied = input.copy();
        assert_eq!(copied.as_deref(), Some("hell"));

        let cut_val = input.cut();
        assert_eq!(cut_val.as_deref(), Some("hell"));
        assert_eq!(input.text, " world");
        assert_eq!(input.cursor, 0);

        // Paste back
        input.paste("brave new");
        assert_eq!(input.text, "brave new world");

        // Select all and replace
        input.select_all();
        assert_eq!(input.selected_text().as_deref(), Some("brave new world"));
        input.insert_str("fresh");
        assert_eq!(input.text, "fresh");
        assert_eq!(input.cursor, 5);
    }

    #[test]
    fn test_headless_mount_interactive_dispatch() {
        let mut col = UiElement::new("column");
        let mut inp = UiElement::new("input");
        inp.id = Some("note_input".to_string());
        inp.props.insert("value".to_string(), "initial".to_string());
        inp.on_change = Some("handle_change".to_string());
        inp.on_key = Some("handle_key".to_string());
        col.children.push(inp);

        let mut mount = HeadlessMount::new("Test App", Some(col));

        // Test typing dispatch
        let (handler, new_val) = mount.dispatch_type("note_input", " edit");
        assert_eq!(handler.as_deref(), Some("handle_change"));
        assert_eq!(new_val, "initial edit");
        assert_eq!(
            mount
                .find("note_input")
                .unwrap()
                .props
                .get("value")
                .unwrap(),
            "initial edit"
        );

        // Test backspace dispatch
        let (handler, after_bs) = mount.dispatch_backspace("note_input");
        assert_eq!(handler.as_deref(), Some("handle_change"));
        assert_eq!(after_bs, "initial edi");

        // Test scrolling
        assert_eq!(mount.scroll_offset(), (0, 0));
        mount.dispatch_scroll(10, 50);
        assert_eq!(mount.scroll_offset(), (10, 50));
        mount.dispatch_scroll(-5, -20);
        assert_eq!(mount.scroll_offset(), (5, 30));

        // Test keyboard handler dispatch
        let key_handler = mount.dispatch_key("Enter");
        assert_eq!(key_handler.as_deref(), Some("handle_key"));
    }
}
