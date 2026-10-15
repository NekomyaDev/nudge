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

/// Headless in-memory mount for a window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeadlessMount {
    pub title: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub root: Option<UiElement>,
}

impl HeadlessMount {
    pub fn new(title: impl Into<String>, root: Option<UiElement>) -> Self {
        Self {
            title: title.into(),
            width: None,
            height: None,
            root,
        }
    }

    pub fn find(&self, id: &str) -> Option<&UiElement> {
        self.root.as_ref().and_then(|r| r.find_by_id(id))
    }

    pub fn find_text(&self, text: &str) -> Option<&UiElement> {
        self.root.as_ref().and_then(|r| r.find_by_text(text))
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
}
