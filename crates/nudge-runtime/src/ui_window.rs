//! Real native OS window backend for Nudge's UI using winit 0.30 and softbuffer 0.4.
#![cfg(feature = "native-gui")]

use crate::font8x8::get_glyph;
use crate::font_engine::{FaceKind, FontEngine, BASE_PX, CELL_H, CELL_W};
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

/// Central color theme. Every hard-coded color in the renderer resolves
/// through a `Theme` field, so light/dark and custom palettes only need to
/// change here. Colors are stored as opaque `0xFFRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub bg: u32,
    pub text: u32,
    pub text_dim: u32,
    pub border: u32,
    pub input_bg: u32,
    pub input_border: u32,
    pub input_border_focused: u32,
    pub button_bg: u32,
    pub button_bg_hover: u32,
    pub button_border: u32,
    pub button_text: u32,
    pub selection_bg: u32,
    pub cursor: u32,
    pub scrollbar_track: u32,
    pub scrollbar_thumb: u32,
    /// Chat bubble backgrounds/text. The user's own messages read as the
    /// accent colour on the right; the assistant's read as a raised surface
    /// on the left, so the two sides are told apart without relying on
    /// alignment alone.
    pub bubble_user_bg: u32,
    pub bubble_user_text: u32,
    pub bubble_assistant_bg: u32,
    pub bubble_assistant_text: u32,
}

impl Theme {
    /// Zinc-900 dark theme (the original Studio palette).
    pub const DARK: Theme = Theme {
        bg: 0xFF18_181B,                    // Zinc-900
        text: 0xFFF4_F4F5,                  // Zinc-100
        text_dim: 0xFF71_717A,              // Zinc-500
        border: 0xFF3F_3F46,                // Zinc-700
        input_bg: 0xFF27_272A,              // Zinc-800
        input_border: 0xFF52_525B,          // Zinc-600
        input_border_focused: 0xFF3B_82F6,  // Blue-500
        button_bg: 0xFF3B_82F6,             // Blue-500
        button_bg_hover: 0xFF25_63EB,       // Blue-600
        button_border: 0xFF60_A5FA,         // Blue-400
        button_text: 0xFFFF_FFFF,           // white
        selection_bg: 0xFF1D_4ED8,          // Blue-700
        cursor: 0xFFFF_FFFF,                // white
        scrollbar_track: 0xFF27_272A,       // Zinc-800
        scrollbar_thumb: 0xFF71_717A,       // Zinc-500
        bubble_user_bg: 0xFF3B_82F6,        // Blue-500
        bubble_user_text: 0xFFFF_FFFF,      // white
        bubble_assistant_bg: 0xFF27_272A,   // Zinc-800
        bubble_assistant_text: 0xFFF4_F4F5, // Zinc-100
    };
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

/// A scroll viewport a box must be clipped to.
///
/// `rect` lives in **content space** (the same space as [`LayoutBox::rect`]).
/// `pan` is the scroll offset of the viewport's *ancestors* — a viewport does
/// not move with its own scroll offset, only its content does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipRegion {
    pub rect: Rect,
    pub pan: (i32, i32),
}

/// A scroll viewport measured by layout, kept for wheel/keyboard routing and
/// scrollbar drawing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrollRegion {
    pub key: String,
    /// Viewport in content space.
    pub rect: Rect,
    /// Total height of the content inside the viewport (content space).
    pub content_h: i32,
    /// Ancestors' pan, used to map the viewport into screen space.
    pub pan: (i32, i32),
}

impl ScrollRegion {
    /// How far the content can scroll before the viewport bottom is reached.
    pub fn max_offset(&self) -> i32 {
        (self.content_h - self.rect.h).max(0)
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
    /// Scroll offset of the enclosing scroll containers: screen position is
    /// `rect - (scroll_x + pan.0, scroll_y + pan.1)`. `(0, 0)` when not inside
    /// any scroll container.
    pub pan: (i32, i32),
    /// Viewports this box must be clipped to. `None` when not scrollable.
    pub clips: Option<Vec<ClipRegion>>,
    /// Inner padding for boxes that draw a background larger than their
    /// content (chat bubbles). Text is drawn at `rect + pad`; `0` elsewhere.
    pub pad: i32,
}

impl LayoutBox {
    /// Screen-space rectangle, mapping content coordinates through the window
    /// scroll and the enclosing scroll containers.
    pub fn screen_rect(&self, scroll_x: i32, scroll_y: i32) -> Rect {
        Rect {
            x: self.rect.x - scroll_x - self.pan.0,
            y: self.rect.y - scroll_y - self.pan.1,
            w: self.rect.w,
            h: self.rect.h,
        }
    }

    /// Intersected screen-space clip for this box, or `None` when unclipped.
    pub fn screen_clip(&self, scroll_x: i32, scroll_y: i32) -> Option<Rect> {
        let clips = self.clips.as_ref()?;
        let mut acc: Option<Rect> = None;
        for c in clips {
            let r = Rect {
                x: c.rect.x - scroll_x - c.pan.0,
                y: c.rect.y - scroll_y - c.pan.1,
                w: c.rect.w,
                h: c.rect.h,
            };
            acc = Some(match acc {
                None => r,
                Some(a) => intersect(a, r)?,
            });
        }
        acc
    }

    /// True when the box is at least partially visible in screen space.
    pub fn visible_in(&self, scroll_x: i32, scroll_y: i32, w: i32, h: i32) -> bool {
        let r = self.screen_rect(scroll_x, scroll_y);
        if r.x + r.w < 0 || r.y + r.h < 0 || r.x >= w || r.y >= h {
            return false;
        }
        match self.screen_clip(scroll_x, scroll_y) {
            None => true,
            Some(c) => intersect(r, c).is_some(),
        }
    }
}

/// Rectangle intersection; `None` when the result is empty.
#[inline]
pub fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    if x1 <= x0 || y1 <= y0 {
        None
    } else {
        Some(Rect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        })
    }
}

// -----------------------------------------------------------------------------
// Software Rasterization Primitives
// -----------------------------------------------------------------------------

/// Blend `color` over the existing pixel at `buf[y*stride + x]` using `alpha`
/// in `[0.0, 1.0]`. `alpha = 1.0` is fully opaque (matches old behaviour).
#[inline]
fn blend_px(buf: &mut [u32], idx: usize, color: u32, alpha: f32) {
    let a = alpha.clamp(0.0, 1.0);
    if a >= 1.0 {
        buf[idx] = color;
        return;
    }
    if a <= 0.0 {
        return;
    }
    let dst = buf[idx];
    let sr = ((color >> 16) & 0xFF) as f32;
    let sg = ((color >> 8) & 0xFF) as f32;
    let sb = (color & 0xFF) as f32;
    let dr = ((dst >> 16) & 0xFF) as f32;
    let dg = ((dst >> 8) & 0xFF) as f32;
    let db = (dst & 0xFF) as f32;
    let r = (sr * a + dr * (1.0 - a)) as u32;
    let g = (sg * a + dg * (1.0 - a)) as u32;
    let b = (sb * a + db * (1.0 - a)) as u32;
    buf[idx] = 0xFF00_0000 | (r << 16) | (g << 8) | b;
}

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
    draw_rect_alpha(buf, stride, height, x, y, w, h, color, 1.0);
}

/// `draw_rect` with an alpha channel in `[0.0, 1.0]` (1.0 = opaque).
#[allow(clippy::too_many_arguments)]
pub fn draw_rect_alpha(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
    alpha: f32,
) {
    let x_start = x.max(0) as usize;
    let y_start = y.max(0) as usize;
    let x_end = ((x + w).max(0) as usize).min(stride);
    let y_end = ((y + h).max(0) as usize).min(height);

    for row in y_start..y_end {
        let row_offset = row * stride;
        for col in x_start..x_end {
            let idx = row_offset + col;
            if alpha >= 1.0 {
                buf[idx] = color;
            } else {
                blend_px(buf, idx, color, alpha);
            }
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

/// Corner radius (px) used by the default widget chrome (buttons, inputs).
/// Kept modest so 36px-tall controls still read as rectangles, but enough to
/// lose the hard 90-degree corners.
pub const CORNER_RADIUS: i32 = 6;

/// Anti-aliased coverage of a rounded rectangle at the centre of pixel
/// `(px, py)` using the standard rounded-box signed distance field.
///
/// Returns `1.0` well inside, `0.0` outside, and a graded value across the
/// ~1px arc so corners are smooth instead of stair-stepped.
#[inline]
fn rounded_rect_coverage(x: i32, y: i32, w: i32, h: i32, r: i32, px: i32, py: i32) -> f32 {
    // Never round more than half the widget, otherwise the shape inverts.
    let r = (r as f32).min(w as f32 / 2.0).min(h as f32 / 2.0).max(0.0);
    let hw = w as f32 / 2.0;
    let hh = h as f32 / 2.0;
    let dx = (px as f32 + 0.5 - (x as f32 + hw)).abs();
    let dy = (py as f32 + 0.5 - (y as f32 + hh)).abs();
    let qx = dx - hw + r;
    let qy = dy - hh + r;
    let sdf = qx.max(qy).min(0.0) + qx.max(0.0).hypot(qy.max(0.0)) - r;
    (0.5 - sdf).clamp(0.0, 1.0)
}

/// [`draw_rect_alpha`] with rounded corners and anti-aliased edges.
#[allow(clippy::too_many_arguments)]
pub fn draw_rounded_rect_alpha(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
    color: u32,
    alpha: f32,
) {
    draw_rounded_rect_alpha_clipped(buf, stride, height, None, x, y, w, h, r, color, alpha);
}

/// Same as [`draw_rounded_rect_alpha`], but every pixel is limited to `clip`
/// (e.g. a scroll viewport). The shape keeps its true position, so a control
/// half-scrolled out of view is sliced exactly at the viewport edge.
#[allow(clippy::too_many_arguments)]
pub fn draw_rounded_rect_alpha_clipped(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
    color: u32,
    alpha: f32,
) {
    if w <= 0 || h <= 0 || alpha <= 0.0 {
        return;
    }
    let r = r.min(w / 2).min(h / 2);
    if r <= 0 {
        match clip.and_then(|c| intersect(Rect { x, y, w, h }, c)) {
            None => {
                if clip.is_none() {
                    draw_rect_alpha(buf, stride, height, x, y, w, h, color, alpha);
                }
            }
            Some(r) => draw_rect_alpha(buf, stride, height, r.x, r.y, r.w, r.h, color, alpha),
        }
        return;
    }

    let x_start = x.max(0) as usize;
    let y_start = y.max(0) as usize;
    let x_end = ((x + w).max(0) as usize).min(stride);
    let y_end = ((y + h).max(0) as usize).min(height);

    for row in y_start..y_end {
        let row_offset = row * stride;
        for col in x_start..x_end {
            let (px, py) = (col as i32, row as i32);
            if let Some(c) = clip {
                if px < c.x || py < c.y || px >= c.x + c.w || py >= c.y + c.h {
                    continue;
                }
            }
            let cov = rounded_rect_coverage(x, y, w, h, r, px, py);
            if cov <= 0.0 {
                continue;
            }
            let a = (cov * alpha).clamp(0.0, 1.0);
            if a >= 1.0 {
                buf[row_offset + col] = color;
            } else {
                blend_px(buf, row_offset + col, color, a);
            }
        }
    }
}

/// Rounded rectangle with a 1px border: the ring is drawn in `border`, the
/// interior in `fill`. Both the outer arc and the inner edge are
/// anti-aliased, and the two regions are complementary so nothing is
/// overdrawn twice.
#[allow(clippy::too_many_arguments)]
pub fn draw_rounded_rect_box(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
    fill: u32,
    border: u32,
) {
    draw_rounded_rect_box_clipped(buf, stride, height, None, x, y, w, h, r, fill, border);
}

/// Same as [`draw_rounded_rect_box`], but no pixel outside `clip` is touched,
/// so chrome inside a scroll viewport is sliced at the viewport edge instead
/// of bleeding over neighbouring content.
#[allow(clippy::too_many_arguments)]
pub fn draw_rounded_rect_box_clipped(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
    fill: u32,
    border: u32,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    if let Some(c) = clip {
        if intersect(Rect { x, y, w, h }, c).is_none() {
            return;
        }
    }
    let r = r.min(w / 2).min(h / 2);
    if r <= 0 {
        match clip.and_then(|c| intersect(Rect { x, y, w, h }, c)) {
            None => {
                if clip.is_none() {
                    draw_rect(buf, stride, height, x, y, w, h, fill);
                    draw_rect_outline(buf, stride, height, x, y, w, h, border);
                }
            }
            Some(rc) => {
                draw_rect(buf, stride, height, rc.x, rc.y, rc.w, rc.h, fill);
                draw_rect_outline(buf, stride, height, rc.x, rc.y, rc.w, rc.h, border);
            }
        }
        return;
    }

    // Interior after removing the 1px border on every side.
    let (ix, iy, iw, ih) = (x + 1, y + 1, w - 2, h - 2);

    let x_start = x.max(0) as usize;
    let y_start = y.max(0) as usize;
    let x_end = ((x + w).max(0) as usize).min(stride);
    let y_end = ((y + h).max(0) as usize).min(height);

    for row in y_start..y_end {
        let row_offset = row * stride;
        for col in x_start..x_end {
            let (px, py) = (col as i32, row as i32);
            if let Some(c) = clip {
                if px < c.x || py < c.y || px >= c.x + c.w || py >= c.y + c.h {
                    continue;
                }
            }
            let outer = rounded_rect_coverage(x, y, w, h, r, px, py);
            if outer <= 0.0 {
                continue;
            }
            let inner = if iw > 0 && ih > 0 {
                rounded_rect_coverage(ix, iy, iw, ih, r - 1, px, py)
            } else {
                0.0
            };
            let border_a = outer * (1.0 - inner);
            if border_a > 0.0 {
                if border_a >= 1.0 {
                    buf[row_offset + col] = border;
                } else {
                    blend_px(buf, row_offset + col, border, border_a);
                }
            }
            if inner > 0.0 {
                if inner >= 1.0 {
                    buf[row_offset + col] = fill;
                } else {
                    blend_px(buf, row_offset + col, fill, inner);
                }
            }
        }
    }
}

/// Draw a single glyph using the real TrueType engine with grayscale
/// anti-aliasing. `x, y` is the top-left of the line box; the baseline is
/// derived from the font's ascent. `draw_px(x, y, w, h, coverage)` receives
/// each covered pixel with coverage in `[0.0, 1.0]`.
pub fn draw_char_ttf<F>(
    engine: &FontEngine,
    kind: FaceKind,
    x: i32,
    y: i32,
    ch: char,
    mut draw_px: F,
) where
    F: FnMut(i32, i32, i32, i32, f32),
{
    engine.draw_glyph(kind, ch, x, y, |gx, gy, w, h, cov| {
        draw_px(gx, gy, w, h, cov);
    });
}

/// Draw `text` with the real TrueType engine, honouring `\n` line breaks and
/// proportional glyph advances (tight, natural spacing).
#[allow(clippy::too_many_arguments)]
pub fn draw_text_ttf<F>(
    engine: &FontEngine,
    kind: FaceKind,
    start_x: i32,
    start_y: i32,
    text: &str,
    mut draw_px: F,
) where
    F: FnMut(i32, i32, i32, i32, f32),
{
    let cell_h = engine.cell_h;
    let mut x = start_x;
    let mut y = start_y;
    for ch in text.chars() {
        if ch == '\r' {
            continue;
        }
        if ch == '\n' {
            x = start_x;
            y += cell_h;
            continue;
        }
        let cx = x;
        let cy = y;
        engine.draw_glyph(kind, ch, cx, cy, |gx, gy, w, h, cov| {
            draw_px(gx, gy, w, h, cov);
        });
        x += engine.advance_of(kind, ch);
    }
}

/// Same as [`draw_text_ttf`], but every pixel is limited to `clip`. Pixels
/// entirely outside the rect are dropped and pixels straddling the edge are
/// drawn only up to the edge, so centered text inside a button can never
/// bleed past the border, and text scrolled to a viewport edge is sliced
/// exactly there.
#[allow(clippy::too_many_arguments)]
pub fn draw_text_ttf_clipped<F>(
    engine: &FontEngine,
    kind: FaceKind,
    start_x: i32,
    start_y: i32,
    text: &str,
    clip: Rect,
    mut draw_px: F,
) where
    F: FnMut(i32, i32, i32, i32, f32),
{
    draw_text_ttf(engine, kind, start_x, start_y, text, |px, py, w, h, cov| {
        if let Some(r) = intersect(Rect { x: px, y: py, w, h }, clip) {
            draw_px(r.x, r.y, r.w, r.h, cov);
        }
    });
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

/// Mutable state threaded through layout: runtime scroll offsets (read), the
/// regions measured this pass (written), and the counter that mints stable
/// keys for `scroll` containers without an `id`.
pub struct LayoutCtx<'a> {
    pub scroll: &'a mut HashMap<String, i32>,
    pub regions: &'a mut Vec<ScrollRegion>,
    pub counter: &'a mut usize,
}

#[inline]
fn clips_owned(clips: &[ClipRegion]) -> Option<Vec<ClipRegion>> {
    if clips.is_empty() {
        None
    } else {
        Some(clips.to_vec())
    }
}

/// Lay `elem` out ignoring scroll containers' offsets (regions are discarded).
///
/// Kept as the simple entry point for callers that only need box geometry;
/// [`NativeApp::layout_root`] is the full pipeline used by the renderer.
pub fn layout_element(
    elem: &UiElement,
    x: i32,
    y: i32,
    max_w: i32,
    boxes: &mut Vec<LayoutBox>,
) -> (i32, i32) {
    let mut scroll = HashMap::new();
    let mut regions = Vec::new();
    let mut counter = 0usize;
    let mut ctx = LayoutCtx {
        scroll: &mut scroll,
        regions: &mut regions,
        counter: &mut counter,
    };
    layout_element_ctx(elem, x, y, max_w, boxes, &mut ctx, (0, 0), &[])
}

/// Recursive worker: `pan` is the accumulated scroll offset of enclosing scroll
/// containers (applied when drawing/hit-testing), `clips` are the viewports the
/// resulting boxes must be clipped to.
#[allow(clippy::only_used_in_recursion, clippy::too_many_arguments)]
pub fn layout_element_ctx(
    elem: &UiElement,
    x: i32,
    y: i32,
    max_w: i32,
    boxes: &mut Vec<LayoutBox>,
    ctx: &mut LayoutCtx,
    pan: (i32, i32),
    clips: &[ClipRegion],
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
                let (cw, ch) = layout_element_ctx(
                    child,
                    x + padding,
                    child_y,
                    max_w - 2 * padding,
                    boxes,
                    ctx,
                    pan,
                    clips,
                );
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
                let (cw, ch) =
                    layout_element_ctx(child, child_x, y + padding, max_w, boxes, ctx, pan, clips);
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
        "scroll" => {
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
            // The viewport is a *fixed* box: content taller than it scrolls
            // inside instead of growing the container.
            let viewport_h = elem
                .props
                .get("height")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(240)
                .max(1);
            let viewport_w = elem
                .props
                .get("width")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(max_w)
                .clamp(1, max_w.max(1));

            let viewport = Rect {
                x,
                y,
                w: viewport_w,
                h: viewport_h,
            };

            // Stable key: an explicit `id` survives layout order changes,
            // otherwise the layout-order index keeps the map key aligned with
            // the same container across renders.
            let key = match &elem.id {
                Some(id) => format!("scroll:{id}"),
                None => format!("scroll:#{}", *ctx.counter),
            };
            *ctx.counter += 1;

            // Offset comes from state clamped at the end of the previous
            // layout pass; `layout_root` re-runs layout if clamping moves it,
            // so the boxes drawn below are never based on a stale offset.
            let offset = ctx.scroll.get(&key).copied().unwrap_or(0);

            // The viewport itself only moves with its ancestors, but its
            // content also moves by this container's own offset.
            let mut child_clips = clips.to_vec();
            child_clips.push(ClipRegion {
                rect: viewport,
                pan,
            });
            let child_pan = (pan.0, pan.1 + offset);

            let mut child_boxes = Vec::new();
            let mut child_y = y + padding;
            let mut max_child_w = 0;
            for child in &elem.children {
                let (cw, ch) = layout_element_ctx(
                    child,
                    x + padding,
                    child_y,
                    viewport_w - 2 * padding,
                    &mut child_boxes,
                    ctx,
                    child_pan,
                    &child_clips,
                );
                max_child_w = max_child_w.max(cw);
                child_y += ch + gap;
            }
            boxes.append(&mut child_boxes);

            let content_h = if elem.children.is_empty() {
                2 * padding
            } else {
                child_y - gap + padding - y
            };

            ctx.regions.push(ScrollRegion {
                key,
                rect: viewport,
                content_h,
                pan,
            });

            // Parents stack siblings below the *viewport*, not the content.
            (viewport_w.max(max_child_w), viewport_h)
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
                    .any(|l| (l.chars().count() as i32) * CELL_W > target_w)
            };

            let content = if should_wrap {
                wrap_text(&raw_content, target_w, CELL_W)
            } else {
                raw_content
            };

            let lines = content.split('\n').count().max(1) as i32;
            let max_line_len = content
                .split('\n')
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0) as i32;
            let w = max_line_len * CELL_W;
            let h = lines * CELL_H;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "text".into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: None,
                on_change: None,
                on_key: elem.on_key.clone(),
                pan,
                clips: clips_owned(clips),
                pad: 0,
            });
            (w, h)
        }
        "bubble" => {
            // A chat bubble is a `text` box with a painted background behind
            // it: the message wraps to a max width, gets padded, and is
            // aligned left/right inside whatever width the parent offers.
            let raw_content = elem.text.clone().unwrap_or_else(|| elem.get_text());

            // Role drives both the palette and the default side.
            let role = elem
                .props
                .get("role")
                .map(|s| s.as_str())
                .unwrap_or("assistant");
            let is_user = role == "user";

            let pad: i32 = elem
                .props
                .get("padding")
                .and_then(|s| s.parse().ok())
                .unwrap_or(10);

            // Never wider than the parent offers, and never so narrow that the
            // padding alone would swallow the box.
            let max_width: i32 = elem
                .props
                .get("max_width")
                .and_then(|s| s.parse().ok())
                .unwrap_or(340)
                .min(max_w.saturating_sub(2 * pad))
                .max(2 * pad + CELL_W);

            let content = wrap_text(&raw_content, max_width - 2 * pad, CELL_W);

            let lines = content.split('\n').count().max(1) as i32;
            let max_line_len = content
                .split('\n')
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0) as i32;
            let text_w = max_line_len * CELL_W;
            let w = (text_w + 2 * pad).min(max_width);
            let h = lines * CELL_H + 2 * pad;

            // Left/right alignment inside the parent's available width.
            let align = elem
                .props
                .get("align")
                .map(|s| s.as_str())
                .unwrap_or(if is_user { "right" } else { "left" });
            let bx = match align {
                "right" => x + (max_w - w).max(0),
                "center" => x + ((max_w - w) / 2).max(0),
                _ => x,
            };

            boxes.push(LayoutBox {
                rect: Rect { x: bx, y, w, h },
                tag: if is_user {
                    "bubble:user"
                } else {
                    "bubble:assistant"
                }
                .into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: None,
                on_change: None,
                on_key: elem.on_key.clone(),
                pan,
                clips: clips_owned(clips),
                pad,
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
            let text_w = (content.chars().count() as i32) * CELL_W;
            let w = (text_w + 32).max(80);
            let h = (CELL_H + 16).max(36);
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "button".into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: elem.on_click.clone().or_else(|| elem.id.clone()),
                on_change: None,
                on_key: elem.on_key.clone(),
                pan,
                clips: clips_owned(clips),
                pad: 0,
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
            let w = 220.max(max_line_len * CELL_W + 24);
            let h = (CELL_H + 16).max(lines * CELL_H + 16);
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "input".into(),
                id: elem.id.clone(),
                text: Some(content),
                on_click: None,
                on_change: elem.on_change.clone(),
                on_key: elem.on_key.clone(),
                pan,
                clips: clips_owned(clips),
                pad: 0,
            });
            (w, h)
        }
        _ => {
            // Generic container or unknown tag
            let mut child_y = y;
            let mut max_w_seen = 0;
            for child in &elem.children {
                let (cw, ch) = layout_element_ctx(child, x, child_y, max_w, boxes, ctx, pan, clips);
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
            // Pan-aware screen rect; a button scrolled outside its scroll
            // viewport is not hoverable even where its rect still overlaps.
            let screen_rect = lbox.screen_rect(scroll_x, scroll_y);
            let clip_ok = lbox
                .screen_clip(scroll_x, scroll_y)
                .is_none_or(|c| c.contains(mx, my));
            if screen_rect.contains(mx, my) && clip_ok {
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
    pub theme: Theme,
    pub fonts: FontEngine,
    /// Scroll offset per `scroll` container (keyed by `scroll:<id>` or
    /// `scroll:#<layout order index>`).
    pub scroll_offsets: HashMap<String, i32>,
    /// Viewports measured by the most recent layout pass.
    pub scroll_regions: Vec<ScrollRegion>,
    /// Mints stable keys for `scroll` containers without an `id`; reset on
    /// every layout pass so keys track layout order.
    scroll_counter: usize,
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
            theme: Theme::DARK,
            fonts: FontEngine::new(BASE_PX),
            scroll_offsets: HashMap::new(),
            scroll_regions: Vec::new(),
            scroll_counter: 0,
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

    /// Innermost measured scroll viewport containing `pos` (screen space).
    ///
    /// Later regions are laid out later, so they sit on top; iterating in
    /// reverse yields the innermost container when views overlap.
    pub fn region_at(&self, pos: (i32, i32)) -> Option<&ScrollRegion> {
        let (px, py) = pos;
        self.scroll_regions.iter().rev().find(|r| {
            let rect = Rect {
                x: r.rect.x - self.scroll_x - r.pan.0,
                y: r.rect.y - self.scroll_y - r.pan.1,
                w: r.rect.w,
                h: r.rect.h,
            };
            rect.contains(px, py)
        })
    }

    /// Scroll one region by `dy` pixels. Returns `false` when the key is
    /// unknown (nothing to scroll).
    pub fn scroll_region_by(&mut self, key: &str, dy: i32) -> bool {
        let Some(region) = self.scroll_regions.iter().find(|r| r.key == key) else {
            return false;
        };
        let max = region.max_offset();
        let cur = self.scroll_offsets.get(key).copied().unwrap_or(0);
        let next = cur.saturating_add(dy).clamp(0, max);
        if next == cur {
            return true; // already at the end; consumed by this region
        }
        self.scroll_offsets.insert(key.to_string(), next);
        self.render();
        true
    }

    /// Clamp every measured region's offset to its current content height.
    /// Returns `true` when any offset moved (the caller should re-run layout
    /// so drawn boxes never use a stale offset).
    fn clamp_scroll_offsets(&mut self) -> bool {
        let mut changed = false;
        for r in &self.scroll_regions {
            let max = r.max_offset();
            let cur = self.scroll_offsets.get(&r.key).copied().unwrap_or(0);
            let next = cur.clamp(0, max);
            if next != cur {
                self.scroll_offsets.insert(r.key.clone(), next);
                changed = true;
            }
        }
        changed
    }

    /// Full layout pipeline: lay `root` out, then re-lay it out whenever
    /// clamping moved a scroll offset (bounded — clamping is monotone, so it
    /// converges after at most two passes).
    pub fn layout_root(&mut self, root: &UiElement) {
        for _ in 0..4 {
            self.scroll_regions.clear();
            self.scroll_counter = 0;
            self.layout_boxes.clear();

            let NativeApp {
                layout_boxes,
                scroll_offsets,
                scroll_regions,
                scroll_counter,
                content_width,
                content_height,
                ..
            } = self;
            let mut ctx = LayoutCtx {
                scroll: scroll_offsets,
                regions: scroll_regions,
                counter: scroll_counter,
            };
            let (total_w, total_h) = layout_element_ctx(
                root,
                10,
                10,
                self.width as i32 - 20,
                layout_boxes,
                &mut ctx,
                (0, 0),
                &[],
            );
            *content_width = total_w;
            *content_height = total_h;

            if !self.clamp_scroll_offsets() {
                break;
            }
        }
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

        // Fetch latest view tree and layout (re-runs if clamping moved a
        // scroll offset, so boxes below never use a stale pan). Done before
        // borrowing the surface, which is borrowed until present().
        if let Some(root) = self.dispatcher.root() {
            self.layout_root(&root);
        } else {
            self.layout_boxes.clear();
            self.scroll_regions.clear();
            self.content_width = 0;
            self.content_height = 0;
        }
        let total_w = self.content_width;
        let total_h = self.content_height;

        // Clamp window scroll offsets
        let max_scroll_y = (total_h - self.height as i32 + 20).max(0);
        let max_scroll_x = (total_w - self.width as i32 + 20).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max_scroll_y);
        self.scroll_x = self.scroll_x.clamp(0, max_scroll_x);

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

        let theme = self.theme;
        let cell_h = self.fonts.cell_h;

        // Background from theme
        draw_rect(
            &mut buffer,
            stride,
            h_usize,
            0,
            0,
            self.width as i32,
            self.height as i32,
            theme.bg,
        );

        let (mx, my) = self.cursor_pos;
        self.hovered_id = find_hovered_button_in_boxes(
            &self.layout_boxes,
            self.scroll_x,
            self.scroll_y,
            self.cursor_pos,
        );

        // Render each layout box with window scroll, enclosing scroll
        // containers (pan) & their viewport clipping
        for lbox in &self.layout_boxes {
            let rx = lbox.rect.x - self.scroll_x - lbox.pan.0;
            let ry = lbox.rect.y - self.scroll_y - lbox.pan.1;

            // Viewport clipping: skip completely out-of-bounds elements
            if !lbox.visible_in(
                self.scroll_x,
                self.scroll_y,
                self.width as i32,
                self.height as i32,
            ) {
                continue;
            }
            let clip = lbox.screen_clip(self.scroll_x, self.scroll_y);

            match lbox.tag.as_str() {
                "text" => {
                    if let Some(ref text) = lbox.text {
                        let color = theme.text;
                        let fonts = &self.fonts;
                        match clip {
                            Some(c) => draw_text_ttf_clipped(
                                fonts,
                                FaceKind::Ui,
                                rx,
                                ry + 2,
                                text,
                                c,
                                |px, py, w, h, cov| {
                                    draw_rect_alpha(
                                        &mut buffer,
                                        stride,
                                        h_usize,
                                        px,
                                        py,
                                        w,
                                        h,
                                        color,
                                        cov,
                                    );
                                },
                            ),
                            None => draw_text_ttf(
                                fonts,
                                FaceKind::Ui,
                                rx,
                                ry + 2,
                                text,
                                |px, py, w, h, cov| {
                                    draw_rect_alpha(
                                        &mut buffer,
                                        stride,
                                        h_usize,
                                        px,
                                        py,
                                        w,
                                        h,
                                        color,
                                        cov,
                                    );
                                },
                            ),
                        }
                    }
                }
                t if t.starts_with("bubble:") => {
                    // Rounded background sized to the padded box, with the
                    // wrapped message drawn inside it. Both are clipped to the
                    // enclosing viewport, so a bubble scrolled out of its
                    // region leaks no pixels.
                    let is_user = t == "bubble:user";
                    let (bg, fg) = if is_user {
                        (theme.bubble_user_bg, theme.bubble_user_text)
                    } else {
                        (theme.bubble_assistant_bg, theme.bubble_assistant_text)
                    };
                    let radius = 10;
                    match clip {
                        Some(c) => draw_rounded_rect_box_clipped(
                            &mut buffer,
                            stride,
                            h_usize,
                            Some(c),
                            rx,
                            ry,
                            lbox.rect.w,
                            lbox.rect.h,
                            radius,
                            bg,
                            theme.border,
                        ),
                        None => draw_rounded_rect_box(
                            &mut buffer,
                            stride,
                            h_usize,
                            rx,
                            ry,
                            lbox.rect.w,
                            lbox.rect.h,
                            radius,
                            bg,
                            theme.border,
                        ),
                    }
                    if let Some(ref text) = lbox.text {
                        let fonts = &self.fonts;
                        let tx = rx + lbox.pad;
                        let ty = ry + lbox.pad + 2;
                        // Keep the glyphs on the same clip as the background.
                        let text_clip = match clip {
                            Some(c) => {
                                let inner = Rect {
                                    x: rx + lbox.pad,
                                    y: ry + lbox.pad,
                                    w: (lbox.rect.w - 2 * lbox.pad).max(0),
                                    h: (lbox.rect.h - 2 * lbox.pad).max(0),
                                };
                                intersect(inner, c)
                            }
                            None => Some(Rect {
                                x: tx,
                                y: ty,
                                w: (lbox.rect.w - 2 * lbox.pad).max(0),
                                h: (lbox.rect.h - 2 * lbox.pad).max(0),
                            }),
                        };
                        if let Some(tc) = text_clip {
                            draw_text_ttf_clipped(
                                fonts,
                                FaceKind::Ui,
                                tx,
                                ty,
                                text,
                                tc,
                                |px, py, w, h, cov| {
                                    draw_rect_alpha(
                                        &mut buffer,
                                        stride,
                                        h_usize,
                                        px,
                                        py,
                                        w,
                                        h,
                                        fg,
                                        cov,
                                    );
                                },
                            );
                        }
                    }
                }
                "button" => {
                    let screen_rect = Rect {
                        x: rx,
                        y: ry,
                        w: lbox.rect.w,
                        h: lbox.rect.h,
                    };
                    // A control scrolled out of its viewport is not hoverable
                    // even if the cursor sits on its (invisible) rectangle.
                    let hovered =
                        screen_rect.contains(mx, my) && clip.is_none_or(|c| c.contains(mx, my));
                    let bg_color = if hovered {
                        theme.button_bg_hover
                    } else {
                        theme.button_bg
                    };
                    draw_rounded_rect_box_clipped(
                        &mut buffer,
                        stride,
                        h_usize,
                        clip,
                        rx,
                        ry,
                        lbox.rect.w,
                        lbox.rect.h,
                        CORNER_RADIUS,
                        bg_color,
                        theme.button_border,
                    );
                    if let Some(ref text) = lbox.text {
                        let text_w = self.fonts.measure_width(FaceKind::Ui, text);
                        let tx = rx + ((lbox.rect.w - text_w) / 2).max(8);
                        let ty = ry + ((lbox.rect.h - cell_h) / 2).max(4);
                        let color = theme.button_text;
                        let fonts = &self.fonts;
                        // Clip to the button interior (intersected with any
                        // enclosing scroll viewport) so wide glyphs can never
                        // bleed past the rounded border, while keeping the
                        // centered proportional layout.
                        let interior = Rect {
                            x: rx,
                            y: ry,
                            w: lbox.rect.w,
                            h: lbox.rect.h,
                        };
                        let text_clip = match clip {
                            None => Some(interior),
                            Some(c) => intersect(interior, c),
                        };
                        if let Some(tc) = text_clip {
                            draw_text_ttf_clipped(
                                fonts,
                                FaceKind::Ui,
                                tx,
                                ty,
                                text,
                                tc,
                                |px, py, w, h, cov| {
                                    draw_rect_alpha(
                                        &mut buffer,
                                        stride,
                                        h_usize,
                                        px,
                                        py,
                                        w,
                                        h,
                                        color,
                                        cov,
                                    );
                                },
                            );
                        }
                    }
                }
                "input" => {
                    let is_focused =
                        self.focused_id.as_deref() == lbox.id.as_deref() && lbox.id.is_some();
                    let border_color = if is_focused {
                        theme.input_border_focused
                    } else {
                        theme.input_border
                    };

                    draw_rounded_rect_box_clipped(
                        &mut buffer,
                        stride,
                        h_usize,
                        clip,
                        rx,
                        ry,
                        lbox.rect.w,
                        lbox.rect.h,
                        CORNER_RADIUS,
                        theme.input_bg,
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
                                            // Measure on the same proportional
                                            // metric the text is drawn with, so
                                            // the highlight hugs the glyphs.
                                            let line_text = st
                                                .text
                                                .split('\n')
                                                .nth(line)
                                                .unwrap_or("")
                                                .trim_end_matches('\r');
                                            let start_off = self.fonts.prefix_width(
                                                FaceKind::Ui,
                                                line_text,
                                                col_start,
                                            );
                                            let end_off = self.fonts.prefix_width(
                                                FaceKind::Ui,
                                                line_text,
                                                col_end,
                                            );
                                            let sel_x = rx + 8 + start_off;
                                            let sel_w = (end_off - start_off).max(0);
                                            let sel_y = ry + 6 + (line as i32) * cell_h;
                                            let sel_rect = Rect {
                                                x: sel_x,
                                                y: sel_y,
                                                w: sel_w,
                                                h: cell_h,
                                            };
                                            let sel_draw = match clip {
                                                None => Some(sel_rect),
                                                Some(c) => intersect(sel_rect, c),
                                            };
                                            if let Some(sr) = sel_draw {
                                                draw_rect(
                                                    &mut buffer,
                                                    stride,
                                                    h_usize,
                                                    sr.x,
                                                    sr.y,
                                                    sr.w,
                                                    sr.h,
                                                    theme.selection_bg,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Draw text, clipped to the field interior (and any
                    // enclosing scroll viewport) so glyphs can never bleed
                    // over the rounded border.
                    let input_text_color = theme.text;
                    let fonts = &self.fonts;
                    let field = Rect {
                        x: rx + 1,
                        y: ry + 1,
                        w: (lbox.rect.w - 2).max(0),
                        h: (lbox.rect.h - 2).max(0),
                    };
                    let text_clip = match clip {
                        None => Some(field),
                        Some(c) => intersect(field, c),
                    };
                    if let Some(tc) = text_clip {
                        draw_text_ttf_clipped(
                            fonts,
                            FaceKind::Ui,
                            rx + 8,
                            ry + 6,
                            &display_text,
                            tc,
                            |px, py, w, h, cov| {
                                draw_rect_alpha(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    px,
                                    py,
                                    w,
                                    h,
                                    input_text_color,
                                    cov,
                                );
                            },
                        );
                    }

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
                        let line_text = display_text
                            .split('\n')
                            .nth(cur_line)
                            .unwrap_or("")
                            .trim_end_matches('\r');
                        // Proportional, not cell-grid: the caret must sit on
                        // the drawn glyph edge, otherwise it drifts right of
                        // short glyphs and looks like a phantom space.
                        let cur_x =
                            rx + 8 + self.fonts.prefix_width(FaceKind::Ui, line_text, cur_col);
                        let cur_y = ry + 6 + (cur_line as i32) * cell_h;
                        let caret = Rect {
                            x: cur_x,
                            y: cur_y,
                            w: 2,
                            h: cell_h,
                        };
                        let caret_draw = match clip {
                            None => Some(caret),
                            Some(c) => intersect(caret, c),
                        };
                        if let Some(cd) = caret_draw {
                            draw_rect(
                                &mut buffer,
                                stride,
                                h_usize,
                                cd.x,
                                cd.y,
                                cd.w,
                                cd.h,
                                theme.cursor,
                            );
                        }
                    }
                }
                _ => {}
            }
        }

        // Scroll thumb for each scrollable region, so nested content shows
        // where it can be scrolled (the global track below covers the window)
        for r in &self.scroll_regions {
            let max = r.max_offset();
            if max <= 0 {
                continue;
            }
            let vx = r.rect.x - self.scroll_x - r.pan.0;
            let vy = r.rect.y - self.scroll_y - r.pan.1;
            let (vw, vh) = (r.rect.w, r.rect.h);
            if vy + vh < 0 || vy >= self.height as i32 || vw <= 0 {
                continue;
            }
            let track_h = (vh - 8).max(1);
            let thumb_h = ((track_h as f64 * vh as f64) / r.content_h.max(1) as f64)
                .max(20.0)
                .min(track_h as f64) as i32;
            let off = self.scroll_offsets.get(&r.key).copied().unwrap_or(0);
            let thumb_y =
                vy + 4 + ((off as f64 / max as f64) * (track_h - thumb_h).max(0) as f64) as i32;
            draw_rect(
                &mut buffer,
                stride,
                h_usize,
                vx + vw - 5,
                thumb_y,
                4,
                thumb_h,
                theme.scrollbar_thumb,
            );
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
                theme.scrollbar_track,
            );
            draw_rect(
                &mut buffer,
                stride,
                h_usize,
                self.width as i32 - 7,
                thumb_y,
                6,
                thumb_h,
                theme.scrollbar_thumb,
            );
        }

        self.render_count += 1;
        let _ = buffer.present();
    }

    pub fn handle_click(&mut self) {
        let (mx, my) = self.cursor_pos;

        let mut clicked_button = None;
        let mut clicked_input = None;

        for lbox in &self.layout_boxes {
            // Screen-space test accounts for the window scroll, the enclosing
            // scroll containers (pan) and their viewports (clip), so a control
            // scrolled out of view can never be clicked through.
            let sr = lbox.screen_rect(self.scroll_x, self.scroll_y);
            if !sr.contains(mx, my) {
                continue;
            }
            if !lbox.visible_in(
                self.scroll_x,
                self.scroll_y,
                self.width as i32,
                self.height as i32,
            ) {
                continue;
            }
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

        if let Some(button_id) = clicked_button {
            self.dispatcher.dispatch_click(&button_id);
            self.render();
        } else if let Some(lbox) = clicked_input {
            // Map the click back into content space for caret placement.
            let cx = mx + self.scroll_x + lbox.pan.0;
            let cy = my + self.scroll_y + lbox.pan.1;
            if let Some(ref id) = lbox.id {
                self.focused_id = Some(id.clone());
                let rel_x = cx - (lbox.rect.x + 8);
                let rel_y = cy - (lbox.rect.y + 6);
                let cell_h = self.fonts.cell_h.max(1);
                let line_idx = (rel_y.max(0) / cell_h) as usize;

                // Row height stays on the line grid (each drawn line advances
                // by exactly cell_h), but the column is resolved with the same
                // proportional advances the glyphs are drawn with.
                let (line_start, target_col_max, char_len, line_text) = {
                    let st = self.ensure_input_state(id, lbox.text.as_deref().unwrap_or(""));
                    let target_line = line_idx.min(st.lines_count().saturating_sub(1));
                    let line_start = st.char_idx_for_line_col(target_line, 0);
                    let target_col_max = st.line_len(target_line);
                    let char_len = st.char_len();
                    let line_text = st
                        .text
                        .split('\n')
                        .nth(target_line)
                        .unwrap_or("")
                        .trim_end_matches('\r')
                        .to_string();
                    (line_start, target_col_max, char_len, line_text)
                };

                let col_idx = self.fonts.col_at_offset(FaceKind::Ui, &line_text, rel_x);
                let target_col = col_idx.min(target_col_max);
                let char_idx = (line_start + target_col).min(char_len);

                if let Some(st) = self.text_inputs.get_mut(id) {
                    st.cursor = char_idx;
                    st.selection_anchor = None;
                }
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

        // Scroll keys target the scroll viewport under the cursor; without
        // one, they fall back to scrolling the whole window.
        let region_target: Option<(String, i32)> = self
            .region_at(self.cursor_pos)
            .map(|r| (r.key.clone(), (r.rect.h - 40).max(40)));
        let window_page = (self.height as i32 - 40).max(40);

        match logical_key {
            Key::Named(NamedKey::PageUp) => match &region_target {
                Some((key, page)) => {
                    self.scroll_region_by(key, -page);
                }
                None => self.scroll_by(0, -window_page),
            },
            Key::Named(NamedKey::PageDown) => match &region_target {
                Some((key, page)) => {
                    self.scroll_region_by(key, *page);
                }
                None => self.scroll_by(0, window_page),
            },
            Key::Named(NamedKey::Home) => match &region_target {
                Some((key, _)) => {
                    self.scroll_region_by(key, i32::MIN);
                }
                None => {
                    self.scroll_y = 0;
                    self.render();
                }
            },
            Key::Named(NamedKey::End) => match &region_target {
                Some((key, _)) => {
                    self.scroll_region_by(key, i32::MAX);
                }
                None => {
                    let max_y = (self.content_height - self.height as i32 + 20).max(0);
                    self.scroll_y = max_y;
                    self.render();
                }
            },
            Key::Named(NamedKey::ArrowUp) => match &region_target {
                Some((key, _)) => {
                    self.scroll_region_by(key, -20);
                }
                None => self.scroll_by(0, -20),
            },
            Key::Named(NamedKey::ArrowDown) => match &region_target {
                Some((key, _)) => {
                    self.scroll_region_by(key, 20);
                }
                None => self.scroll_by(0, 20),
            },
            Key::Named(NamedKey::Enter) => key_name = "Enter".to_string(),
            Key::Named(NamedKey::Escape) => key_name = "Escape".to_string(),
            Key::Character(s) => key_name = s.to_string(),
            _ => {}
        }

        let scroll_keys = matches!(
            logical_key,
            Key::Named(
                NamedKey::PageUp
                    | NamedKey::PageDown
                    | NamedKey::Home
                    | NamedKey::End
                    | NamedKey::ArrowUp
                    | NamedKey::ArrowDown
            )
        );
        if scroll_keys {
            handled_scroll = true;
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
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(dx, dy) => {
                        ((-dx * 30.0) as i32, (-dy * 30.0) as i32)
                    }
                    MouseScrollDelta::PixelDelta(pos) => (-pos.x as i32, -pos.y as i32),
                };
                // A scroll viewport under the cursor swallows the wheel so the
                // page behind it stays put until the cursor leaves the region.
                let region_key = self.region_at(self.cursor_pos).map(|r| r.key.clone());
                match region_key {
                    Some(key) => {
                        self.scroll_region_by(&key, dy);
                    }
                    None => self.scroll_by(dx, dy),
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
        let engine = FontEngine::new(BASE_PX);
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
            pan: (0, 0),
            clips: None,
            pad: 0,
        }];

        // Simulate click on line 1, col 2:
        // x is the proportional offset of 2 glyphs into "line1" (the same
        // metric the glyphs are drawn with), NOT cell_grid 2*CELL_W.
        // y: lbox.rect.y + 6 + 1 * CELL_H = 20 + 6 + 20 = 46
        let x_line1_col2 = 10 + 8 + engine.prefix_width(FaceKind::Ui, "line1", 2);
        app.cursor_pos = (x_line1_col2, 20 + 6 + CELL_H);
        app.handle_click();

        assert_eq!(app.focused_id.as_deref(), Some("multiline_input"));
        let st = app.text_inputs.get("multiline_input").unwrap();
        // line 0 is "line0\n" (len 6), line 1 col 2 is char index 6 + 2 = 8 ('n')
        assert_eq!(
            st.cursor, 8,
            "click on line 1 must map to char index on line 1"
        );

        // Simulate click on line 2, col 3:
        // x: proportional offset of 3 glyphs into "line2"
        // y: 20 + 6 + 2 * CELL_H = 66
        let x_line2_col3 = 10 + 8 + engine.prefix_width(FaceKind::Ui, "line2", 3);
        app.cursor_pos = (x_line2_col3, 20 + 6 + 2 * CELL_H);
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
            pan: (0, 0),
            clips: None,
            pad: 0,
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
        let wrapped = wrap_text(long_message, 200, CELL_W);
        let lines: Vec<&str> = wrapped.split('\n').collect();
        assert!(lines.len() > 1, "long text must wrap into multiple lines");
        for line in &lines {
            let line_px = (line.chars().count() as i32) * CELL_W;
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
            h > CELL_H,
            "wrapped height must exceed single-line height of {CELL_H}px"
        );
        assert_eq!(h, (lines.len() as i32) * CELL_H);
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
            pan: (0, 0),
            clips: None,
            pad: 0,
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

    /// A rounded rectangle must actually cut its corners: the outermost corner
    /// pixel stays background while the centre is filled.
    #[test]
    fn test_rounded_rect_cuts_corners() {
        const BG: u32 = 0xFF00_0000;
        const FILL: u32 = 0xFFFF_FFFF;
        let (stride, height) = (40usize, 40usize);
        let mut buf = vec![BG; stride * height];
        let at = |b: &[u32], x: i32, y: i32| b[(y as usize) * stride + x as usize];

        draw_rounded_rect_alpha(&mut buf, stride, height, 10, 10, 20, 20, 6, FILL, 1.0);

        assert_eq!(at(&buf, 10, 10), BG, "top-left corner must be cut");
        assert_eq!(at(&buf, 29, 10), BG, "top-right corner must be cut");
        assert_eq!(at(&buf, 10, 29), BG, "bottom-left corner must be cut");
        assert_eq!(at(&buf, 29, 29), BG, "bottom-right corner must be cut");

        assert_eq!(at(&buf, 20, 20), FILL, "centre must be filled");
        assert_eq!(
            at(&buf, 10, 20),
            FILL,
            "left edge at mid-height must be filled"
        );
        assert_eq!(
            at(&buf, 20, 10),
            FILL,
            "top edge at mid-width must be filled"
        );

        // Nothing outside the box may be touched.
        assert_eq!(at(&buf, 9, 20), BG, "outside left must stay background");
        assert_eq!(at(&buf, 20, 9), BG, "outside top must stay background");
    }

    /// The arc must be anti-aliased (a graded pixel exists), not stair-stepped.
    #[test]
    fn test_rounded_rect_edges_are_anti_aliased() {
        const BG: u32 = 0xFF00_0000;
        const FILL: u32 = 0xFFFF_FFFF;
        let (stride, height) = (40usize, 40usize);
        let mut buf = vec![BG; stride * height];
        draw_rounded_rect_alpha(&mut buf, stride, height, 10, 10, 20, 20, 6, FILL, 1.0);

        let partial = buf.iter().filter(|&&p| p != BG && p != FILL).count();
        assert!(
            partial > 0,
            "expected at least one anti-aliased pixel along the arcs"
        );
        // ...but the shape must stay mostly solid: fewer partial than solid px.
        let solid = buf.iter().filter(|&&p| p == FILL).count();
        assert!(solid > partial, "arc pixels should not dominate the shape");
    }

    /// `draw_rounded_rect_box` must produce a 1px border ring with the fill
    /// inside it, and leave the cut corner as background.
    #[test]
    fn test_rounded_rect_box_border_and_fill() {
        const BG: u32 = 0xFF00_0000;
        const FILL: u32 = 0xFF00_00FF;
        const BORDER: u32 = 0xFFFF_FFFF;
        let (stride, height) = (40usize, 40usize);
        let mut buf = vec![BG; stride * height];
        let at = |b: &[u32], x: i32, y: i32| b[(y as usize) * stride + x as usize];

        draw_rounded_rect_box(&mut buf, stride, height, 10, 10, 20, 20, 6, FILL, BORDER);

        assert_eq!(at(&buf, 10, 10), BG, "corner cut must remain background");
        assert_eq!(at(&buf, 10, 20), BORDER, "left edge must be the border");
        assert_eq!(at(&buf, 20, 10), BORDER, "top edge must be the border");
        assert_eq!(at(&buf, 20, 20), FILL, "interior must be the fill");
        assert_eq!(at(&buf, 11, 20), FILL, "fill starts right after the border");
        assert_eq!(at(&buf, 20, 11), FILL, "fill starts right after the border");
    }

    /// Radius 0 must degrade to the plain rectangle helpers, byte for byte.
    #[test]
    fn test_rounded_rect_radius_zero_matches_rect() {
        let (stride, height) = (32usize, 32usize);
        let mut sharp = vec![0xFF00_0000u32; stride * height];
        let mut rounded = vec![0xFF00_0000u32; stride * height];
        let (x, y, w, h, color) = (5, 5, 20, 14, 0xFFAA_BBCC);

        draw_rect(&mut sharp, stride, height, x, y, w, h, color);
        draw_rect_outline(&mut sharp, stride, height, x, y, w, h, 0xFF11_2233);

        draw_rounded_rect_box(
            &mut rounded,
            stride,
            height,
            x,
            y,
            w,
            h,
            0,
            color,
            0xFF11_2233,
        );

        assert_eq!(sharp, rounded, "radius 0 must equal the square helpers");
    }

    /// Partial off-screen boxes must clip safely instead of panicking or
    /// scribbling outside the buffer.
    #[test]
    fn test_rounded_rect_clips_to_framebuffer() {
        const BG: u32 = 0xFF00_0000;
        let (stride, height) = (24usize, 24usize);
        let mut buf = vec![BG; stride * height];

        // Straddles the top-left corner, the right edge and the bottom edge.
        draw_rounded_rect_alpha(
            &mut buf,
            stride,
            height,
            -5,
            -5,
            30,
            40,
            6,
            0xFFFF_FFFF,
            1.0,
        );
        draw_rounded_rect_box(
            &mut buf,
            stride,
            height,
            20,
            20,
            20,
            20,
            6,
            0xFF00_00FF,
            0xFFFF_FFFF,
        );

        // Every pixel is addressable and the buffer length is unchanged.
        assert_eq!(buf.len(), stride * height);
        assert!(
            buf.iter().any(|&p| p != BG),
            "clipped drawing must still paint inside the buffer"
        );
    }

    // -------------------------------------------------------------------------
    // Nested scroll regions (roadmap 2.1)
    // -------------------------------------------------------------------------

    /// `column[ scroll(id="feed", height, padding=0, gap=6)[ msgs... ],
    ///  button("Send") ]` — the shape Studio's chat pane uses.
    fn chat_tree(msg_count: usize, viewport_h: i32) -> UiElement {
        let mut feed = UiElement::new("scroll");
        feed.id = Some("feed".into());
        feed.props.insert("height".into(), viewport_h.to_string());
        feed.props.insert("padding".into(), "0".into());
        feed.props.insert("gap".into(), "6".into());
        for i in 0..msg_count {
            let mut t = UiElement::new("text");
            t.text = Some(format!("message {i}"));
            feed.children.push(t);
        }

        let mut send = UiElement::new("button");
        send.id = Some("send".into());
        send.text = Some("Send".into());

        let mut root = UiElement::new("column");
        root.props.insert("padding".into(), "0".into());
        root.props.insert("gap".into(), "10".into());
        root.children.push(feed);
        root.children.push(send);
        root
    }

    /// A viewport keeps the height it declared, its content is measured, and
    /// siblings stack below the *viewport* — not below the scrolled content.
    #[test]
    fn test_scroll_viewport_is_fixed_and_content_is_measured() {
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

        let root = chat_tree(10, 100);
        app.layout_root(&root);

        assert_eq!(app.scroll_regions.len(), 1, "one scroll viewport measured");
        let region = &app.scroll_regions[0];
        assert_eq!(region.key, "scroll:feed", "explicit id becomes the key");
        assert_eq!(region.rect.h, 100, "viewport keeps its declared height");
        assert_eq!(
            region.content_h,
            10 * CELL_H + 9 * 6,
            "content height is measured, not the viewport height"
        );
        assert_eq!(region.max_offset(), 10 * CELL_H + 9 * 6 - 100);

        let send = app
            .layout_boxes
            .iter()
            .find(|b| b.id.as_deref() == Some("send"))
            .expect("Send button laid out");
        assert_eq!(
            send.rect.y,
            region.rect.y + region.rect.h + 10,
            "sibling stacks below the viewport, not below the content"
        );

        let first = app
            .layout_boxes
            .iter()
            .find(|b| b.text.as_deref() == Some("message 0"))
            .expect("first message laid out");
        assert_eq!(first.pan, (0, 0), "no scrolling yet");
        let clips = first
            .clips
            .as_ref()
            .expect("content inside a scroll region must be clipped");
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].rect, region.rect, "clipped to the viewport");
    }

    /// A stored offset pans the content, and layout clamps an overshooting
    /// offset back into range before anything is drawn or clicked.
    #[test]
    fn test_scroll_offset_pans_content_and_is_clamped() {
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

        let root = chat_tree(10, 100);
        app.scroll_offsets.insert("scroll:feed".into(), 10_000);
        app.layout_root(&root);

        let max = app.scroll_regions[0].max_offset();
        assert_eq!(
            app.scroll_offsets["scroll:feed"], max,
            "an offset past the content is clamped by layout"
        );

        app.scroll_offsets.insert("scroll:feed".into(), 40);
        app.layout_root(&root);

        let first = app
            .layout_boxes
            .iter()
            .find(|b| b.text.as_deref() == Some("message 0"))
            .unwrap();
        assert_eq!(first.pan, (0, 40), "content pans by the region offset");
        assert_eq!(
            first.screen_rect(app.scroll_x, app.scroll_y).y,
            first.rect.y - 40
        );
        assert!(
            !first.visible_in(
                app.scroll_x,
                app.scroll_y,
                app.width as i32,
                app.height as i32
            ),
            "scrolled above the viewport is not visible"
        );

        // With pan 40 the viewport shows content y 50..150, i.e. messages 1..5.
        let mid = app
            .layout_boxes
            .iter()
            .find(|b| b.text.as_deref() == Some("message 3"))
            .unwrap();
        assert!(
            mid.visible_in(
                app.scroll_x,
                app.scroll_y,
                app.width as i32,
                app.height as i32
            ),
            "content scrolled into the viewport is visible"
        );
        let far = app
            .layout_boxes
            .iter()
            .find(|b| b.text.as_deref() == Some("message 9"))
            .unwrap();
        assert!(
            !far.visible_in(
                app.scroll_x,
                app.scroll_y,
                app.width as i32,
                app.height as i32
            ),
            "content still below the viewport stays clipped"
        );
    }

    /// Clicks follow the pan; content scrolled out of its viewport can never
    /// be clicked through to the widgets underneath.
    #[test]
    fn test_click_follows_scroll_and_never_clicks_through() {
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

        // scroll[ msgs..., button("Deep") ] — the button is at the bottom of
        // the content, so scrolling brings it into view.
        let mut feed = UiElement::new("scroll");
        feed.id = Some("feed".into());
        feed.props.insert("height".into(), "50".into());
        feed.props.insert("padding".into(), "0".into());
        feed.props.insert("gap".into(), "6".into());
        for i in 0..5 {
            let mut t = UiElement::new("text");
            t.text = Some(format!("m{i}"));
            feed.children.push(t);
        }
        let mut deep = UiElement::new("button");
        deep.id = Some("deep".into());
        deep.text = Some("Deep".into());
        feed.children.push(deep);

        let mut root = UiElement::new("column");
        root.props.insert("padding".into(), "0".into());
        root.children.push(feed);
        app.layout_root(&root);

        let button = app
            .layout_boxes
            .iter()
            .find(|b| b.id.as_deref() == Some("deep"))
            .unwrap()
            .clone();
        assert_eq!(
            app.scroll_regions[0].max_offset(),
            button.rect.y + button.rect.h - app.scroll_regions[0].rect.y - 50,
            "the region scrolls until the button bottom meets the viewport"
        );

        // Bring the button into view, then click it where it is *drawn*.
        app.scroll_offsets.insert("scroll:feed".into(), 100);
        app.layout_root(&root);
        let screen_y = 140 - 100;
        app.cursor_pos = (20, screen_y + 10);
        app.handle_click();

        // The same screen point, but with the button scrolled away, must be a
        // dead click even though its content-space rect still covers it.
        app.scroll_offsets.insert("scroll:feed".into(), 0);
        app.layout_root(&root);
        app.cursor_pos = (20, screen_y + 10);
        app.handle_click();

        // Clicking the visible button once more, to prove the middle click
        // above dispatched nothing (the count would be 3 otherwise).
        app.scroll_offsets.insert("scroll:feed".into(), 100);
        app.layout_root(&root);
        app.cursor_pos = (20, screen_y + 10);
        app.handle_click();

        drop(app);
        assert_eq!(
            val, 2,
            "a control scrolled out of its viewport must not receive clicks"
        );
    }

    /// Wheel/keyboard routing: the region under the cursor swallows the
    /// gesture, the window only scrolls when the cursor is outside any region.
    #[test]
    fn test_wheel_and_keys_route_to_region_under_cursor() {
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

        let root = chat_tree(10, 100);
        app.layout_root(&root);

        assert_eq!(
            app.region_at((20, 30)).map(|r| r.key.as_str()),
            Some("scroll:feed"),
            "cursor inside the viewport finds the region"
        );
        assert!(
            app.region_at((20, 200)).is_none(),
            "cursor below the viewport belongs to the window"
        );

        app.cursor_pos = (20, 30);
        let window_before = app.scroll_y;
        let region_key = "scroll:feed".to_string();
        app.scroll_region_by(&region_key, 60);
        assert_eq!(app.scroll_offsets[&region_key], 60);
        assert_eq!(
            app.scroll_y, window_before,
            "scrolling a region must not move the window"
        );

        // Keyboard: the same routing applies to PageDown / End / Home.
        let page = app.scroll_regions[0].rect.h - 40;
        app.send_key("PageDown");
        assert_eq!(app.scroll_offsets[&region_key], 60 + page);
        app.send_key("End");
        assert_eq!(
            app.scroll_offsets[&region_key],
            app.scroll_regions[0].max_offset(),
            "End scrolls the region to its bottom"
        );
        app.send_key("Home");
        assert_eq!(
            app.scroll_offsets[&region_key], 0,
            "Home rewinds the region"
        );

        // With the cursor outside the region the keys fall back to the window.
        app.cursor_pos = (20, 200);
        app.height = 120; // short window, so the window itself can scroll
        app.send_key("End");
        assert_eq!(
            app.scroll_y,
            (app.content_height - app.height as i32 + 20).max(0),
            "End scrolls the window when no region is under the cursor"
        );
    }

    /// When the content shrinks (messages deleted) the region's max offset
    /// shrinks with it. `scroll_region_by` may read a *stale* max from the
    /// previous layout, so after the gesture settles the stored offset must
    /// never exceed the fresh max — otherwise the viewport renders blank.
    #[test]
    fn test_region_offset_is_reclamped_when_content_shrinks() {
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
        let region_key = "scroll:feed".to_string();

        // 40 messages in a 100px viewport: plenty of headroom to scroll.
        let tall = chat_tree(40, 100);
        app.layout_root(&tall);
        app.cursor_pos = (20, 30);
        app.scroll_region_by(&region_key, 10_000);
        let big_max = app.scroll_regions[0].max_offset();
        assert_eq!(
            app.scroll_offsets[&region_key], big_max,
            "wheel clamps to the tall content's max"
        );

        // Delete most messages: the content now fits the viewport, so the
        // region has nothing to scroll.
        let short = chat_tree(3, 100);
        app.layout_root(&short);
        let fresh_max = app.scroll_regions[0].max_offset();
        assert_eq!(fresh_max, 0, "short content leaves nothing to scroll");
        assert_eq!(
            app.scroll_offsets[&region_key], 0,
            "layout re-clamps the stored offset to the fresh max after the\n\
             content shrinks, so the viewport never renders blank"
        );

        // A wheel gesture over the shrunken region must stay clamped and must
        // not resurrect the stale offset.
        app.scroll_region_by(&region_key, 500);
        assert_eq!(app.scroll_offsets[&region_key], 0);
    }

    /// Scroll inside scroll: both viewports clip, and the offsets compose.
    #[test]
    fn test_nested_scroll_containers_compose() {
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

        let mut inner = UiElement::new("scroll");
        inner.id = Some("inner".into());
        inner.props.insert("height".into(), "50".into());
        inner.props.insert("padding".into(), "0".into());
        for i in 0..8 {
            let mut t = UiElement::new("text");
            t.text = Some(format!("inner{i}"));
            inner.children.push(t);
        }

        let mut outer = UiElement::new("scroll");
        outer.id = Some("outer".into());
        outer.props.insert("height".into(), "200".into());
        outer.props.insert("padding".into(), "0".into());
        outer.props.insert("gap".into(), "10".into());
        outer.children.push(inner);
        // Filler below the inner region so the outer content overflows its
        // 200px viewport and actually has something to scroll.
        for i in 0..12 {
            let mut t = UiElement::new("text");
            t.text = Some(format!("outer{i}"));
            outer.children.push(t);
        }

        let mut root = UiElement::new("column");
        root.props.insert("padding".into(), "0".into());
        root.children.push(outer);
        app.layout_root(&root);

        assert_eq!(app.scroll_regions.len(), 2, "both viewports measured");

        app.scroll_offsets.insert("scroll:outer".into(), 30);
        app.layout_root(&root);

        let leaf = app
            .layout_boxes
            .iter()
            .find(|b| b.text.as_deref() == Some("inner3"))
            .expect("inner content laid out");
        assert_eq!(leaf.pan, (0, 30), "inner content inherits the outer offset");
        let clips = leaf.clips.as_ref().expect("clipped by both viewports");
        assert_eq!(
            clips.len(),
            2,
            "clipped by the outer and the inner viewport"
        );
        // The outer viewport only moves with its own ancestors (none here).
        assert_eq!(clips[0].pan, (0, 0));
        // The inner viewport is itself inside the outer region, so it pans.
        assert_eq!(clips[1].pan, (0, 30));
    }

    /// Clip helpers: nested viewports intersect, and clipped chrome paints
    /// strictly inside the viewport.
    #[test]
    fn test_screen_clip_intersects_nested_viewports() {
        let outer = ClipRegion {
            rect: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 80,
            },
            pan: (0, 0),
        };
        let inner = ClipRegion {
            rect: Rect {
                x: 10,
                y: 10,
                w: 40,
                h: 20,
            },
            pan: (0, 10),
        };
        let mut b = LayoutBox {
            rect: Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 80,
            },
            tag: "text".into(),
            id: None,
            text: None,
            on_click: None,
            on_change: None,
            on_key: None,
            pan: (0, 0),
            clips: None,
            pad: 0,
        };
        b.clips = Some(vec![outer, inner]);
        assert_eq!(
            b.screen_clip(0, 0),
            Some(Rect {
                x: 10,
                y: 0,
                w: 40,
                h: 20
            }),
            "the tighter viewport wins"
        );
        // Both viewports share the ancestor pan, so window scrolling shifts
        // the intersection without ever emptying it.
        assert_eq!(
            b.screen_clip(0, 60),
            Some(Rect {
                x: 10,
                y: -60,
                w: 40,
                h: 20
            })
        );

        // A viewport scrolled far past its ancestor clips everything away.
        let far = ClipRegion {
            rect: Rect {
                x: 10,
                y: 10,
                w: 40,
                h: 20,
            },
            pan: (0, 1000),
        };
        b.clips = Some(vec![outer, far]);
        assert_eq!(
            b.screen_clip(0, 0),
            None,
            "disjoint viewports clip the box away entirely"
        );
    }

    /// Clipped chrome paints strictly inside the viewport, never past it.
    #[test]
    fn test_rounded_box_clipped_paints_only_inside_clip() {
        const BG: u32 = 0xFF00_0000;
        let (stride, height) = (30usize, 30usize);
        let clip = Rect {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        let mut buf = vec![BG; stride * height];
        draw_rounded_rect_box_clipped(
            &mut buf,
            stride,
            height,
            Some(clip),
            0,
            0,
            30,
            30,
            6,
            0xFF00_00FF,
            0xFFFF_FFFF,
        );

        for y in 0..height {
            for x in 0..stride {
                if x >= 10 || y >= 10 {
                    assert_eq!(
                        buf[y * stride + x],
                        BG,
                        "pixel ({x},{y}) outside the clip must stay background"
                    );
                }
            }
        }
        assert!(
            buf.iter().take(100).any(|&p| p != BG),
            "the visible part of the control must still be painted"
        );
    }

    /// Build a bubble element with the given role and style overrides.
    fn bubble_elem(role: &str, msg: &str, props: &[(&str, &str)]) -> UiElement {
        let mut e = UiElement::new("bubble");
        e.id = Some("b".into());
        e.text = Some(msg.to_string());
        e.props.insert("role".into(), role.to_string());
        for (k, v) in props {
            e.props.insert((*k).to_string(), (*v).to_string());
        }
        e
    }

    /// A bubble wraps its message to `max_width` and pads it, so the box never
    /// grows past the cap no matter how long the message is.
    #[test]
    fn test_bubble_wraps_to_max_width_and_pads_the_box() {
        let long = "this is a fairly long chat message that would otherwise stretch \
                    the bubble well past a comfortable reading width";
        let elem = bubble_elem("assistant", long, &[("max_width", "240")]);

        let mut boxes = Vec::new();
        let (w, h) = layout_element(&elem, 10, 10, 600, &mut boxes);

        assert_eq!(boxes.len(), 1, "bubble lays out exactly one box");
        let b = &boxes[0];
        assert_eq!(b.tag, "bubble:assistant", "assistant role is recorded");
        assert_eq!(b.pad, 10, "default padding is carried to the box");
        assert!(
            b.rect.w <= 240,
            "bubble width {} must not exceed max_width 240",
            b.rect.w
        );
        assert_eq!(b.rect.w, w, "returned width matches the box");
        assert_eq!(b.rect.h, h, "returned height matches the box");
        assert!(
            h > CELL_H + 20,
            "a long message must wrap into several padded lines"
        );
        // Every wrapped line must fit inside the bubble's text area.
        for line in b.text.as_deref().unwrap_or("").split('\n') {
            let px = (line.chars().count() as i32) * CELL_W;
            assert!(
                px <= 240 - 20,
                "line '{line}' ({px}px) exceeds the padded text area"
            );
        }
    }

    /// `padding` widens the box symmetrically on both axes.
    #[test]
    fn test_bubble_padding_grows_the_box() {
        let bare = bubble_elem("assistant", "hello", &[]);
        let padded = bubble_elem("assistant", "hello", &[("padding", "24")]);

        let mut b1 = Vec::new();
        let (w1, h1) = layout_element(&bare, 10, 10, 600, &mut b1);
        let mut b2 = Vec::new();
        let (w2, h2) = layout_element(&padded, 10, 10, 600, &mut b2);

        assert_eq!(w2 - w1, 2 * (24 - 10), "padding grows width on both sides");
        assert_eq!(h2 - h1, 2 * (24 - 10), "padding grows height on both sides");
        assert_eq!(b2[0].pad, 24, "the box carries its padding through");
    }

    /// Alignment: a user bubble hugs the right edge, the assistant's the left,
    /// and an explicit `align` wins over the role default.
    #[test]
    fn test_bubble_alignment_defaults_by_role_and_can_be_overridden() {
        let parent_w = 400;
        let at = |role: &str, align: Option<&str>| -> Rect {
            let mut props = vec![];
            if let Some(a) = align {
                props.push(("align", a));
            }
            let elem = bubble_elem(role, "hello", &props);
            let mut boxes = Vec::new();
            layout_element(&elem, 10, 10, parent_w, &mut boxes);
            boxes[0].rect
        };

        let user = at("user", None);
        assert_eq!(
            user.x,
            10 + parent_w - user.w,
            "a user bubble is flush against the parent's right edge"
        );

        let assist = at("assistant", None);
        assert_eq!(
            assist.x, 10,
            "an assistant bubble is flush against the left"
        );

        let flipped = at("user", Some("left"));
        assert_eq!(
            flipped.x, 10,
            "an explicit align overrides the role's default side"
        );

        let centred = at("assistant", Some("center"));
        assert_eq!(
            centred.x,
            10 + (parent_w - centred.w) / 2,
            "center alignment sits halfway across the parent"
        );
    }

    /// Alignment, measured exactly: inside a scroll viewport an assistant
    /// bubble's left edge must sit `pad` from the viewport's left, and a user
    /// bubble's right edge `pad` from the viewport's right. Symmetric — no
    /// one-sided drift.
    #[test]
    fn test_bubble_alignment_is_symmetric_inside_a_viewport() {
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
                width: 420,
                height: 360,
            },
            dummy,
        );

        let mut feed = UiElement::new("scroll");
        feed.id = Some("feed".into());
        feed.props.insert("height".into(), "150".into());
        feed.props.insert("padding".into(), "8".into());
        feed.props.insert("gap".into(), "8".into());
        feed.children
            .push(bubble_elem("assistant", "left side", &[]));
        feed.children.push(bubble_elem("user", "right side", &[]));

        let mut root = UiElement::new("column");
        root.props.insert("padding".into(), "12".into());
        root.children.push(feed);
        app.layout_root(&root);

        let viewport = &app.scroll_regions[0].rect;
        let mut seen = [false; 2];
        for b in &app.layout_boxes {
            if b.tag == "bubble:assistant" {
                seen[0] = true;
                assert_eq!(
                    b.rect.x - viewport.x,
                    8,
                    "assistant bubble must be padded 8px from the viewport's left"
                );
            } else if b.tag == "bubble:user" {
                seen[1] = true;
                let right_gap = (viewport.x + viewport.w) - (b.rect.x + b.rect.w);
                assert_eq!(
                    right_gap,
                    8,
                    "user bubble must be padded 8px from the viewport's right \
                     (viewport right {}, bubble right {})",
                    viewport.x + viewport.w,
                    b.rect.x + b.rect.w
                );
            }
        }
        assert!(
            seen[0] && seen[1],
            "both roles must have laid out (got {:?})",
            seen
        );
    }

    /// Bubbles inherit pan + clip like every other box, so one scrolled out of
    /// its viewport leaves no painted pixels behind.
    #[test]
    fn test_bubble_inherits_scroll_pan_and_viewport_clip() {
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

        let mut feed = UiElement::new("scroll");
        feed.id = Some("feed".into());
        feed.props.insert("height".into(), "80".into());
        feed.props.insert("padding".into(), "0".into());
        feed.props.insert("gap".into(), "6".into());
        for i in 0..8 {
            feed.children
                .push(bubble_elem("assistant", &format!("message {i}"), &[]));
        }
        let mut root = UiElement::new("column");
        root.props.insert("padding".into(), "0".into());
        root.children.push(feed);

        app.layout_root(&root);

        let first = app
            .layout_boxes
            .iter()
            .find(|b| b.tag.starts_with("bubble"))
            .expect("bubble laid out inside the scroll region");
        assert!(
            first.clips.is_some(),
            "a bubble inside a scroll region must carry the viewport clip"
        );
        let clips = first.clips.as_ref().unwrap();
        assert_eq!(clips.len(), 1, "clipped by its own viewport");
        assert_eq!(clips[0].rect.h, 80, "the clip is the viewport, not the box");

        // Scrolling the region pans the bubble's screen position. `render()`
        // would re-run layout, but it needs a live surface, so the test drives
        // the same layout pass the renderer would.
        let y_before = first.rect.y - app.scroll_y - first.pan.1;
        app.scroll_region_by("scroll:feed", 40);
        app.layout_root(&root);
        let after = app
            .layout_boxes
            .iter()
            .find(|b| b.tag.starts_with("bubble"))
            .expect("re-laid out after scrolling");
        let y_after = after.rect.y - app.scroll_y - after.pan.1;
        assert_eq!(
            y_after,
            y_before - 40,
            "scrolling the region moves the bubble up by the offset"
        );

        // A bubble scrolled above the viewport is not visible at all.
        assert!(
            !after.visible_in(
                app.scroll_x,
                app.scroll_y,
                app.width as i32,
                app.height as i32
            ),
            "the first bubble is clipped away once scrolled past"
        );
    }

    /// The rounded background stays inside the viewport clip, so a bubble can
    /// never bleed over content outside its scroll region.
    #[test]
    fn test_bubble_background_is_clipped_to_the_viewport() {
        const BG: u32 = 0xFF00_0000;
        const FILL: u32 = 0xFF3B_82F6; // Theme::DARK.bubble_user_bg
        let stride = 60usize;
        let height_px = 60usize;

        let mut buf = vec![BG; stride * height_px];
        // A bubble whose left half sits inside the clip and right half outside.
        let clip = Rect {
            x: 0,
            y: 0,
            w: 30,
            h: 60,
        };
        draw_rounded_rect_box_clipped(
            &mut buf,
            stride,
            height_px,
            Some(clip),
            20,
            10,
            40,
            30,
            8,
            FILL,
            0xFF11_2233,
        );

        assert_eq!(
            buf[25 * stride + 25],
            FILL,
            "the part inside the viewport is painted"
        );
        for x in 30..60 {
            assert_eq!(
                buf[25 * stride + x],
                BG,
                "pixel x={x} past the viewport edge must stay background"
            );
        }
    }
}
