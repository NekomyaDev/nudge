//! Real native OS window backend for Nudge's UI using winit 0.30 and softbuffer 0.4.
#![cfg(feature = "native-gui")]

use crate::font8x8::get_glyph;
use crate::ui::{TextInputState, UiElement};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

/// Configuration for opening a native OS window.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Nudge UI".to_string(),
            width: 400,
            height: 300,
        }
    }
}

/// Callback dispatcher for reacting to UI events and fetching updated view trees.
pub trait UiDispatcher {
    fn root(&mut self) -> Option<UiElement>;
    fn dispatch_click(&mut self, button_id: &str);
    fn dispatch_change(&mut self, element_id: &str, value: &str) {
        let _ = (element_id, value);
    }
    fn dispatch_key(&mut self, key: &str, target_id: Option<&str>) {
        let _ = (key, target_id);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub rect: Rect,
    pub tag: String,
    pub id: Option<String>,
    pub text: Option<String>,
    pub on_click: Option<String>,
    pub on_change: Option<String>,
    pub on_key: Option<String>,
}

// -----------------------------------------------------------------------------
// Software Rasterization Primitives
// -----------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn draw_rect(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    let x_start = x.max(0) as usize;
    let y_start = y.max(0) as usize;
    let x_end = ((x + w).max(0) as usize).min(stride);
    let y_end = ((y + h).max(0) as usize).min(height);

    for row in y_start..y_end {
        let row_offset = row * stride;
        for col in x_start..x_end {
            buf[row_offset + col] = color;
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_rect_outline(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    draw_rect(buf, stride, height, x, y, w, 1, color);
    draw_rect(buf, stride, height, x, y + h - 1, w, 1, color);
    draw_rect(buf, stride, height, x, y, 1, h, color);
    draw_rect(buf, stride, height, x + w - 1, y, 1, h, color);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_char(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    x: i32,
    y: i32,
    ch: char,
    color: u32,
    scale: usize,
) {
    let glyph = get_glyph(ch);

    for (row, &byte) in glyph.iter().enumerate() {
        for col in 0..8 {
            // font8x8 stores each row MSB-first: bit 7 is the leftmost pixel.
            if (byte & (1 << (7 - col))) != 0 {
                let px = x + (col * scale) as i32;
                let py = y + (row * scale) as i32;
                draw_rect(
                    buf,
                    stride,
                    height,
                    px,
                    py,
                    scale as i32,
                    scale as i32,
                    color,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    start_x: i32,
    mut y: i32,
    text: &str,
    color: u32,
    scale: usize,
) {
    let char_w = (8 * scale) as i32;
    let line_h = (10 * scale) as i32;
    let mut x = start_x;
    for ch in text.chars() {
        if ch == '\r' {
            continue;
        }
        if ch == '\n' {
            x = start_x;
            y += line_h;
            continue;
        }
        draw_char(buf, stride, height, x, y, ch, color, scale);
        x += char_w;
    }
}

// -----------------------------------------------------------------------------
// Layout Engine
// -----------------------------------------------------------------------------

/// Wraps text cleanly on word boundaries (whitespace) if lines exceed `max_width_px`.
/// Preserves explicit newlines `\n`.
pub fn wrap_text(content: &str, max_width_px: i32, char_width_px: i32) -> String {
    let char_w = char_width_px.max(1);
    let max_chars = (max_width_px / char_w).max(1) as usize;

    let mut result_lines = Vec::new();

    for paragraph in content.split('\n') {
        let total_chars = paragraph.chars().count();
        if total_chars <= max_chars {
            result_lines.push(paragraph.to_string());
            continue;
        }

        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            result_lines.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        let mut current_len = 0usize;

        for word in words {
            let word_len = word.chars().count();

            // If a single word exceeds max_chars, it must be hard-broken
            if word_len > max_chars {
                if !current_line.is_empty() {
                    result_lines.push(current_line);
                    current_line = String::new();
                    current_len = 0;
                }

                let mut word_chunk = String::new();
                let mut chunk_len = 0usize;
                for ch in word.chars() {
                    word_chunk.push(ch);
                    chunk_len += 1;
                    if chunk_len == max_chars {
                        result_lines.push(word_chunk);
                        word_chunk = String::new();
                        chunk_len = 0;
                    }
                }
                if !word_chunk.is_empty() {
                    current_line = word_chunk;
                    current_len = chunk_len;
                }
                continue;
            }

            if current_line.is_empty() {
                current_line = word.to_string();
                current_len = word_len;
            } else if current_len + 1 + word_len <= max_chars {
                current_line.push(' ');
                current_line.push_str(word);
                current_len += 1 + word_len;
            } else {
                result_lines.push(current_line);
                current_line = word.to_string();
                current_len = word_len;
            }
        }

        if !current_line.is_empty() {
            result_lines.push(current_line);
        }
    }

    result_lines.join("\n")
}

#[allow(clippy::only_used_in_recursion)]
pub fn layout_element(
    elem: &UiElement,
    x: i32,
    y: i32,
    max_w: i32,
    boxes: &mut Vec<LayoutBox>,
) -> (i32, i32) {
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

            let mut child_y = y + padding;
            let mut max_child_w = 0;

            for child in &elem.children {
                let (cw, ch) =
                    layout_element(child, x + padding, child_y, max_w - 2 * padding, boxes);
                max_child_w = max_child_w.max(cw);
                child_y += ch + gap;
            }

            let total_h = if elem.children.is_empty() {
                2 * padding
            } else {
                child_y - gap + padding - y
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

            let mut child_x = x + padding;
            let mut max_child_h = 0;

            for child in &elem.children {
                let (cw, ch) = layout_element(child, child_x, y + padding, max_w, boxes);
                max_child_h = max_child_h.max(ch);
                child_x += cw + gap;
            }

            let total_w = if elem.children.is_empty() {
                2 * padding
            } else {
                child_x - gap + padding - x
            };
            let total_h = max_child_h + 2 * padding;

            (total_w, total_h)
        }
        "text" => {
            let raw_content = elem.text.clone().unwrap_or_else(|| elem.get_text());
            let explicit_wrap = elem.props.get("wrap").map(|s| s.as_str());
            let target_w = if max_w > 80 { max_w } else { 540 };
            let should_wrap = if explicit_wrap == Some("true") {
                true
            } else if explicit_wrap == Some("false") {
                false
            } else {
                raw_content
                    .split('\n')
                    .any(|l| (l.chars().count() as i32) * 16 > target_w)
            };

            let content = if should_wrap {
                wrap_text(&raw_content, target_w, 16)
            } else {
                raw_content
            };

            let lines = content.split('\n').count().max(1) as i32;
            let max_line_len = content
                .split('\n')
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0) as i32;
            let w = max_line_len * 16;
            let h = lines * 20;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "text".into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: None,
                on_change: None,
                on_key: elem.on_key.clone(),
            });
            (w, h)
        }
        "button" => {
            let content = elem
                .text
                .clone()
                .or_else(|| elem.props.get("label").cloned())
                .unwrap_or_else(|| {
                    let t = elem.get_text();
                    if t.is_empty() {
                        "Button".into()
                    } else {
                        t
                    }
                });
            let text_w = (content.chars().count() as i32) * 16;
            let w = (text_w + 32).max(80);
            let h = 36;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "button".into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: elem.on_click.clone().or_else(|| elem.id.clone()),
                on_change: None,
                on_key: elem.on_key.clone(),
            });
            (w, h)
        }
        "input" => {
            let content = elem
                .text
                .clone()
                .or_else(|| elem.props.get("value").cloned())
                .unwrap_or_default();
            let lines = content.split('\n').count().max(1) as i32;
            let max_line_len = content
                .split('\n')
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0) as i32;
            let w = 220.max(max_line_len * 16 + 24);
            let h = 36.max(lines * 20 + 16);
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "input".into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: None,
                on_change: elem.on_change.clone(),
                on_key: elem.on_key.clone(),
            });
            (w, h)
        }
        _ => {
            // Generic container or unknown tag
            let mut child_y = y;
            let mut max_w_seen = 0;
            for child in &elem.children {
                let (cw, ch) = layout_element(child, x, child_y, max_w, boxes);
                max_w_seen = max_w_seen.max(cw);
                child_y += ch + 8;
            }
            (max_w_seen, (child_y - y).max(20))
        }
    }
}

/// Helper to determine which button in `boxes` contains the cursor coordinates `pos`.
pub fn find_hovered_button_in_boxes(
    boxes: &[LayoutBox],
    scroll_x: i32,
    scroll_y: i32,
    pos: (i32, i32),
) -> Option<String> {
    let (mx, my) = pos;
    for (i, lbox) in boxes.iter().enumerate() {
        if lbox.tag == "button" {
            let rx = lbox.rect.x - scroll_x;
            let ry = lbox.rect.y - scroll_y;
            let screen_rect = Rect {
                x: rx,
                y: ry,
                w: lbox.rect.w,
                h: lbox.rect.h,
            };
            if screen_rect.contains(mx, my) {
                return Some(
                    lbox.id
                        .clone()
                        .or_else(|| lbox.on_click.clone())
                        .unwrap_or_else(|| format!("__btn_{i}")),
                );
            }
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Application Handler and Event Loop
// -----------------------------------------------------------------------------

pub struct NativeApp<D: UiDispatcher> {
    pub config: WindowConfig,
    pub dispatcher: D,
    pub window: Option<Arc<Window>>,
    pub context: Option<softbuffer::Context<Arc<Window>>>,
    pub surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    pub width: u32,
    pub height: u32,
    pub cursor_pos: (i32, i32),
    pub hovered_id: Option<String>,
    pub layout_boxes: Vec<LayoutBox>,
    pub render_count: usize,
    pub test_runner: Option<fn(&mut NativeApp<D>, &ActiveEventLoop)>,
    pub focused_id: Option<String>,
    pub text_inputs: HashMap<String, TextInputState>,
    pub scroll_x: i32,
    pub scroll_y: i32,
    pub content_width: i32,
    pub content_height: i32,
    pub clipboard: String,
    pub shift_pressed: bool,
    pub ctrl_pressed: bool,
}

impl<D: UiDispatcher> NativeApp<D> {
    pub fn new(config: WindowConfig, dispatcher: D) -> Self {
        let width = config.width.max(100);
        let height = config.height.max(100);
        Self {
            config,
            dispatcher,
            window: None,
            context: None,
            surface: None,
            width,
            height,
            cursor_pos: (0, 0),
            hovered_id: None,
            layout_boxes: Vec::new(),
            render_count: 0,
            test_runner: None,
            focused_id: None,
            text_inputs: HashMap::new(),
            scroll_x: 0,
            scroll_y: 0,
            content_width: 0,
            content_height: 0,
            clipboard: String::new(),
            shift_pressed: false,
            ctrl_pressed: false,
        }
    }

    pub fn ensure_input_state(&mut self, id: &str, initial_text: &str) -> &mut TextInputState {
        if !self.text_inputs.contains_key(id) {
            self.text_inputs
                .insert(id.to_string(), TextInputState::new(initial_text));
        }
        self.text_inputs.get_mut(id).unwrap()
    }

    pub fn scroll_by(&mut self, dx: i32, dy: i32) {
        let max_y = (self.content_height - self.height as i32 + 20).max(0);
        let max_x = (self.content_width - self.width as i32 + 20).max(0);
        self.scroll_x = (self.scroll_x + dx).clamp(0, max_x);
        self.scroll_y = (self.scroll_y + dy).clamp(0, max_y);
        self.render();
    }

    pub fn find_hovered_button(&self, pos: (i32, i32)) -> Option<String> {
        find_hovered_button_in_boxes(&self.layout_boxes, self.scroll_x, self.scroll_y, pos)
    }

    pub fn handle_cursor_moved(&mut self, pos: (i32, i32)) -> bool {
        self.cursor_pos = pos;
        let new_hovered = self.find_hovered_button(pos);
        if new_hovered != self.hovered_id {
            self.hovered_id = new_hovered;
            true
        } else {
            false
        }
    }

    pub fn render(&mut self) {
        if self.surface.is_none() || self.window.is_none() {
            return;
        }

        let w_nz = match NonZeroU32::new(self.width) {
            Some(w) => w,
            None => return,
        };
        let h_nz = match NonZeroU32::new(self.height) {
            Some(h) => h,
            None => return,
        };

        let surface = self.surface.as_mut().unwrap();
        if surface.resize(w_nz, h_nz).is_err() {
            return;
        }

        let mut buffer = match surface.buffer_mut() {
            Ok(b) => b,
            Err(_) => return,
        };

        let stride = self.width as usize;
        let h_usize = self.height as usize;

        // Background: Zinc-900 (0xFF18181B)
        draw_rect(
            &mut buffer,
            stride,
            h_usize,
            0,
            0,
            self.width as i32,
            self.height as i32,
            0xFF18181B,
        );

        // Fetch latest view tree and layout
        self.layout_boxes.clear();
        let mut total_w = 0;
        let mut total_h = 0;
        if let Some(root) = self.dispatcher.root() {
            let (cw, ch) = layout_element(
                &root,
                10,
                10,
                self.width as i32 - 20,
                &mut self.layout_boxes,
            );
            total_w = cw;
            total_h = ch;
        }
        self.content_width = total_w;
        self.content_height = total_h;

        // Clamp scroll offsets
        let max_scroll_y = (total_h - self.height as i32 + 20).max(0);
        let max_scroll_x = (total_w - self.width as i32 + 20).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max_scroll_y);
        self.scroll_x = self.scroll_x.clamp(0, max_scroll_x);

        let (mx, my) = self.cursor_pos;
        self.hovered_id = find_hovered_button_in_boxes(
            &self.layout_boxes,
            self.scroll_x,
            self.scroll_y,
            self.cursor_pos,
        );

        // Render each layout box with viewport scroll offset & clipping
        for lbox in &self.layout_boxes {
            let rx = lbox.rect.x - self.scroll_x;
            let ry = lbox.rect.y - self.scroll_y;

            // Viewport clipping: skip completely out-of-bounds elements
            if ry + lbox.rect.h < 0
                || ry >= self.height as i32
                || rx + lbox.rect.w < 0
                || rx >= self.width as i32
            {
                continue;
            }

            match lbox.tag.as_str() {
                "text" => {
                    if let Some(ref text) = lbox.text {
                        draw_text(
                            &mut buffer,
                            stride,
                            h_usize,
                            rx,
                            ry + 2,
                            text,
                            0xFFF4F4F5, // crisp white
                            2,
                        );
                    }
                }
                "button" => {
                    let screen_rect = Rect {
                        x: rx,
                        y: ry,
                        w: lbox.rect.w,
                        h: lbox.rect.h,
                    };
                    let hovered = screen_rect.contains(mx, my);
                    let bg_color = if hovered {
                        0xFF2563EB // Blue-600 hover
                    } else {
                        0xFF3B82F6 // Blue-500
                    };
                    draw_rect(
                        &mut buffer,
                        stride,
                        h_usize,
                        rx,
                        ry,
                        lbox.rect.w,
                        lbox.rect.h,
                        bg_color,
                    );
                    draw_rect_outline(
                        &mut buffer,
                        stride,
                        h_usize,
                        rx,
                        ry,
                        lbox.rect.w,
                        lbox.rect.h,
                        0xFF60A5FA, // lighter border
                    );
                    if let Some(ref text) = lbox.text {
                        let text_w = (text.chars().count() as i32) * 16;
                        let tx = rx + ((lbox.rect.w - text_w) / 2).max(8);
                        let ty = ry + ((lbox.rect.h - 16) / 2).max(4);
                        draw_text(&mut buffer, stride, h_usize, tx, ty, text, 0xFFFFFFFF, 2);
                    }
                }
                "input" => {
                    let is_focused =
                        self.focused_id.as_deref() == lbox.id.as_deref() && lbox.id.is_some();
                    let border_color = if is_focused {
                        0xFF3B82F6 // Blue-500 focused outline
                    } else {
                        0xFF52525B // Zinc-600 unfocused border
                    };

                    draw_rect(
                        &mut buffer,
                        stride,
                        h_usize,
                        rx,
                        ry,
                        lbox.rect.w,
                        lbox.rect.h,
                        0xFF27272A, // Zinc-800
                    );
                    draw_rect_outline(
                        &mut buffer,
                        stride,
                        h_usize,
                        rx,
                        ry,
                        lbox.rect.w,
                        lbox.rect.h,
                        border_color,
                    );

                    let (display_text, cursor_opt, selection_opt) = if let Some(ref id) = lbox.id {
                        if let Some(st) = self.text_inputs.get(id) {
                            (st.text.clone(), Some(st.cursor), st.selection_range())
                        } else {
                            (lbox.text.clone().unwrap_or_default(), None, None)
                        }
                    } else {
                        (lbox.text.clone().unwrap_or_default(), None, None)
                    };

                    // Draw selection highlight if focused and range selected
                    if is_focused {
                        if let Some((sel_start, sel_end)) = selection_opt {
                            if let Some(ref id) = lbox.id {
                                if let Some(st) = self.text_inputs.get(id) {
                                    let (start_line, start_col) = st.line_and_col_of(sel_start);
                                    let (end_line, end_col) = st.line_and_col_of(sel_end);
                                    for line in start_line..=end_line {
                                        let col_start =
                                            if line == start_line { start_col } else { 0 };
                                        let col_end = if line == end_line {
                                            end_col
                                        } else {
                                            st.line_len(line)
                                        };
                                        if col_end > col_start {
                                            let sel_x = rx + 8 + (col_start as i32) * 16;
                                            let sel_w = ((col_end - col_start) as i32) * 16;
                                            let sel_y = ry + 6 + (line as i32) * 20;
                                            draw_rect(
                                                &mut buffer,
                                                stride,
                                                h_usize,
                                                sel_x,
                                                sel_y,
                                                sel_w,
                                                20,
                                                0xFF1D4ED8, // Blue-700 selection background
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Draw text
                    draw_text(
                        &mut buffer,
                        stride,
                        h_usize,
                        rx + 8,
                        ry + 10,
                        &display_text,
                        0xFFE4E4E7,
                        2,
                    );

                    // Draw cursor bar if focused
                    if is_focused {
                        let cur_idx = cursor_opt.unwrap_or_else(|| display_text.chars().count());
                        let (cur_line, cur_col) = if let Some(ref id) = lbox.id {
                            if let Some(st) = self.text_inputs.get(id) {
                                st.line_and_col_of(cur_idx)
                            } else {
                                (0, cur_idx)
                            }
                        } else {
                            (0, cur_idx)
                        };
                        let cur_x = rx + 8 + (cur_col as i32) * 16;
                        let cur_y = ry + 8 + (cur_line as i32) * 20;
                        draw_rect(
                            &mut buffer,
                            stride,
                            h_usize,
                            cur_x,
                            cur_y,
                            2,
                            20,
                            0xFFFFFFFF, // white cursor
                        );
                    }
                }
                _ => {}
            }
        }

        // Render scrollbar if content exceeds viewport
        if self.content_height > self.height as i32 {
            let track_h = self.height as i32;
            let thumb_h =
                ((track_h as f32 * track_h as f32) / self.content_height as f32).max(24.0) as i32;
            let max_scroll = (self.content_height - track_h + 20).max(1);
            let thumb_y =
                ((self.scroll_y as f32 / max_scroll as f32) * (track_h - thumb_h) as f32) as i32;

            draw_rect(
                &mut buffer,
                stride,
                h_usize,
                self.width as i32 - 8,
                0,
                8,
                track_h,
                0xFF27272A,
            );
            draw_rect(
                &mut buffer,
                stride,
                h_usize,
                self.width as i32 - 7,
                thumb_y,
                6,
                thumb_h,
                0xFF71717A, // Zinc-500
            );
        }

        self.render_count += 1;
        let _ = buffer.present();
    }

    pub fn handle_click(&mut self) {
        let (mx, my) = self.cursor_pos;
        let cx = mx + self.scroll_x;
        let cy = my + self.scroll_y;

        let mut clicked_button = None;
        let mut clicked_input = None;

        for lbox in &self.layout_boxes {
            if lbox.rect.contains(cx, cy) {
                if lbox.tag == "button" {
                    if let Some(ref id) = lbox.id {
                        clicked_button = Some(id.clone());
                    } else if let Some(ref on_click) = lbox.on_click {
                        clicked_button = Some(on_click.clone());
                    }
                    break;
                } else if lbox.tag == "input" {
                    clicked_input = Some(lbox.clone());
                    break;
                }
            }
        }

        if let Some(button_id) = clicked_button {
            self.dispatcher.dispatch_click(&button_id);
            self.render();
        } else if let Some(lbox) = clicked_input {
            if let Some(ref id) = lbox.id {
                self.focused_id = Some(id.clone());
                let rel_x = cx - (lbox.rect.x + 8);
                let rel_y = cy - (lbox.rect.y + 8);
                let line_idx = (rel_y.max(0) / 20) as usize;
                let col_idx = (rel_x.max(0) / 16) as usize;

                let st = self.ensure_input_state(id, lbox.text.as_deref().unwrap_or(""));
                let target_line = line_idx.min(st.lines_count().saturating_sub(1));
                let target_col = col_idx.min(st.line_len(target_line));
                let char_idx = st.char_idx_for_line_col(target_line, target_col);
                st.cursor = char_idx.min(st.char_len());
                st.selection_anchor = None;
            }
            self.render();
        } else {
            if self.focused_id.is_some() {
                self.focused_id = None;
                self.render();
            }
        }
    }

    pub fn handle_key_input(&mut self, logical_key: &Key, typed_text: Option<&str>) {
        if let Some(ref focused_id) = self.focused_id.clone() {
            let lbox = self
                .layout_boxes
                .iter()
                .find(|b| b.id.as_deref() == Some(focused_id))
                .cloned();

            let initial = lbox
                .as_ref()
                .and_then(|b| b.text.as_deref())
                .unwrap_or_default();
            let on_change = lbox.as_ref().and_then(|b| b.on_change.clone());
            let on_key = lbox.as_ref().and_then(|b| b.on_key.clone());

            let mut value_changed = false;
            let mut key_name = String::new();
            let shift = self.shift_pressed;

            if self.ctrl_pressed {
                match logical_key {
                    Key::Character(s) if s.eq_ignore_ascii_case("a") => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.select_all();
                        self.render();
                        return;
                    }
                    Key::Character(s) if s.eq_ignore_ascii_case("c") => {
                        let st = self.ensure_input_state(focused_id, initial);
                        if let Some(copied) = st.copy() {
                            self.clipboard = copied;
                        }
                        return;
                    }
                    Key::Character(s) if s.eq_ignore_ascii_case("x") => {
                        let st = self.ensure_input_state(focused_id, initial);
                        if let Some(cut_text) = st.cut() {
                            self.clipboard = cut_text;
                            value_changed = true;
                        }
                    }
                    Key::Character(s) if s.eq_ignore_ascii_case("v") => {
                        if !self.clipboard.is_empty() {
                            let clip = self.clipboard.clone();
                            let st = self.ensure_input_state(focused_id, initial);
                            st.paste(&clip);
                            value_changed = true;
                        }
                    }
                    Key::Character(s) if s.eq_ignore_ascii_case("z") => {
                        let st = self.ensure_input_state(focused_id, initial);
                        if shift {
                            st.redo();
                            value_changed = true;
                            key_name = "Redo".into();
                        } else {
                            st.undo();
                            value_changed = true;
                            key_name = "Undo".into();
                        }
                    }
                    Key::Character(s) if s.eq_ignore_ascii_case("y") => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.redo();
                        value_changed = true;
                        key_name = "Redo".into();
                    }
                    Key::Named(NamedKey::Home) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.move_doc_start(shift);
                        key_name = "Home".to_string();
                    }
                    Key::Named(NamedKey::End) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.move_doc_end(shift);
                        key_name = "End".to_string();
                    }
                    _ => {}
                }
            } else {
                match logical_key {
                    Key::Named(NamedKey::ArrowLeft) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.move_left(shift);
                        key_name = "ArrowLeft".to_string();
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.move_right(shift);
                        key_name = "ArrowRight".to_string();
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.move_up(shift);
                        key_name = "ArrowUp".to_string();
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.move_down(shift);
                        key_name = "ArrowDown".to_string();
                    }
                    Key::Named(NamedKey::Home) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.move_home(shift);
                        key_name = "Home".to_string();
                    }
                    Key::Named(NamedKey::End) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        st.move_end(shift);
                        key_name = "End".to_string();
                    }
                    Key::Named(NamedKey::Backspace) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        value_changed = st.backspace();
                        key_name = "Backspace".to_string();
                    }
                    Key::Named(NamedKey::Delete) => {
                        let st = self.ensure_input_state(focused_id, initial);
                        value_changed = st.delete();
                        key_name = "Delete".to_string();
                    }
                    Key::Named(NamedKey::Enter) => {
                        let is_multiline = lbox
                            .as_ref()
                            .and_then(|b| b.text.as_ref())
                            .map(|t| t.contains('\n'))
                            .unwrap_or(false)
                            || shift;
                        if is_multiline {
                            let st = self.ensure_input_state(focused_id, initial);
                            st.insert_newline();
                            value_changed = true;
                        }
                        key_name = "Enter".to_string();
                    }
                    Key::Named(NamedKey::Escape) => {
                        key_name = "Escape".to_string();
                        self.focused_id = None;
                    }
                    Key::Named(NamedKey::Tab) => {
                        key_name = "Tab".to_string();
                        let input_ids: Vec<String> = self
                            .layout_boxes
                            .iter()
                            .filter(|b| b.tag == "input" && b.id.is_some())
                            .filter_map(|b| b.id.clone())
                            .collect();
                        if !input_ids.is_empty() {
                            let curr_pos = input_ids.iter().position(|id| id == focused_id);
                            let next_idx = match curr_pos {
                                Some(pos) => (pos + 1) % input_ids.len(),
                                None => 0,
                            };
                            self.focused_id = Some(input_ids[next_idx].clone());
                        }
                    }
                    _ => {
                        if let Some(txt) = typed_text {
                            if !txt.chars().any(|c| c.is_control()) && !txt.is_empty() {
                                let st = self.ensure_input_state(focused_id, initial);
                                st.insert_str(txt);
                                value_changed = true;
                                key_name = txt.to_string();
                            }
                        } else if let Key::Character(s) = logical_key {
                            if !s.chars().any(|c| c.is_control()) && !s.is_empty() {
                                let st = self.ensure_input_state(focused_id, initial);
                                st.insert_str(s.as_str());
                                value_changed = true;
                                key_name = s.to_string();
                            }
                        }
                    }
                }
            }

            if value_changed {
                let new_text = self
                    .text_inputs
                    .get(focused_id)
                    .map(|st| st.text.clone())
                    .unwrap_or_default();
                if on_change.is_some() {
                    self.dispatcher.dispatch_change(focused_id, &new_text);
                }
            }

            if !key_name.is_empty() {
                let chord = if self.ctrl_pressed {
                    format!("Ctrl+{key_name}")
                } else {
                    key_name
                };
                if on_key.is_some() {
                    self.dispatcher.dispatch_key(&chord, Some(focused_id));
                }
            }

            self.render();
            return;
        }

        let mut handled_scroll = false;
        let mut key_name = String::new();

        match logical_key {
            Key::Named(NamedKey::PageUp) => {
                let delta = -(self.height as i32 - 40);
                self.scroll_by(0, delta);
                handled_scroll = true;
                key_name = "PageUp".to_string();
            }
            Key::Named(NamedKey::PageDown) => {
                let delta = self.height as i32 - 40;
                self.scroll_by(0, delta);
                handled_scroll = true;
                key_name = "PageDown".to_string();
            }
            Key::Named(NamedKey::Home) => {
                self.scroll_y = 0;
                self.render();
                handled_scroll = true;
                key_name = "Home".to_string();
            }
            Key::Named(NamedKey::End) => {
                let max_y = (self.content_height - self.height as i32 + 20).max(0);
                self.scroll_y = max_y;
                self.render();
                handled_scroll = true;
                key_name = "End".to_string();
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.scroll_by(0, -20);
                handled_scroll = true;
                key_name = "ArrowUp".to_string();
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.scroll_by(0, 20);
                handled_scroll = true;
                key_name = "ArrowDown".to_string();
            }
            Key::Named(NamedKey::Enter) => key_name = "Enter".to_string(),
            Key::Named(NamedKey::Escape) => key_name = "Escape".to_string(),
            Key::Character(s) => key_name = s.to_string(),
            _ => {}
        }

        if !key_name.is_empty() {
            let chord = if self.ctrl_pressed {
                format!("Ctrl+{key_name}")
            } else {
                key_name
            };
            self.dispatcher.dispatch_key(&chord, None);
        }

        if !handled_scroll {
            self.render();
        }
    }

    /// Programmatically focus an input field by id.
    pub fn focus_input(&mut self, input_id: &str) -> bool {
        let target = self
            .layout_boxes
            .iter()
            .find(|b| b.tag == "input" && b.id.as_deref() == Some(input_id))
            .cloned();

        if let Some(b) = target {
            self.focused_id = Some(input_id.to_string());
            let initial = b.text.as_deref().unwrap_or_default();
            self.ensure_input_state(input_id, initial);
            self.render();
            true
        } else {
            false
        }
    }

    /// Type text into the currently focused input.
    pub fn type_into_focused(&mut self, text: &str) {
        if let Some(fid) = self.focused_id.clone() {
            let initial = self
                .layout_boxes
                .iter()
                .find(|b| b.id.as_deref() == Some(&fid))
                .and_then(|b| b.text.clone())
                .unwrap_or_default();
            let st = self.ensure_input_state(&fid, &initial);
            st.insert_str(text);
            let new_text = st.text.clone();
            self.dispatcher.dispatch_change(&fid, &new_text);
            self.render();
        }
    }

    /// Focus an input by id and type text into it.
    pub fn type_into(&mut self, input_id: &str, text: &str) -> bool {
        if self.focus_input(input_id) {
            self.type_into_focused(text);
            true
        } else {
            false
        }
    }

    /// Programmatically send a key (e.g. "Enter", "Backspace", "ArrowLeft").
    pub fn send_key(&mut self, key_str: &str) {
        if key_str == "Undo" || key_str == "Ctrl+z" || key_str == "Ctrl+Z" {
            let prev_ctrl = self.ctrl_pressed;
            let prev_shift = self.shift_pressed;
            self.ctrl_pressed = true;
            self.shift_pressed = false;
            self.handle_key_input(&Key::Character("z".into()), None);
            self.ctrl_pressed = prev_ctrl;
            self.shift_pressed = prev_shift;
            return;
        }
        if key_str == "Redo"
            || key_str == "Ctrl+y"
            || key_str == "Ctrl+Y"
            || key_str == "Ctrl+Shift+z"
            || key_str == "Ctrl+Shift+Z"
        {
            let prev_ctrl = self.ctrl_pressed;
            let prev_shift = self.shift_pressed;
            self.ctrl_pressed = true;
            self.shift_pressed = key_str.starts_with("Ctrl+Shift");
            let key_char = if self.shift_pressed { "z" } else { "y" };
            self.handle_key_input(&Key::Character(key_char.into()), None);
            self.ctrl_pressed = prev_ctrl;
            self.shift_pressed = prev_shift;
            return;
        }

        let (key, text) = match key_str {
            "Backspace" => (Key::Named(NamedKey::Backspace), None),
            "Delete" => (Key::Named(NamedKey::Delete), None),
            "ArrowLeft" => (Key::Named(NamedKey::ArrowLeft), None),
            "ArrowRight" => (Key::Named(NamedKey::ArrowRight), None),
            "ArrowUp" => (Key::Named(NamedKey::ArrowUp), None),
            "ArrowDown" => (Key::Named(NamedKey::ArrowDown), None),
            "Home" => (Key::Named(NamedKey::Home), None),
            "End" => (Key::Named(NamedKey::End), None),
            "PageUp" => (Key::Named(NamedKey::PageUp), None),
            "PageDown" => (Key::Named(NamedKey::PageDown), None),
            "Enter" => (Key::Named(NamedKey::Enter), None),
            "Escape" => (Key::Named(NamedKey::Escape), None),
            "Tab" => (Key::Named(NamedKey::Tab), None),
            other => (Key::Character(other.into()), Some(other)),
        };
        self.handle_key_input(&key, text);
    }

    /// Programmatically dispatch a click on a button by its id.
    pub fn click_button(&mut self, button_id: &str) -> bool {
        let target = self
            .layout_boxes
            .iter()
            .find(|b| {
                b.tag == "button"
                    && (b.id.as_deref() == Some(button_id)
                        || b.on_click.as_deref() == Some(button_id))
            })
            .cloned();
        if let Some(b) = target {
            self.cursor_pos = (b.rect.x + b.rect.w / 2, b.rect.y + b.rect.h / 2);
            self.handle_click();
            true
        } else {
            false
        }
    }
}

impl<D: UiDispatcher> ApplicationHandler for NativeApp<D> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let win_attrs = WindowAttributes::default()
                .with_title(&self.config.title)
                .with_inner_size(LogicalSize::new(self.config.width, self.config.height));

            let window = match event_loop.create_window(win_attrs) {
                Ok(w) => Arc::new(w),
                Err(_) => return,
            };

            let context = match softbuffer::Context::new(window.clone()) {
                Ok(c) => c,
                Err(_) => return,
            };

            let surface = match softbuffer::Surface::new(&context, window.clone()) {
                Ok(s) => s,
                Err(_) => return,
            };

            self.window = Some(window);
            self.context = Some(context);
            self.surface = Some(surface);

            self.render();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.width = size.width.max(1);
                self.height = size.height.max(1);
                if let (Some(surface), Some(w), Some(h)) = (
                    &mut self.surface,
                    NonZeroU32::new(self.width),
                    NonZeroU32::new(self.height),
                ) {
                    let _ = surface.resize(w, h);
                }
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.render();
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.handle_cursor_moved((position.x as i32, position.y as i32)) {
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.handle_click();
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    MouseScrollDelta::LineDelta(dx, dy) => {
                        self.scroll_by((-dx * 30.0) as i32, (-dy * 30.0) as i32);
                    }
                    MouseScrollDelta::PixelDelta(pos) => {
                        self.scroll_by(-pos.x as i32, -pos.y as i32);
                    }
                }
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift_pressed = modifiers.state().shift_key();
                self.ctrl_pressed =
                    modifiers.state().control_key() || modifiers.state().super_key();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        text,
                        ..
                    },
                ..
            } => {
                self.handle_key_input(&logical_key, text.as_deref());
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(runner) = self.test_runner.take() {
            runner(self, event_loop);
        }
    }
}

/// Create a winit EventLoop configured to run on any thread (needed for tests / background threads).
pub fn create_event_loop() -> Result<EventLoop<()>, String> {
    let mut builder = EventLoop::builder();
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use winit::platform::wayland::EventLoopBuilderExtWayland;
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoopBuilderExtX11::with_any_thread(&mut builder, true);
        EventLoopBuilderExtWayland::with_any_thread(&mut builder, true);
    }
    builder
        .build()
        .map_err(|e| format!("failed to initialize winit event loop: {e}"))
}

/// Run native event loop with softbuffer presenting to an OS window, with an optional test runner.
pub fn run_window_with_test_runner<D: UiDispatcher>(
    config: WindowConfig,
    dispatcher: D,
    test_runner: Option<fn(&mut NativeApp<D>, &ActiveEventLoop)>,
) -> Result<(), String> {
    let event_loop = create_event_loop()?;
    let mut app = NativeApp::new(config, dispatcher);
    app.test_runner = test_runner;
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("error during native window execution: {e}"))?;
    Ok(())
}

/// Run native event loop with softbuffer presenting to an OS window.
pub fn run_window<D: UiDispatcher>(config: WindowConfig, dispatcher: D) -> Result<(), String> {
    run_window_with_test_runner(config, dispatcher, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyDisp<'a> {
        val: &'a mut i32,
        notes: &'a mut String,
        last_key: &'a mut String,
    }

    impl<'a> UiDispatcher for DummyDisp<'a> {
        fn root(&mut self) -> Option<UiElement> {
            None
        }
        fn dispatch_click(&mut self, _button_id: &str) {
            *self.val += 1;
        }
        fn dispatch_change(&mut self, _element_id: &str, value: &str) {
            *self.notes = value.to_string();
        }
        fn dispatch_key(&mut self, key: &str, _target_id: Option<&str>) {
            *self.last_key = key.to_string();
        }
    }

    #[test]
    fn test_dispatcher_compiles_with_lifetimes() {
        let mut x = 42;
        let mut notes = String::new();
        let mut key = String::new();
        let mut d = DummyDisp {
            val: &mut x,
            notes: &mut notes,
            last_key: &mut key,
        };
        d.dispatch_click("btn");
        d.dispatch_change("inp", "text");
        d.dispatch_key("Enter", None);
        // NLL ends d's &mut borrows at its last use, so x/notes/key are readable
        // here; an explicit drop(d) would trip clippy's drop_non_drop (DummyDisp
        // has no Drop impl).
        assert_eq!(x, 43);
        assert_eq!(notes, "text");
        assert_eq!(key, "Enter");
    }

    #[test]
    fn test_layout_column_and_button_boxes() {
        let mut col = UiElement::new("column");
        col.props.insert("padding".into(), "10".into());
        col.props.insert("gap".into(), "5".into());

        let mut txt = UiElement::new("text");
        txt.text = Some("Count: 0".into());

        let mut btn = UiElement::new("button");
        btn.id = Some("btn_inc".into());
        btn.text = Some("Increment".into());
        btn.on_click = Some("inc".into());

        let mut inp = UiElement::new("input");
        inp.id = Some("input_val".into());
        inp.text = Some("edit me".into());
        inp.on_change = Some("set_val".into());

        col.children.push(txt);
        col.children.push(btn);
        col.children.push(inp);

        let mut boxes = Vec::new();
        let (w, h) = layout_element(&col, 0, 0, 300, &mut boxes);
        assert!(w > 0);
        assert!(h > 0);
        assert_eq!(boxes.len(), 3);
        assert_eq!(boxes[0].tag, "text");
        assert_eq!(boxes[1].tag, "button");
        assert_eq!(boxes[1].id.as_deref(), Some("btn_inc"));
        assert!(boxes[1]
            .rect
            .contains(boxes[1].rect.x + 2, boxes[1].rect.y + 2));
        assert_eq!(boxes[2].tag, "input");
        assert_eq!(boxes[2].id.as_deref(), Some("input_val"));
    }

    #[test]
    fn test_draw_char_and_text_rasterization() {
        let mut fb = vec![0u32; 100 * 50];
        draw_text(&mut fb, 100, 50, 10, 10, "Hi", 0xFFFFFFFF, 1);
        let non_zero_pixels = fb.iter().filter(|&&p| p == 0xFFFFFFFF).count();
        assert!(non_zero_pixels > 0, "must rasterize character glyphs");
    }

    #[test]
    fn test_non_ascii_glyph_rendering_uses_placeholder() {
        let mut fb_ascii = vec![0u32; 16 * 16];
        let mut fb_non_ascii = vec![0u32; 16 * 16];
        let mut fb_del = vec![0u32; 16 * 16];

        draw_char(&mut fb_ascii, 16, 16, 0, 0, '?', 0xFFFFFFFF, 1);
        draw_char(&mut fb_non_ascii, 16, 16, 0, 0, '🦀', 0xFFFFFFFF, 1);
        // DEL glyph is ASCII 127
        draw_char(&mut fb_del, 16, 16, 0, 0, 127 as char, 0xFFFFFFFF, 1);

        assert_eq!(
            fb_non_ascii, fb_ascii,
            "non-ASCII characters should be rendered with '?' placeholder glyph"
        );
        assert_ne!(
            fb_non_ascii, fb_del,
            "non-ASCII characters must not render the DEL control glyph"
        );
    }

    #[test]
    fn test_multiline_click_coordinates_maps_to_line() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let dummy = DummyDisp {
            val: &mut val,
            notes: &mut notes,
            last_key: &mut key,
        };
        let mut app = NativeApp::new(
            WindowConfig {
                title: "Test".into(),
                width: 400,
                height: 300,
            },
            dummy,
        );

        // Set up layout boxes directly with an input at (10, 20) with multiline text
        app.layout_boxes = vec![LayoutBox {
            rect: Rect {
                x: 10,
                y: 20,
                w: 300,
                h: 100,
            },
            tag: "input".into(),
            id: Some("multiline_input".into()),
            text: Some("line0\nline1\nline2".into()),
            on_click: None,
            on_change: None,
            on_key: None,
        }];

        // Simulate click on line 1, col 2:
        // x: lbox.rect.x + 8 + 2 * 16 = 10 + 8 + 32 = 50
        // y: lbox.rect.y + 8 + 1 * 20 = 20 + 8 + 20 = 48
        app.cursor_pos = (50, 48);
        app.handle_click();

        assert_eq!(app.focused_id.as_deref(), Some("multiline_input"));
        let st = app.text_inputs.get("multiline_input").unwrap();
        // line 0 is "line0\n" (len 6), line 1 col 2 is char index 6 + 2 = 8 ('n')
        assert_eq!(
            st.cursor, 8,
            "click on line 1 must map to char index on line 1"
        );

        // Simulate click on line 2, col 3:
        // x: 10 + 8 + 3 * 16 = 66
        // y: 20 + 8 + 2 * 20 = 68
        app.cursor_pos = (66, 68);
        app.handle_click();

        let st = app.text_inputs.get("multiline_input").unwrap();
        // line 0 (6) + line 1 (6) + col 3 = 15 ('e')
        assert_eq!(
            st.cursor, 15,
            "click on line 2 must map to char index on line 2"
        );
    }

    #[test]
    #[ignore = "requires an active display (X11/Wayland/Xvfb); run via scripts/gui_acceptance.sh"]
    fn test_native_gui_desktop_acceptance() {
        if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
            eprintln!(
                "Skipping desktop acceptance test: neither DISPLAY nor WAYLAND_DISPLAY is set."
            );
            return;
        }

        // 1. Load small .ndg UI (examples/gui_counter.ndg)
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let ndg_path = manifest_dir.join("../../examples/gui_counter.ndg");
        let src = std::fs::read_to_string(&ndg_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", ndg_path.display()));

        let tokens = nudgec::lexer::lex(&src).expect("lex failed");
        let items = nudgec::parser::parse(tokens).expect("parse failed");
        let mut interp = nudgec::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load_items failed");

        let env = std::rc::Rc::new(std::cell::RefCell::new(nudgec::eval::env::Env::new()));
        let app_id = interp
            .mount_headless_app(&Some("CounterApp".to_string()), "MainWindow", &env)
            .expect("mount MainWindow failed");

        fn convert_ui_element(from: &nudgec::nudge_runtime::ui::UiElement) -> UiElement {
            UiElement {
                tag: from.tag.clone(),
                id: from.id.clone(),
                text: from.text.clone(),
                props: from.props.clone(),
                children: from.children.iter().map(convert_ui_element).collect(),
                on_click: from.on_click.clone(),
                on_change: from.on_change.clone(),
                on_key: from.on_key.clone(),
            }
        }

        struct TestDispatcher<'a> {
            interp: &'a mut nudgec::eval::interpreter::Interpreter,
            app_id: usize,
            env: &'a std::rc::Rc<std::cell::RefCell<nudgec::eval::env::Env>>,
            synthetic_note: String,
            last_key: String,
        }

        impl<'a> UiDispatcher for TestDispatcher<'a> {
            fn root(&mut self) -> Option<UiElement> {
                let elem = self
                    .interp
                    .headless_apps
                    .get(self.app_id)
                    .and_then(|a| a.root.as_ref())?;
                let mut root = convert_ui_element(elem);

                // Add an input and scrollable text items dynamically for acceptance test coverage
                let mut inp = UiElement::new("input");
                inp.id = Some("test_inp".to_string());
                inp.props
                    .insert("value".to_string(), self.synthetic_note.clone());
                inp.on_change = Some("set_note".to_string());
                inp.on_key = Some("handle_key".to_string());
                root.children.push(inp);

                // Add items to force overflow for scrolling test
                for i in 0..15 {
                    let mut extra_txt = UiElement::new("text");
                    extra_txt.text = Some(format!("Row item {i}"));
                    root.children.push(extra_txt);
                }

                Some(root)
            }

            fn dispatch_click(&mut self, button_id: &str) {
                let _ = self
                    .interp
                    .dispatch_click_headless(self.app_id, button_id, self.env);
            }

            fn dispatch_change(&mut self, _element_id: &str, value: &str) {
                self.synthetic_note = value.to_string();
            }

            fn dispatch_key(&mut self, key: &str, _target_id: Option<&str>) {
                self.last_key = key.to_string();
            }
        }

        let config = WindowConfig {
            title: "Nudge Desktop Acceptance Test".into(),
            width: 300,
            height: 200,
        };

        fn acceptance_runner(app: &mut NativeApp<TestDispatcher>, event_loop: &ActiveEventLoop) {
            println!("[desktop-gui-test] Event loop started, verifying OS window creation...");
            assert!(app.window.is_some(), "winit OS window was not created");
            assert!(app.surface.is_some(), "softbuffer surface was not created");
            let win = app.window.as_ref().unwrap();
            let size = win.inner_size();
            println!(
                "[desktop-gui-test] Real OS window created: title={:?}, inner_size={}x{}",
                app.config.title, size.width, size.height
            );

            // Assert initial render happened
            // Xvfb/CI can render an extra initial frame (DPI/scale/expose) before this
            // point, so assert "at least one initial frame" instead of an exact count.
            assert!(
                app.render_count >= 1,
                "initial frame should be rendered (got {})",
                app.render_count
            );
            let text_before = app
                .layout_boxes
                .iter()
                .find(|b| b.tag == "text")
                .and_then(|b| b.text.as_deref())
                .unwrap_or("");
            assert_eq!(
                text_before, "Counter: 0",
                "initial rendered text must be 'Counter: 0'"
            );
            println!(
                "[desktop-gui-test] Initial view rendered: {:?}",
                text_before
            );

            // Check initial agent state in interpreter
            let val_before = app
                .dispatcher
                .interp
                .agent_store
                .get_field("CounterApp", "val");
            assert_eq!(
                val_before,
                Ok(nudgec::eval::val::NudgeVal::Int(0)),
                "initial agent state val must be 0"
            );
            println!("[desktop-gui-test] Initial agent state verified: CounterApp.state.val = 0");

            // Dispatch click on 'btn_inc' through window path
            println!("[desktop-gui-test] Programmatically dispatching click to 'btn_inc'...");
            let renders_before_click = app.render_count;
            let clicked = app.click_button("btn_inc");
            assert!(clicked, "button 'btn_inc' must be found and clicked");
            println!("[desktop-gui-test] Click event dispatched through runtime handler");

            // Check agent state changed
            let val_after = app
                .dispatcher
                .interp
                .agent_store
                .get_field("CounterApp", "val");
            assert_eq!(
                val_after,
                Ok(nudgec::eval::val::NudgeVal::Int(1)),
                "agent state val must increment to 1 after click"
            );
            println!("[desktop-gui-test] Agent state change verified: CounterApp.state.val = 1");

            // Check re-render happened (relative: absolute counts differ across
            // environments because the initial frame count varies)
            assert!(
                app.render_count > renders_before_click,
                "framebuffer re-render must have fired after event dispatch (before={}, after={})",
                renders_before_click,
                app.render_count
            );
            let text_after = app
                .layout_boxes
                .iter()
                .find(|b| b.tag == "text")
                .and_then(|b| b.text.as_deref())
                .unwrap_or("");
            assert_eq!(
                text_after, "Counter: 1",
                "re-rendered text must be 'Counter: 1'"
            );
            println!(
                "[desktop-gui-test] Framebuffer re-render verified: {:?} (render_count={})",
                text_after, app.render_count
            );

            // 4. Test Text Input & Editing on real OS window
            println!("[desktop-gui-test] Testing native text input editing on 'test_inp'...");
            let focused = app.focus_input("test_inp");
            assert!(focused, "input 'test_inp' must be focused");
            assert_eq!(app.focused_id.as_deref(), Some("test_inp"));

            app.type_into_focused("Hello Nudge");
            assert_eq!(
                app.dispatcher.synthetic_note, "Hello Nudge",
                "on_change handler must receive typed text"
            );

            // Test backspace key
            app.send_key("Backspace");
            assert_eq!(
                app.dispatcher.synthetic_note, "Hello Nudg",
                "Backspace must delete last char and fire on_change"
            );

            // Test cursor navigation and delete
            app.send_key("Home");
            app.send_key("Delete");
            assert_eq!(
                app.dispatcher.synthetic_note, "ello Nudg",
                "Delete at Home must remove the first char"
            );
            app.send_key("End");
            app.send_key("!");
            assert_eq!(
                app.dispatcher.synthetic_note, "ello Nudg!",
                "Typing at End must append"
            );
            println!("[desktop-gui-test] Text input editing verified successfully!");

            // 5. Test Scrolling on real OS window
            println!("[desktop-gui-test] Testing viewport scrolling on real window...");
            assert_eq!(app.scroll_y, 0, "initial scroll_y must be 0");
            app.scroll_by(0, 60);
            assert_eq!(app.scroll_y, 60, "scroll_y must advance by 60");
            println!(
                "[desktop-gui-test] Viewport scroll verified: scroll_offset=({}, {})",
                app.scroll_x, app.scroll_y
            );

            // 6. Test Keyboard Event Dispatch
            println!("[desktop-gui-test] Testing keyboard event dispatch...");
            app.send_key("Enter");
            assert_eq!(
                app.dispatcher.last_key, "Enter",
                "on_key handler must receive Enter event"
            );
            println!("[desktop-gui-test] Keyboard event dispatch verified successfully!");

            println!("[desktop-gui-test] All desktop acceptance assertions passed. Exiting winit event loop.");
            event_loop.exit();
        }

        let dispatcher = TestDispatcher {
            interp: &mut interp,
            app_id,
            env: &env,
            synthetic_note: String::new(),
            last_key: String::new(),
        };

        run_window_with_test_runner(config, dispatcher, Some(acceptance_runner))
            .expect("run_window execution failed");

        let final_val = interp
            .agent_store
            .get_field("CounterApp", "val")
            .expect("read CounterApp.state.val");
        assert_eq!(
            final_val,
            nudgec::eval::val::NudgeVal::Int(1),
            "final state in interpreter must be 1"
        );
        println!("[desktop-gui-test] Desktop acceptance test successfully completed!");
    }

    #[test]
    fn test_unicode_glyphs_rendered_distinct_from_question_mark() {
        let question_glyph = crate::font8x8::get_glyph('?');
        let blank_glyph = [0u8; 8];

        let test_chars = [
            'ğ', 'ü', 'ş', 'ö', 'ç', 'ı', 'é', '•', '€', 'Ğ', 'Ü', 'Ş', 'Ö', 'Ç', 'İ',
        ];
        for &ch in &test_chars {
            let glyph = crate::font8x8::get_glyph(ch);
            assert_ne!(glyph, blank_glyph, "glyph for '{ch}' must not be blank");
            assert_ne!(
                glyph, question_glyph,
                "glyph for '{ch}' must not render as '?'"
            );

            // Also test rasterizing into a framebuffer
            let mut fb = vec![0u32; 16 * 16];
            draw_char(&mut fb, 16, 16, 0, 0, ch, 0xFFFFFFFF, 1);
            let lit_pixels = fb.iter().filter(|&&p| p == 0xFFFFFFFF).count();
            assert!(
                lit_pixels > 0,
                "rasterized character '{ch}' must have lit pixels"
            );
        }
    }

    #[test]
    fn test_cursor_moved_redraw_optimization() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let dummy = DummyDisp {
            val: &mut val,
            notes: &mut notes,
            last_key: &mut key,
        };
        let mut app = NativeApp::new(
            WindowConfig {
                title: "Test".into(),
                width: 400,
                height: 300,
            },
            dummy,
        );

        // Add a button at (50, 50) with width 100, height 40
        app.layout_boxes = vec![LayoutBox {
            rect: Rect {
                x: 50,
                y: 50,
                w: 100,
                h: 40,
            },
            tag: "button".into(),
            id: Some("test_btn".into()),
            text: Some("Click".into()),
            on_click: Some("click".into()),
            on_change: None,
            on_key: None,
        }];

        // Initially at (0, 0) - not hovering
        assert_eq!(app.find_hovered_button((0, 0)), None);
        assert_eq!(app.hovered_id, None);

        // Moving within empty space (0,0) -> (10, 10): hover state unchanged (None -> None)
        let redraw_needed = app.handle_cursor_moved((10, 10));
        assert!(
            !redraw_needed,
            "moving in empty space must NOT trigger redraw"
        );
        assert_eq!(app.hovered_id, None);

        // Moving into button (60, 60): hover state changes (None -> Some("test_btn"))
        let redraw_needed = app.handle_cursor_moved((60, 60));
        assert!(redraw_needed, "entering button MUST trigger redraw");
        assert_eq!(app.hovered_id.as_deref(), Some("test_btn"));

        // Moving WITHIN the button (60, 60) -> (80, 70): hover state unchanged
        let redraw_needed = app.handle_cursor_moved((80, 70));
        assert!(
            !redraw_needed,
            "moving within same button must NOT trigger redraw"
        );
        assert_eq!(app.hovered_id.as_deref(), Some("test_btn"));

        // Exiting the button (80, 70) -> (200, 200): hover state changes (Some -> None)
        let redraw_needed = app.handle_cursor_moved((200, 200));
        assert!(redraw_needed, "exiting button MUST trigger redraw");
        assert_eq!(app.hovered_id, None);

        // Moving in empty space again
        let redraw_needed = app.handle_cursor_moved((250, 250));
        assert!(
            !redraw_needed,
            "moving in empty space again must NOT trigger redraw"
        );
    }

    #[test]
    fn test_text_word_wrapping_within_bounds() {
        let long_message = "This is a long user message in the chat conversation that exceeds the standard window width";
        let wrapped = wrap_text(long_message, 200, 16);
        let lines: Vec<&str> = wrapped.split('\n').collect();
        assert!(lines.len() > 1, "long text must wrap into multiple lines");
        for line in &lines {
            let line_px = (line.chars().count() as i32) * 16;
            assert!(
                line_px <= 200,
                "wrapped line '{line}' (px={line_px}) exceeds max width 200px"
            );
        }

        // Test layout_element on a text element
        let mut elem = UiElement::new("text");
        elem.text = Some(long_message.to_string());
        elem.props.insert("wrap".into(), "true".into());

        let mut boxes = Vec::new();
        let (_w, h) = layout_element(&elem, 10, 10, 200, &mut boxes);
        assert_eq!(boxes.len(), 1);
        assert!(boxes[0].rect.w <= 200, "box width must not exceed 200px");
        assert!(
            h > 20,
            "wrapped height must exceed single-line height of 20px"
        );
        assert_eq!(h, (lines.len() as i32) * 20);
        assert_eq!(boxes[0].rect.h, h);
    }

    #[test]
    fn test_native_app_send_key_undo_redo_and_dispatch_change() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let dummy = DummyDisp {
            val: &mut val,
            notes: &mut notes,
            last_key: &mut key,
        };
        let mut app = NativeApp::new(
            WindowConfig {
                title: "Test".into(),
                width: 400,
                height: 300,
            },
            dummy,
        );

        app.layout_boxes = vec![LayoutBox {
            rect: Rect {
                x: 10,
                y: 10,
                w: 200,
                h: 30,
            },
            tag: "input".into(),
            id: Some("test_input".into()),
            text: Some("initial".into()),
            on_click: None,
            on_change: Some("change_handler".into()),
            on_key: Some("key_handler".into()),
        }];

        assert!(app.focus_input("test_input"));
        assert_eq!(app.focused_id.as_deref(), Some("test_input"));

        // Type into the input
        app.type_into_focused(" edit");
        assert_eq!(
            app.text_inputs.get("test_input").unwrap().text,
            "initial edit"
        );
        assert_eq!(app.dispatcher.notes, "initial edit");

        // Send Undo key
        app.send_key("Undo");
        assert_eq!(app.text_inputs.get("test_input").unwrap().text, "initial");
        assert_eq!(app.dispatcher.notes, "initial");
        assert_eq!(app.dispatcher.last_key, "Ctrl+Undo");

        // Send Redo key
        app.send_key("Redo");
        assert_eq!(
            app.text_inputs.get("test_input").unwrap().text,
            "initial edit"
        );
        assert_eq!(app.dispatcher.notes, "initial edit");
        assert_eq!(app.dispatcher.last_key, "Ctrl+Redo");

        // Test Backspace then Undo then Redo
        app.send_key("Backspace");
        assert_eq!(
            app.text_inputs.get("test_input").unwrap().text,
            "initial edi"
        );
        assert_eq!(app.dispatcher.notes, "initial edi");

        app.send_key("Undo");
        assert_eq!(
            app.text_inputs.get("test_input").unwrap().text,
            "initial edit"
        );
        assert_eq!(app.dispatcher.notes, "initial edit");

        app.send_key("Redo");
        assert_eq!(
            app.text_inputs.get("test_input").unwrap().text,
            "initial edi"
        );
        assert_eq!(app.dispatcher.notes, "initial edi");

        // Test Ctrl+z and Ctrl+y aliases
        app.send_key("Ctrl+z");
        assert_eq!(
            app.text_inputs.get("test_input").unwrap().text,
            "initial edit"
        );
        app.send_key("Ctrl+y");
        assert_eq!(
            app.text_inputs.get("test_input").unwrap().text,
            "initial edi"
        );

        // Test Ctrl+Shift+z alias
        app.send_key("Ctrl+z");
        assert_eq!(
            app.text_inputs.get("test_input").unwrap().text,
            "initial edit"
        );
        app.send_key("Ctrl+Shift+z");
        assert_eq!(
            app.text_inputs.get("test_input").unwrap().text,
            "initial edi"
        );
    }
}
