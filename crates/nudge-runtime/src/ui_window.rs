//! Real native OS window backend for Nudge's UI using winit 0.30 and softbuffer 0.4.
#![cfg(feature = "native-gui")]

use crate::font8x8::FONT_8X8_BASIC;
use crate::ui::UiElement;
use std::num::NonZeroU32;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
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
    let code = (ch as usize).min(127);
    let glyph = FONT_8X8_BASIC[code];

    for (row, &byte) in glyph.iter().enumerate() {
        for col in 0..8 {
            if (byte & (1 << col)) != 0 {
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
    mut x: i32,
    y: i32,
    text: &str,
    color: u32,
    scale: usize,
) {
    let char_w = (8 * scale) as i32;
    for ch in text.chars() {
        if ch == '\n' {
            continue;
        }
        draw_char(buf, stride, height, x, y, ch, color, scale);
        x += char_w;
    }
}

// -----------------------------------------------------------------------------
// Layout Engine
// -----------------------------------------------------------------------------

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
            let content = elem.text.clone().unwrap_or_else(|| elem.get_text());
            let w = (content.len() as i32) * 16;
            let h = 20;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "text".into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: None,
                on_change: None,
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
            let text_w = (content.len() as i32) * 16;
            let w = (text_w + 32).max(80);
            let h = 36;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "button".into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: elem.on_click.clone().or_else(|| elem.id.clone()),
                on_change: None,
            });
            (w, h)
        }
        "input" => {
            let content = elem
                .text
                .clone()
                .or_else(|| elem.props.get("value").cloned())
                .unwrap_or_default();
            let w = 220.max((content.len() as i32) * 16 + 24);
            let h = 36;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "input".into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: None,
                on_change: elem.on_change.clone(),
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

// -----------------------------------------------------------------------------
// Application Handler and Event Loop
// -----------------------------------------------------------------------------

struct NativeApp<D: UiDispatcher> {
    config: WindowConfig,
    dispatcher: D,
    window: Option<Arc<Window>>,
    context: Option<softbuffer::Context<Arc<Window>>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    width: u32,
    height: u32,
    cursor_pos: (i32, i32),
    layout_boxes: Vec<LayoutBox>,
}

impl<D: UiDispatcher> NativeApp<D> {
    fn new(config: WindowConfig, dispatcher: D) -> Self {
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
            layout_boxes: Vec::new(),
        }
    }

    fn render(&mut self) {
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
        if let Some(root) = self.dispatcher.root() {
            layout_element(
                &root,
                10,
                10,
                self.width as i32 - 20,
                &mut self.layout_boxes,
            );
        }

        let (mx, my) = self.cursor_pos;

        // Render each layout box
        for lbox in &self.layout_boxes {
            match lbox.tag.as_str() {
                "text" => {
                    if let Some(ref text) = lbox.text {
                        draw_text(
                            &mut buffer,
                            stride,
                            h_usize,
                            lbox.rect.x,
                            lbox.rect.y + 2,
                            text,
                            0xFFF4F4F5, // crisp white
                            2,
                        );
                    }
                }
                "button" => {
                    let hovered = lbox.rect.contains(mx, my);
                    let bg_color = if hovered {
                        0xFF2563EB // Blue-600 hover
                    } else {
                        0xFF3B82F6 // Blue-500
                    };
                    draw_rect(
                        &mut buffer,
                        stride,
                        h_usize,
                        lbox.rect.x,
                        lbox.rect.y,
                        lbox.rect.w,
                        lbox.rect.h,
                        bg_color,
                    );
                    draw_rect_outline(
                        &mut buffer,
                        stride,
                        h_usize,
                        lbox.rect.x,
                        lbox.rect.y,
                        lbox.rect.w,
                        lbox.rect.h,
                        0xFF60A5FA, // lighter border
                    );
                    if let Some(ref text) = lbox.text {
                        let text_w = (text.len() as i32) * 16;
                        let tx = lbox.rect.x + ((lbox.rect.w - text_w) / 2).max(8);
                        let ty = lbox.rect.y + ((lbox.rect.h - 16) / 2).max(4);
                        draw_text(&mut buffer, stride, h_usize, tx, ty, text, 0xFFFFFFFF, 2);
                    }
                }
                "input" => {
                    draw_rect(
                        &mut buffer,
                        stride,
                        h_usize,
                        lbox.rect.x,
                        lbox.rect.y,
                        lbox.rect.w,
                        lbox.rect.h,
                        0xFF27272A, // Zinc-800
                    );
                    draw_rect_outline(
                        &mut buffer,
                        stride,
                        h_usize,
                        lbox.rect.x,
                        lbox.rect.y,
                        lbox.rect.w,
                        lbox.rect.h,
                        0xFF52525B, // Zinc-600
                    );
                    if let Some(ref text) = lbox.text {
                        draw_text(
                            &mut buffer,
                            stride,
                            h_usize,
                            lbox.rect.x + 8,
                            lbox.rect.y + 10,
                            text,
                            0xFFE4E4E7,
                            2,
                        );
                    }
                }
                _ => {}
            }
        }

        let _ = buffer.present();
    }

    fn handle_click(&mut self) {
        let (mx, my) = self.cursor_pos;
        let mut clicked_id = None;

        for lbox in &self.layout_boxes {
            if lbox.tag == "button" && lbox.rect.contains(mx, my) {
                if let Some(ref id) = lbox.id {
                    clicked_id = Some(id.clone());
                    break;
                } else if let Some(ref on_click) = lbox.on_click {
                    clicked_id = Some(on_click.clone());
                    break;
                }
            }
        }

        if let Some(button_id) = clicked_id {
            self.dispatcher.dispatch_click(&button_id);
            self.render();
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
                self.cursor_pos = (position.x as i32, position.y as i32);
                if let Some(win) = &self.window {
                    win.request_redraw();
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
            _ => {}
        }
    }
}

/// Run native event loop with softbuffer presenting to an OS window.
pub fn run_window<D: UiDispatcher>(config: WindowConfig, dispatcher: D) -> Result<(), String> {
    let event_loop =
        EventLoop::new().map_err(|e| format!("failed to initialize winit event loop: {e}"))?;
    let mut app = NativeApp::new(config, dispatcher);
    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("error during native window execution: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyDisp<'a> {
        val: &'a mut i32,
    }

    impl<'a> UiDispatcher for DummyDisp<'a> {
        fn root(&mut self) -> Option<UiElement> {
            None
        }
        fn dispatch_click(&mut self, _button_id: &str) {
            *self.val += 1;
        }
    }

    #[test]
    fn test_dispatcher_compiles_with_lifetimes() {
        let mut x = 42;
        let mut d = DummyDisp { val: &mut x };
        d.dispatch_click("btn");
        assert_eq!(x, 43);
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

        col.children.push(txt);
        col.children.push(btn);

        let mut boxes = Vec::new();
        let (w, h) = layout_element(&col, 0, 0, 300, &mut boxes);
        assert!(w > 0);
        assert!(h > 0);
        assert_eq!(boxes.len(), 2);
        assert_eq!(boxes[0].tag, "text");
        assert_eq!(boxes[1].tag, "button");
        assert_eq!(boxes[1].id.as_deref(), Some("btn_inc"));
        assert!(boxes[1]
            .rect
            .contains(boxes[1].rect.x + 2, boxes[1].rect.y + 2));
    }

    #[test]
    fn test_draw_char_and_text_rasterization() {
        let mut fb = vec![0u32; 100 * 50];
        draw_text(&mut fb, 100, 50, 10, 10, "Hi", 0xFFFFFFFF, 1);
        let non_zero_pixels = fb.iter().filter(|&&p| p == 0xFFFFFFFF).count();
        assert!(non_zero_pixels > 0, "must rasterize character glyphs");
    }
}
