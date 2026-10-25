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
    pub undo_stack: Vec<(String, usize)>,
    pub redo_stack: Vec<(String, usize)>,
}

impl TextInputState {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.chars().count();
        Self {
            text,
            cursor,
            selection_anchor: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn push_undo_snapshot(&mut self) {
        self.undo_stack.push((self.text.clone(), self.cursor));
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> bool {
        if let Some((prev_text, prev_cursor)) = self.undo_stack.pop() {
            self.redo_stack.push((self.text.clone(), self.cursor));
            self.text = prev_text;
            self.cursor = prev_cursor.min(self.char_len());
            self.selection_anchor = None;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some((next_text, next_cursor)) = self.redo_stack.pop() {
            self.undo_stack.push((self.text.clone(), self.cursor));
            if self.undo_stack.len() > 50 {
                self.undo_stack.remove(0);
            }
            self.text = next_text;
            self.cursor = next_cursor.min(self.char_len());
            self.selection_anchor = None;
            true
        } else {
            false
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.push_undo_snapshot();
        self.text = text.into();
        self.cursor = self.cursor.min(self.char_len());
        self.selection_anchor = None;
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

    fn insert_str_raw(&mut self, s: &str) {
        self.delete_selection();
        let prefix: String = self.text.chars().take(self.cursor).collect();
        let suffix: String = self.text.chars().skip(self.cursor).collect();
        let insert_len = s.chars().count();
        self.text = format!("{prefix}{s}{suffix}");
        self.cursor += insert_len;
        self.selection_anchor = None;
    }

    pub fn insert_str(&mut self, s: &str) {
        self.push_undo_snapshot();
        self.insert_str_raw(s);
    }

    pub fn insert_char(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.insert_str(c.encode_utf8(&mut buf));
    }

    pub fn backspace(&mut self) -> bool {
        if !self.has_selection() && self.cursor == 0 {
            return false;
        }
        self.push_undo_snapshot();
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
        if !self.has_selection() && self.cursor >= self.char_len() {
            return false;
        }
        self.push_undo_snapshot();
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
        let (line, _) = self.line_and_col_of(self.cursor);
        let line_start = self.char_idx_for_line_col(line, 0);
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor = line_start;
    }

    pub fn move_end(&mut self, extend_selection: bool) {
        let (line, _) = self.line_and_col_of(self.cursor);
        let line_end = self.char_idx_for_line_col(line, self.line_len(line));
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor = line_end;
    }

    pub fn move_doc_start(&mut self, extend_selection: bool) {
        if extend_selection {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
        } else {
            self.selection_anchor = None;
        }
        self.cursor = 0;
    }

    pub fn move_doc_end(&mut self, extend_selection: bool) {
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

    pub fn line_and_col_of(&self, char_idx: usize) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;
        for c in self.text.chars().take(char_idx) {
            if c == '\n' {
                line += 1;
                col = 0;
            } else if c != '\r' {
                col += 1;
            }
        }
        (line, col)
    }

    pub fn char_idx_for_line_col(&self, target_line: usize, target_col: usize) -> usize {
        let mut curr_line = 0;
        let mut curr_col = 0;
        let mut idx = 0;
        for c in self.text.chars() {
            if curr_line == target_line {
                if c == '\n' || c == '\r' || curr_col == target_col {
                    return idx;
                }
                if c != '\r' {
                    curr_col += 1;
                }
            } else if c == '\n' {
                curr_line += 1;
                curr_col = 0;
            }
            idx += 1;
        }
        idx
    }

    pub fn lines_count(&self) -> usize {
        self.text.split('\n').count()
    }

    pub fn line_len(&self, line_idx: usize) -> usize {
        self.text
            .split('\n')
            .nth(line_idx)
            .map(|l| l.trim_end_matches('\r').chars().count())
            .unwrap_or(0)
    }

    pub fn move_up(&mut self, extend_selection: bool) {
        let (line, col) = self.line_and_col_of(self.cursor);
        if line == 0 {
            self.move_home(extend_selection);
        } else {
            let target_line = line - 1;
            let target_col = col.min(self.line_len(target_line));
            let new_cursor = self.char_idx_for_line_col(target_line, target_col);
            if extend_selection {
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some(self.cursor);
                }
            } else {
                self.selection_anchor = None;
            }
            self.cursor = new_cursor;
        }
    }

    pub fn move_down(&mut self, extend_selection: bool) {
        let total_lines = self.lines_count();
        let (line, col) = self.line_and_col_of(self.cursor);
        if line + 1 >= total_lines {
            self.move_end(extend_selection);
        } else {
            let target_line = line + 1;
            let target_col = col.min(self.line_len(target_line));
            let new_cursor = self.char_idx_for_line_col(target_line, target_col);
            if extend_selection {
                if self.selection_anchor.is_none() {
                    self.selection_anchor = Some(self.cursor);
                }
            } else {
                self.selection_anchor = None;
            }
            self.cursor = new_cursor;
        }
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor = self.char_len();
    }

    pub fn cut(&mut self) -> Option<String> {
        let txt = self.selected_text();
        if txt.is_some() {
            self.push_undo_snapshot();
            self.delete_selection();
        }
        txt
    }

    pub fn copy(&self) -> Option<String> {
        self.selected_text()
    }

    pub fn paste(&mut self, s: &str) {
        self.push_undo_snapshot();
        self.insert_str_raw(s);
    }
}

/// Estimate the bounding layout size (width, height) of a UiElement.
pub fn estimate_element_size(elem: &UiElement, max_w: i64) -> (i64, i64) {
    match elem.tag.as_str() {
        "column" => {
            let padding = elem
                .props
                .get("padding")
                .and_then(|s| s.parse().ok())
                .unwrap_or(16);
            let gap = elem
                .props
                .get("gap")
                .and_then(|s| s.parse().ok())
                .unwrap_or(10);

            let mut child_y = padding;
            let mut max_child_w = 0;
            let child_max_w = (max_w - 2 * padding).max(0);

            for child in &elem.children {
                let (cw, ch) = estimate_element_size(child, child_max_w);
                max_child_w = max_child_w.max(cw);
                child_y += ch + gap;
            }

            let total_h = if elem.children.is_empty() {
                2 * padding
            } else {
                child_y - gap + padding
            };
            let total_w = max_child_w + 2 * padding;
            (total_w, total_h)
        }
        "row" => {
            let padding = elem
                .props
                .get("padding")
                .and_then(|s| s.parse().ok())
                .unwrap_or(16);
            let gap = elem
                .props
                .get("gap")
                .and_then(|s| s.parse().ok())
                .unwrap_or(10);

            let mut child_x = padding;
            let mut max_child_h = 0;

            for child in &elem.children {
                let (cw, ch) = estimate_element_size(child, max_w);
                max_child_h = max_child_h.max(ch);
                child_x += cw + gap;
            }

            let total_w = if elem.children.is_empty() {
                2 * padding
            } else {
                child_x - gap + padding
            };
            let total_h = max_child_h + 2 * padding;
            (total_w, total_h)
        }
        "text" => {
            let text = elem.text.as_deref().unwrap_or("");
            let lines = text.split('\n').count().max(1) as i64;
            let max_line_len = text
                .split('\n')
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0) as i64;
            let w = (max_line_len * 16).min(max_w);
            (w, lines * 24)
        }
        "bubble" => {
            // Same wrapping budget as the native renderer: the message wraps
            // to a max width and the box grows by its padding on both sides.
            let pad: i64 = elem
                .props
                .get("padding")
                .and_then(|s| s.parse().ok())
                .unwrap_or(10);
            let max_width: i64 = elem
                .props
                .get("max_width")
                .and_then(|s| s.parse().ok())
                .unwrap_or(340)
                .min((max_w - 2 * pad).max(0))
                .max(2 * pad + 16);
            let text = elem.text.as_deref().unwrap_or("");
            let max_chars = ((max_width - 2 * pad) / 16).max(1);

            let mut lines = 0i64;
            let mut widest = 0i64;
            for para in text.split('\n') {
                let mut cur = 0i64;
                for word in para.split_whitespace() {
                    let wl = word.chars().count() as i64;
                    if cur == 0 {
                        cur = wl;
                    } else if cur + 1 + wl <= max_chars {
                        cur += 1 + wl;
                    } else {
                        widest = widest.max(cur);
                        lines += 1;
                        cur = wl;
                    }
                }
                widest = widest.max(cur);
                lines += 1;
            }
            let lines = lines.max(1);
            let w = (widest * 16 + 2 * pad).min(max_width);
            (w, lines * 24 + 2 * pad)
        }
        "button" => {
            let len = elem.text.as_ref().map(|t| t.chars().count()).unwrap_or(0) as i64;
            let w = (len * 16 + 32).max(100).min(max_w);
            (w, 36)
        }
        "input" => {
            let multiline = elem
                .props
                .get("multiline")
                .map(|s| s == "true")
                .unwrap_or(false);
            if multiline {
                let text = elem.text.as_deref().unwrap_or("");
                let lines = text.split('\n').count().max(3) as i64;
                (max_w, lines * 24 + 16)
            } else {
                (max_w, 40)
            }
        }
        "select" => {
            // As wide as the widest thing it can ever show — the current label
            // or any option — so opening the list never widens the field and a
            // row never runs past the box that chose it. The per-character
            // budget is the generous one the `text` arm uses, deliberately
            // wider than the native renderer's own measure: an estimate that
            // overshoots leaves room, and one that undershoots clips.
            //
            // Height is fixed. The list is an overlay, never part of the flow,
            // so a longer option list must not grow the form around it.
            let current = elem.text.as_deref().unwrap_or("");
            let widest_opt = elem
                .props
                .get("options")
                .map(|o| o.split('\n').map(|l| l.chars().count()).max().unwrap_or(0))
                .unwrap_or(0);
            let chars = current.chars().count().max(widest_opt) as i64;
            let w = (chars * 16 + 56).min(max_w).max(140);
            (w, 40)
        }
        "checkbox" | "toggle" => {
            // The control, a gap, then the label beside it — the same shape
            // the native renderer lays out, and the same generous
            // per-character budget the `text` arm uses so an overshoot leaves
            // room instead of clipping the words.
            //
            // Height is fixed for both: neither control grows when the label
            // is long, it just widens.
            let control_w: i64 = if elem.tag == "toggle" { 44 } else { 18 };
            let label = elem.text.as_deref().unwrap_or("");
            let chars = label.chars().count() as i64;
            let w = (control_w + 10 + chars * 16 + 8)
                .min(max_w)
                .max(control_w + 18);
            (w, 34)
        }
        "slider" => {
            // A track of fixed length, a gap, then the label beside it — the
            // same shape the flag controls lay out, and the same generous
            // per-character budget the `text` arm uses so an overshoot leaves
            // room instead of clipping the words.
            //
            // The track does not stretch with its bounds the way a text field
            // does: a 1000-wide window and a 200-wide one should show the same
            // dial, not a different one.
            let label = elem.text.as_deref().unwrap_or("");
            let chars = label.chars().count() as i64;
            let w = (SLIDER_TRACK_W as i64 + 10 + chars * 16 + 8)
                .min(max_w)
                .max(SLIDER_TRACK_W as i64 + 18);
            (w, 34)
        }
        _ => (0, 0),
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

    pub fn content_size(&self) -> (i64, i64) {
        if let Some(ref root) = self.root {
            let max_w = self.width.unwrap_or(800) - 20;
            estimate_element_size(root, max_w)
        } else {
            (0, 0)
        }
    }

    pub fn max_scroll(&self) -> (i64, i64) {
        let (cw, ch) = self.content_size();
        let vw = self.width.unwrap_or(800);
        let vh = self.height.unwrap_or(600);
        let max_x = (cw - vw + 20).max(0);
        let max_y = (ch - vh + 20).max(0);
        (max_x, max_y)
    }

    pub fn scroll(&mut self, dx: i64, dy: i64) {
        let (max_x, max_y) = self.max_scroll();
        let target_x = self.scroll_x + dx;
        let target_y = self.scroll_y + dy;
        self.scroll_x = if self.width.is_some() || max_x > 0 {
            target_x.clamp(0, max_x)
        } else {
            target_x.max(0)
        };
        self.scroll_y = if self.height.is_some() || max_y > 0 {
            target_y.clamp(0, max_y)
        } else {
            target_y.max(0)
        };
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

    /// Dispatch change event to an input element and return (handler_name, new_text).
    pub fn dispatch_change(&mut self, id: &str, new_value: &str) -> (Option<String>, String) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.set_text(new_value);
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

    /// Send delete to an input element and return (handler_name, new_text).
    pub fn dispatch_delete(&mut self, id: &str) -> (Option<String>, String) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.delete();
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

    /// Send newline (enter) to an input element and return (handler_name, new_text).
    pub fn dispatch_newline(&mut self, id: &str) -> (Option<String>, String) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.insert_newline();
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

    pub fn dispatch_cursor_left(&mut self, id: &str, extend_selection: bool) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.move_left(extend_selection);
    }

    pub fn dispatch_cursor_right(&mut self, id: &str, extend_selection: bool) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.move_right(extend_selection);
    }

    pub fn dispatch_cursor_up(&mut self, id: &str, extend_selection: bool) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.move_up(extend_selection);
    }

    pub fn dispatch_cursor_down(&mut self, id: &str, extend_selection: bool) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.move_down(extend_selection);
    }

    pub fn dispatch_cursor_home(&mut self, id: &str, extend_selection: bool) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.move_home(extend_selection);
    }

    pub fn dispatch_cursor_end(&mut self, id: &str, extend_selection: bool) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.move_end(extend_selection);
    }

    pub fn dispatch_select_all(&mut self, id: &str) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.select_all();
    }

    pub fn dispatch_cut(&mut self, id: &str) -> (Option<String>, Option<String>) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        let cut_val = state.cut();
        let new_val = state.text.clone();
        let handler = if let Some(elem) = self.find_mut(id) {
            elem.text = Some(new_val.clone());
            elem.props.insert("value".to_string(), new_val);
            elem.on_change.clone()
        } else {
            None
        };
        (handler, cut_val)
    }

    pub fn dispatch_paste(&mut self, id: &str, s: &str) -> (Option<String>, String) {
        self.focus(id);
        let state = self.ensure_input_state(id);
        state.paste(s);
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

    /// Dispatch a key event directly to a target element or fallback to focused/root.
    pub fn dispatch_key_to(&self, target_id: Option<&str>, _key: &str) -> Option<String> {
        if let Some(tid) = target_id {
            if let Some(elem) = self.find(tid) {
                if let Some(ref h) = elem.on_key {
                    return Some(h.clone());
                }
            }
        }
        self.dispatch_key(_key)
    }
}

/// Whether a stored string means "on". Every boolean prop the renderer reads
/// (`disabled`, a control's `value`) answers to this one spelling, so a form
/// never has to remember which words this frame happens to accept.
pub fn is_truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on"
    )
}

/// The word a boolean control's `value` prop carries. Spelled the way
/// `is_truthy` already reads "on", so the two can never disagree about the
/// same string — one writes what the other parses.
pub fn flag_str(on: bool) -> &'static str {
    if on {
        "true"
    } else {
        "false"
    }
}

/// How many decimal places a slider's `step` speaks in. Derived from the
/// step's own shortest spelling, so `step = 0.5` is one place wide and
/// `step = 1` is none — which is what decides whether the control's values
/// are written with a decimal point at all.
fn step_decimals(step: f64) -> usize {
    // Not a usable step: nothing to derive a precision from.
    if !(step.is_finite() && step > 0.0) {
        return 0;
    }
    match format!("{step}").split_once('.') {
        Some((_, frac)) => frac.chars().count().min(6),
        None => 0,
    }
}

/// Length of a slider's track. Fixed rather than proportional to the row, so
/// a slider looks like the same dial in a narrow window as in a wide one —
/// only the label gets to grow with the text.
pub const SLIDER_TRACK_W: i32 = 160;
/// Height of the track itself. Wide enough to read as a groove the handle
/// sits in, narrow enough to stay a detail beside its label.
pub const SLIDER_TRACK_H: i32 = 8;
/// Side of the square the handle is drawn as, then rounded to a circle by
/// its own radius in the renderer.
pub const SLIDER_KNOB: i32 = 18;
/// Height of a whole slider row — track plus its label's line.
pub const SLIDER_H: i32 = 34;

/// Where a drag lands: clamped to `[min, max]`, then pulled onto the nearest
/// rung of `step` counted from `min`, so a slider never rests between two
/// values the program cannot tell apart.
pub fn slider_snap(value: f64, min: f64, max: f64, step: f64) -> f64 {
    if max <= min {
        return min;
    }
    let v = value.clamp(min, max);
    // An un-runnable step is not an error: the value simply stays where it
    // was clamped. A NaN step included, which is the case a plain `> 0.0`
    // would silently let through by comparing false.
    if !(step.is_finite() && step > 0.0) {
        return v;
    }
    let rung = (min + ((v - min) / step).round() * step).clamp(min, max);
    // Shake off the dust a float leaves on a rung (`0.1 * 3` is not exactly
    // `0.3`), so the same position always produces the same bits. Without
    // this, two neighbouring pixels that both belong to one rung can differ
    // in the last place and be read as two different values — which is a drag
    // that reports the same number twice.
    if rung.abs() < 1e9 {
        ((rung * 1e9).round() / 1e9).clamp(min, max)
    } else {
        rung
    }
}

/// The word a slider's `value` prop carries, and the value a handler is
/// handed — one spelling for both, so a control can never report a number the
/// state was not written with.
///
/// The decimal places come from the *step*, not from wherever the drag
/// happened to stop: a `step = 1` budget therefore always writes `1500` and
/// always hands its handler an `Int`, while a `step = 0.1` temperature always
/// writes `0.7` and always hands its handler a `Float`. A value whose type
/// flipped mid-drag would make `state.budget = v` type-jitter for no reason
/// the programmer wrote down. Rounding to the step's own precision is also
/// what keeps a float's dust (`0.30000000000000004`) out of state the next
/// read will print.
pub fn slider_num_str(value: f64, step: f64) -> String {
    format!("{:.*}", step_decimals(step), value)
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

    #[test]
    fn test_multiline_editing_and_cursor_navigation() {
        let mut input = TextInputState::new("line1\nline2\nline3");
        assert_eq!(input.lines_count(), 3);
        assert_eq!(input.cursor, 17); // End of line 3
        assert_eq!(input.line_and_col_of(input.cursor), (2, 5));

        // Move up to line 2 (at col 5)
        input.move_up(false);
        assert_eq!(input.line_and_col_of(input.cursor), (1, 5));
        assert_eq!(input.cursor, 11);

        // Move to start of line 2
        input.move_home(false);
        assert_eq!(input.line_and_col_of(input.cursor), (1, 0));
        assert_eq!(input.cursor, 6);

        // Move to end of line 2
        input.move_end(false);
        assert_eq!(input.line_and_col_of(input.cursor), (1, 5));
        assert_eq!(input.cursor, 11);

        // Move up to line 1
        input.move_up(false);
        assert_eq!(input.line_and_col_of(input.cursor), (0, 5));

        // Move up again at top line -> goes to home
        input.move_up(false);
        assert_eq!(input.cursor, 0);

        // Insert newline in between
        input.insert_str("new ");
        input.insert_newline();
        assert_eq!(input.lines_count(), 4);
        assert_eq!(input.text, "new \nline1\nline2\nline3");

        // Move down across lines
        input.move_down(false);
        assert_eq!(input.line_and_col_of(input.cursor), (2, 0));

        // Select down with shift
        input.move_down(true);
        assert!(input.has_selection());
        let (start, end) = input.selection_range().unwrap();
        assert!(end > start);
    }

    #[test]
    fn test_headless_mount_scroll_clamping() {
        let mut col = UiElement::new("column");
        for i in 0..20 {
            let mut txt = UiElement::new("text");
            txt.text = Some(format!("Row {i}: long text item for height and width"));
            col.children.push(txt);
        }

        let mut mount = HeadlessMount::new("Bounded App", Some(col));
        mount.width = Some(300);
        mount.height = Some(200);

        let (_cw, ch) = mount.content_size();
        assert!(
            ch > 200,
            "content height must exceed viewport 200 (got {ch})"
        );
        let (max_x, max_y) = mount.max_scroll();
        assert!(max_y > 0, "max_y scroll must be positive");

        // Scroll within bounds
        mount.scroll(0, 50);
        assert_eq!(mount.scroll_offset(), (0, 50));

        // Scroll past max -> clamped to max_y
        mount.scroll(0, max_y + 500);
        assert_eq!(mount.scroll_y, max_y);

        // Scroll negative -> clamped to 0
        mount.scroll(0, -(max_y + 500));
        assert_eq!(mount.scroll_y, 0);

        // Horizontal clamping
        mount.scroll(max_x + 100, 0);
        assert_eq!(mount.scroll_x, max_x);
        mount.scroll(-1000, 0);
        assert_eq!(mount.scroll_x, 0);
    }

    #[test]
    fn test_crlf_column_drift_and_line_lengths() {
        let input = TextInputState::new("line0\r\nline1\r\nline2");
        assert_eq!(input.lines_count(), 3);
        assert_eq!(input.line_len(0), 5);
        assert_eq!(input.line_len(1), 5);
        assert_eq!(input.line_len(2), 5);

        // Column counting ignores \r
        assert_eq!(input.line_and_col_of(0), (0, 0));
        assert_eq!(input.line_and_col_of(5), (0, 5));
        assert_eq!(input.line_and_col_of(6), (0, 5));
        assert_eq!(input.line_and_col_of(7), (1, 0));
        assert_eq!(input.line_and_col_of(9), (1, 2));

        // char_idx_for_line_col ignores \r
        assert_eq!(input.char_idx_for_line_col(0, 5), 5);
        assert_eq!(input.char_idx_for_line_col(1, 0), 7);
        assert_eq!(input.char_idx_for_line_col(1, 2), 9);
    }

    #[test]
    fn test_set_text_preserves_cursor_and_clears_selection() {
        let mut input = TextInputState::new("hello world");
        input.cursor = 5;
        input.selection_anchor = Some(2);

        // Setting new text preserves mid-cursor if within new length
        input.set_text("hello brave world");
        assert_eq!(input.cursor, 5);
        assert_eq!(input.selection_anchor, None);

        // Setting shorter text clamps cursor to new length
        input.set_text("hi");
        assert_eq!(input.cursor, 2);
        assert_eq!(input.selection_anchor, None);
    }

    #[test]
    fn test_headless_mount_dispatch_change_preserves_mid_cursor() {
        let mut col = UiElement::new("column");
        let mut inp = UiElement::new("input");
        inp.id = Some("test_inp".to_string());
        inp.props.insert("value".to_string(), "hello".to_string());
        col.children.push(inp);

        let mut mount = HeadlessMount::new("App", Some(col));
        let st = mount.ensure_input_state("test_inp");
        st.cursor = 2; // Mid-string cursor between 'e' and 'l'

        mount.dispatch_change("test_inp", "heXYZllo");
        let st_after = mount.ensure_input_state("test_inp");
        assert_eq!(st_after.cursor, 2, "cursor must be preserved on change");
        assert_eq!(st_after.text, "heXYZllo");
        assert_eq!(st_after.selection_anchor, None);
    }

    #[test]
    fn test_multiline_selection_line_and_col_mapping() {
        let mut input = TextInputState::new("line0\nline1\nline2");
        assert_eq!(input.line_and_col_of(6), (1, 0));
        assert_eq!(input.line_and_col_of(8), (1, 2));
        assert_eq!(input.line_and_col_of(11), (1, 5));
        assert_eq!(input.line_and_col_of(14), (2, 2));

        // Select within line 1
        input.cursor = 9;
        input.selection_anchor = Some(7);
        let (start, end) = input.selection_range().unwrap();
        assert_eq!(input.line_and_col_of(start), (1, 1));
        assert_eq!(input.line_and_col_of(end), (1, 3));

        // Select across lines: line 0 to line 2
        input.cursor = 14;
        input.selection_anchor = Some(2);
        let (start2, end2) = input.selection_range().unwrap();
        assert_eq!(input.line_and_col_of(start2), (0, 2));
        assert_eq!(input.line_and_col_of(end2), (2, 2));
    }

    #[test]
    fn test_text_input_undo_redo_typing() {
        let mut input = TextInputState::new("hello");
        assert_eq!(input.text, "hello");
        assert_eq!(input.cursor, 5);
        assert!(input.undo_stack.is_empty());
        assert!(input.redo_stack.is_empty());

        input.insert_str(" world");
        assert_eq!(input.text, "hello world");
        assert_eq!(input.cursor, 11);
        assert_eq!(input.undo_stack.len(), 1);

        // Undo typing
        assert!(input.undo());
        assert_eq!(input.text, "hello");
        assert_eq!(input.cursor, 5);
        assert_eq!(input.redo_stack.len(), 1);

        // Redo typing
        assert!(input.redo());
        assert_eq!(input.text, "hello world");
        assert_eq!(input.cursor, 11);
        assert!(input.redo_stack.is_empty());

        // Redo again returns false
        assert!(!input.redo());
    }

    #[test]
    fn test_text_input_undo_redo_backspace() {
        let mut input = TextInputState::new("hello");
        assert!(input.backspace());
        assert_eq!(input.text, "hell");
        assert_eq!(input.cursor, 4);

        // Undo backspace
        assert!(input.undo());
        assert_eq!(input.text, "hello");
        assert_eq!(input.cursor, 5);

        // Redo backspace
        assert!(input.redo());
        assert_eq!(input.text, "hell");
        assert_eq!(input.cursor, 4);
    }

    #[test]
    fn test_text_input_undo_redo_delete_cut_paste() {
        let mut input = TextInputState::new("test");
        input.cursor = 0;
        assert!(input.delete());
        assert_eq!(input.text, "est");
        assert!(input.undo());
        assert_eq!(input.text, "test");
        assert!(input.redo());
        assert_eq!(input.text, "est");

        // Cut with selection
        input.select_all();
        let cut = input.cut();
        assert_eq!(cut.as_deref(), Some("est"));
        assert_eq!(input.text, "");
        assert!(input.undo());
        assert_eq!(input.text, "est");
        assert!(input.redo());
        assert_eq!(input.text, "");

        // Paste
        input.paste("pasted");
        assert_eq!(input.text, "pasted");
        assert!(input.undo());
        assert_eq!(input.text, "");
        assert!(input.redo());
        assert_eq!(input.text, "pasted");

        // Set text
        input.set_text("reset");
        assert_eq!(input.text, "reset");
        assert!(input.undo());
        assert_eq!(input.text, "pasted");
        assert!(input.redo());
        assert_eq!(input.text, "reset");
    }

    #[test]
    fn test_text_input_undo_stack_cap_and_multiple_steps() {
        let mut input = TextInputState::new("");
        for i in 0..60 {
            input.insert_str(&format!("{i},"));
        }
        // Stack must be capped at 50 snapshots
        assert_eq!(input.undo_stack.len(), 50);

        // Can undo 50 times
        for _ in 0..50 {
            assert!(input.undo());
        }
        // 51st undo fails
        assert!(!input.undo());

        // Can redo 50 times back
        for _ in 0..50 {
            assert!(input.redo());
        }
        assert!(!input.redo());
    }

    /// A choice field's estimate has to be settled by the widest thing it can
    /// ever show, and fixed in height no matter how many options it carries.
    /// A list is an overlay: a longer one must not grow the form around it.
    #[test]
    fn test_estimate_select_sizes_to_its_widest_option_and_stays_one_row_tall() {
        let short = select_elem("Small", &["Small", "Medium", "Large"]);
        let tall = select_elem("Small", &["Small", "Medium", "Large", "Extra large option"]);

        let (short_w, short_h) = estimate_element_size(&short, 1000);
        let (tall_w, tall_h) = estimate_element_size(&tall, 1000);

        assert_eq!(
            short_w,
            "Medium".len() as i64 * 16 + 56,
            "the widest option sets the width, not the label showing"
        );
        assert!(tall_w > short_w, "a wider option list makes a wider field");
        assert_eq!(
            short_h, tall_h,
            "…but never a taller one: the list is an overlay, not more form"
        );

        let (_, clipped) = estimate_element_size(&tall, 200);
        assert_eq!(
            clipped, 40,
            "the width is clamped to the room there is, and the height is still one row"
        );
    }

    /// A select without options still has to occupy a field's worth of room —
    /// it is a control the user can see, not a gap.
    #[test]
    fn test_estimate_select_without_options_still_occupies_a_field() {
        let empty = select_elem("Small", &[]);
        let (w, h) = estimate_element_size(&empty, 1000);
        assert!(
            w >= 140,
            "a field is never narrower than one can be typed into"
        );
        assert_eq!(h, 40, "and it is one row tall like any other");
    }

    fn select_elem(value: &str, options: &[&str]) -> UiElement {
        let mut elem = UiElement::new("select");
        elem.text = Some(value.into());
        elem.props.insert("value".to_string(), value.into());
        if !options.is_empty() {
            elem.props.insert("options".to_string(), options.join("\n"));
        }
        elem
    }

    /// A boolean control is measured as its whole row — control, gap and the
    /// label beside it — with the same generous per-character budget the
    /// `text` arm uses, so an overshoot leaves room where an undershoot would
    /// clip the words. Neither form grows taller when the label is long; it
    /// only widens.
    #[test]
    fn test_estimate_flag_row_covers_the_control_and_the_label() {
        let mut chk = UiElement::new("checkbox");
        chk.text = Some("Dark mode".into());
        let (w, h) = estimate_element_size(&chk, 1000);
        assert_eq!(h, 34, "a flag is one row tall whatever the label says");
        assert!(
            w >= 18 + 10 + 9 * 16,
            "the control, the gap and all nine characters fit: {w}"
        );

        let mut long = UiElement::new("checkbox");
        long.text = Some("A considerably longer label than the first".into());
        let (w2, h2) = estimate_element_size(&long, 1000);
        assert!(w2 > w, "a longer label widens the row");
        assert_eq!(h2, h, "…but never makes it taller");

        let mut tgl = UiElement::new("toggle");
        tgl.text = Some("Dark mode".into());
        let (w3, h3) = estimate_element_size(&tgl, 1000);
        assert!(w3 > w, "a toggle claims more room for its control");
        assert_eq!(h3, h, "…without claiming more room down the page");

        // Clamped like every other control, so a very long label cannot push
        // the estimate past the window it is being measured against.
        assert!(w2 <= 1000, "never wider than the room there is");
    }

    /// A drag's arithmetic: clamped to the ends, pulled onto the step's own
    /// rungs, and never resting between two numbers the program would read
    /// the same way.
    #[test]
    fn test_slider_snap_clamps_and_steps() {
        // Whole-numbered steps land exactly on their rungs.
        assert_eq!(slider_snap(37.0, 0.0, 100.0, 10.0), 40.0);
        assert_eq!(
            slider_snap(32.0, 0.0, 100.0, 10.0),
            30.0,
            "halfway goes down"
        );
        assert_eq!(slider_snap(35.0, 0.0, 100.0, 10.0), 40.0, "halfway goes up");

        // A float's dust is gone before it reaches state: `0.1 * 3` is not
        // `0.3`, and a slider must not report that to a handler.
        let sum = 0.1f64 + 0.1 + 0.1;
        assert_eq!(slider_snap(sum, 0.0, 1.0, 0.1), 0.3);

        // Clamped at both ends, so a drag past the track cannot overshoot.
        assert_eq!(slider_snap(500.0, 0.0, 100.0, 10.0), 100.0);
        assert_eq!(slider_snap(-5.0, 0.0, 100.0, 10.0), 0.0);

        // Counted from `min`, so a range that does not start at zero still
        // has a rung at its own bottom.
        assert_eq!(slider_snap(3.0, -10.0, 10.0, 5.0), 5.0, "13/5 rounds to 3");
        assert_eq!(slider_snap(-8.0, -10.0, 10.0, 5.0), -10.0);
        assert_eq!(slider_snap(-9.0, -10.0, 10.0, 5.0), -10.0);

        // A degenerate range is a single position, not a division by zero.
        assert_eq!(slider_snap(42.0, 5.0, 5.0, 1.0), 5.0);
        // …and so is a nonsense step.
        assert_eq!(slider_snap(3.7, 0.0, 10.0, 0.0), 3.7);
    }

    /// The spelling decides the *type* a handler is handed, and it has to be
    /// the same on every drag of one control: a handler declaring `v: float`
    /// must not be handed an Int because the value happened to land whole.
    #[test]
    fn test_slider_num_str_keeps_one_type_per_control() {
        // A whole-numbered step never grows a point: an Int, every time.
        assert_eq!(slider_num_str(42.0, 1.0), "42");
        assert_eq!(slider_num_str(1500.0, 1.0), "1500");
        assert_eq!(slider_num_str(0.0, 1.0), "0");

        // A fractional step always has one — including when the value lands
        // on a whole number, which is the case that would otherwise flip the
        // handler's parameter type mid-drag.
        assert_eq!(slider_num_str(42.0, 0.5), "42.0");
        assert_eq!(slider_num_str(42.5, 0.5), "42.5");
        assert_eq!(slider_num_str(0.30000000000000004, 0.1), "0.3");

        // The precision comes from the step, so a coarse float step does not
        // print a page of digits nobody asked for.
        assert_eq!(slider_num_str(0.123456789, 0.25), "0.12");
        assert_eq!(
            slider_num_str(slider_snap(0.2499999, 0.0, 1.0, 0.25), 0.25),
            "0.25",
            "…and a value that belongs to a quarter lands exactly on it"
        );
    }

    /// A slider claims a fixed dial's worth of room and one row's height, and
    /// a longer label only ever widens it — the same shape the flags lay out.
    #[test]
    fn test_estimate_element_size_slider_is_a_fixed_track_and_one_row() {
        let mut s = UiElement::new("slider");
        s.text = Some("Temp".into());
        let (w, h) = estimate_element_size(&s, 1000);
        assert_eq!(h, 34, "a slider is one row tall whatever the label says");
        assert!(
            w >= SLIDER_TRACK_W as i64 + 10 + 4 * 16,
            "the track, the gap and all four characters fit: {w}"
        );

        let mut long = UiElement::new("slider");
        long.text = Some("A considerably longer label than the first".into());
        let (w2, h2) = estimate_element_size(&long, 1000);
        assert!(w2 > w, "a longer label widens the row");
        assert_eq!(h2, h, "…but never makes it taller");

        // The floor wins over the clamp: the track has a fixed length, so a
        // window too narrow to hold it cannot squeeze the dial smaller — it
        // gets clipped by the renderer instead. (Same as a toggle in a window
        // narrower than the toggle itself.)
        let narrow = estimate_element_size(&s, 120);
        assert_eq!(
            narrow.0,
            (SLIDER_TRACK_W + 18) as i64,
            "the track never shrinks below itself plus its own margin"
        );
        assert_eq!(
            narrow.1, 34,
            "and a narrow window shortens the label, not the track"
        );
    }
}
