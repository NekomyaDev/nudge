//! Real native OS window backend for Nudge's UI using winit 0.30 and softbuffer 0.4.
#![cfg(feature = "native-gui")]

use crate::font8x8::get_glyph;
use crate::font_engine::{FaceKind, FontEngine, BASE_PX, CELL_H, CELL_W};
use crate::ui::{
    is_truthy, TextInputState, UiElement, PROGRESS_H, PROGRESS_TRACK_H, PROGRESS_TRACK_W, SLIDER_H,
    SLIDER_KNOB, SLIDER_TRACK_H, SLIDER_TRACK_W, SPINNER_ARC_DEG, SPINNER_PERIOD_SECS,
    SPINNER_SIZE, SPINNER_STROKE,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId};

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
    /// The palette to paint the next frame in, if the program has an opinion.
    ///
    /// Asked on every frame rather than once at startup so state can drive it:
    /// a theme toggle is nothing more than state plus a button, and the switch
    /// lands on the very next paint. Returning `None` keeps whatever is already
    /// in force.
    fn theme(&mut self) -> Option<Theme> {
        None
    }
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
    /// Lighter end of the button's vertical gradient; `button_bg` is the bottom
    /// edge, so controls read as subtly domed instead of flat slabs. The hover
    /// state runs `button_bg` → `button_bg_hover`, i.e. one step darker overall
    /// than the resting gradient, which matches how the flat tones related.
    pub button_bg_top: u32,
    /// Bottom of the pressed (mouse-down) ramp, whose top is `button_bg_hover` —
    /// so the three states form one continuous ladder (400/500 → 500/600 →
    /// 600/700) rather than three unrelated colours.
    pub button_bg_pressed: u32,
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
    /// Colour every drop shadow is blended in (alpha is decided per element
    /// by `blur`/`strength`). Kept in the theme so a light surface can switch
    /// to a softer grey instead of pure black.
    pub shadow: u32,
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
        button_bg_top: 0xFF60_A5FA,         // Blue-400 (gradient top)
        button_bg_pressed: 0xFF1D_4ED8,     // Blue-700 (pressed ladder rung)
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
        shadow: 0xFF00_0000,                // black; strength is per element
    };

    /// KDE Breeze Dark theme: official KDE Plasma 6 charcoal + Breeze Blue palette.
    pub const BREEZE_DARK: Theme = Theme {
        bg: 0xFF24_272B,                    // KDE Breeze Dark base
        text: 0xFFFC_FCFC,                  // Breeze crisp text
        text_dim: 0xFF8A_95A0,              // Breeze muted grey
        border: 0xFF3A_4048,                // Breeze separator / border
        input_bg: 0xFF1B_1E20,              // Dark input field
        input_border: 0xFF3A_4048,          // Input border
        input_border_focused: 0xFF3D_AEE9,  // KDE Breeze Blue
        button_bg: 0xFF31_363B,             // Breeze button surface
        button_bg_hover: 0xFF3D_AEE9,       // Breeze Blue hover
        button_bg_top: 0xFF38_3E45,         // Subtle dome top
        button_bg_pressed: 0xFF1D_80B5,     // Pressed blue ladder rung
        button_border: 0xFF4D_5660,         // Button border
        button_text: 0xFFFC_FCFC,           // Breeze button text
        selection_bg: 0xFF3D_AEE9,          // KDE Breeze Blue active selection
        cursor: 0xFFFC_FCFC,                // white caret
        scrollbar_track: 0xFF1E_2022,       // Dark scroll track
        scrollbar_thumb: 0xFF4D_5660,       // Scroll thumb
        bubble_user_bg: 0xFF3D_AEE9,        // Breeze Blue
        bubble_user_text: 0xFFFF_FFFF,      // white
        bubble_assistant_bg: 0xFF31_363B,   // Assistant card
        bubble_assistant_text: 0xFFFC_FCFC, // white
        shadow: 0xFF00_0000,                // black
    };

    /// Zinc-50 light theme: the same ladder, walked the other way.
    pub const LIGHT: Theme = Theme {
        bg: 0xFFFA_FAFA,                    // Zinc-50
        text: 0xFF18_181B,                  // Zinc-900
        text_dim: 0xFF71_717A,              // Zinc-500 — muted on either surface
        border: 0xFFE4_E4E7,                // Zinc-200
        input_bg: 0xFFFF_FFFF,              // white
        input_border: 0xFFA1_A1AA,          // Zinc-400
        input_border_focused: 0xFF25_63EB,  // Blue-600
        button_bg: 0xFF25_63EB,             // Blue-600
        button_bg_hover: 0xFF1D_4ED8,       // Blue-700
        button_bg_top: 0xFF3B_82F6,         // Blue-500 (gradient top)
        button_bg_pressed: 0xFF1E_40AF,     // Blue-800 (pressed ladder rung)
        button_border: 0xFF3B_82F6,         // Blue-500
        button_text: 0xFFFF_FFFF,           // white
        selection_bg: 0xFFBF_DBFE,          // Blue-200 — behind dark text
        cursor: 0xFF18_181B,                // Zinc-900 — a white caret would vanish
        scrollbar_track: 0xFFE4_E4E7,       // Zinc-200
        scrollbar_thumb: 0xFFA1_A1AA,       // Zinc-400
        bubble_user_bg: 0xFF25_63EB,        // Blue-600
        bubble_user_text: 0xFFFF_FFFF,      // white
        bubble_assistant_bg: 0xFFFF_FFFF,   // white — a raised surface
        bubble_assistant_text: 0xFF18_181B, // Zinc-900
        shadow: 0xFF71_717A,                // a soft grey; black would smudge
    };

    /// KDE Breeze Light theme: official KDE Plasma 6 clean light palette.
    pub const BREEZE_LIGHT: Theme = Theme {
        bg: 0xFFEF_F0F1,                    // Breeze Light base
        text: 0xFF23_2629,                  // Breeze dark text
        text_dim: 0xFF7F_8C8D,              // Breeze muted text
        border: 0xFFBD_C3C7,                // Breeze light border
        input_bg: 0xFFFF_FFFF,              // Pure white input field
        input_border: 0xFFBD_C3C7,          // Input border
        input_border_focused: 0xFF3D_AEE9,  // KDE Breeze Blue
        button_bg: 0xFFDF_E1E2,             // Breeze light button surface
        button_bg_hover: 0xFF3D_AEE9,       // Breeze Blue hover
        button_bg_top: 0xFFE5_E7E8,         // Button dome top
        button_bg_pressed: 0xFF29_80B9,     // Pressed blue
        button_border: 0xFFBD_C3C7,         // Button border
        button_text: 0xFF23_2629,           // Dark text
        selection_bg: 0xFF3D_AEE9,          // KDE Breeze Blue selection
        cursor: 0xFF23_2629,                // Dark caret
        scrollbar_track: 0xFFDF_E1E2,       // Scroll track
        scrollbar_thumb: 0xFFBD_C3C7,       // Scroll thumb
        bubble_user_bg: 0xFF3D_AEE9,        // Breeze Blue
        bubble_user_text: 0xFFFF_FFFF,      // white
        bubble_assistant_bg: 0xFFFF_FFFF,   // white card
        bubble_assistant_text: 0xFF23_2629, // dark text
        shadow: 0xFF7F_8C8D,                // soft grey
    };

    /// The palette a `theme:` window field names, if it names one at all.
    pub fn named(name: &str) -> Option<Theme> {
        match name.trim().to_ascii_lowercase().as_str() {
            "breeze" | "breeze-dark" => Some(Theme::BREEZE_DARK),
            "breeze-light" => Some(Theme::BREEZE_LIGHT),
            "dark" => Some(Theme::DARK),
            "light" => Some(Theme::LIGHT),
            _ => None,
        }
    }

    pub fn is_dark(&self) -> bool {
        self.bg == Theme::DARK.bg || self.bg == Theme::BREEZE_DARK.bg
    }
}

pub fn parse_hex_color(s: &str) -> Option<u32> {
    let raw = s.strip_prefix('#').unwrap_or(s);
    if raw.len() == 6 {
        let v = u32::from_str_radix(raw, 16).ok()?;
        Some(0xFF00_0000 | v)
    } else if raw.len() == 8 {
        u32::from_str_radix(raw, 16).ok()
    } else {
        None
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

    pub fn union(&self, other: &Rect) -> Rect {
        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = (self.x + self.w).max(other.x + other.w);
        let y2 = (self.y + self.h).max(other.y + other.h);
        Rect {
            x: x1,
            y: y1,
            w: (x2 - x1).max(0),
            h: (y2 - y1).max(0),
        }
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

/// Thickness of the window's scrollbars, in pixels.
pub const SCROLLBAR_SIZE: i32 = 8;
/// Shortest a thumb may be drawn, so it stays grabbable however long the
/// content is. Past this point the thumb no longer tracks the content
/// proportionally — the track simply runs out — but it never vanishes.
pub const SCROLLBAR_MIN_THUMB: i32 = 24;

/// Which of the window's two scrollbars something refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarAxis {
    Vertical,
    Horizontal,
}

/// How far an axis allows scrolling, including the 20px of slack the layout
/// keeps below the last row. Sharing this with the clamp is what makes a thumb
/// reach the track's end at exactly the moment the content stops moving.
pub fn max_scroll(content: i32, viewport: i32) -> i32 {
    (content - viewport + 20).max(0)
}

/// Where each window scrollbar's track sits, or `None` for an axis that has
/// nothing to scroll.
///
/// The two tracks are laid out so neither overlaps the other: the vertical bar
/// keeps the full right edge, corner included, and a horizontal one stops
/// short of it. An axis with no scroll therefore gets the whole edge, and the
/// geometry never leaves an unreachable square where neither bar answers.
pub fn scrollbar_tracks(
    win_w: i32,
    win_h: i32,
    content_w: i32,
    content_h: i32,
) -> (Option<Rect>, Option<Rect>) {
    let vertical = (content_h > win_h).then_some(Rect {
        x: win_w - SCROLLBAR_SIZE,
        y: 0,
        w: SCROLLBAR_SIZE,
        h: win_h,
    });
    let horizontal = (content_w > win_w).then_some(Rect {
        x: 0,
        y: win_h - SCROLLBAR_SIZE,
        // The vertical bar owns the corner, so a horizontal one runs up to it
        // rather than underneath it — leaving no square where neither bar
        // answers the pointer.
        w: if content_h > win_h {
            (win_w - SCROLLBAR_SIZE).max(0)
        } else {
            win_w
        },
        h: SCROLLBAR_SIZE,
    });
    (vertical, horizontal)
}

/// How long a thumb is along its track, given the track's length, how much of
/// the content is visible at once, the total content, and the axis' largest
/// offset.
///
/// Proportional to the visible share so it reads as "how much of this am I
/// seeing", floored at [`SCROLLBAR_MIN_THUMB`] so it stays grabbable, and
/// capped at the track itself.
pub fn scrollbar_thumb_len(track: i32, viewport: i32, content: i32, max_offset: i32) -> i32 {
    if track <= 0 || content <= 0 || max_offset <= 0 {
        return 0;
    }
    let proportional = ((track as i64 * viewport as i64) / content as i64) as i32;
    proportional.clamp(SCROLLBAR_MIN_THUMB, track)
}

/// Where along its track a thumb sits, in track-local pixels, for the given
/// offset. Pinned to the far end exactly when the axis is at its limit, so the
/// thumb never overshoots the track however the offset was reached.
pub fn scrollbar_thumb_pos(track: i32, thumb: i32, offset: i32, max_offset: i32) -> i32 {
    let travel = (track - thumb).max(0);
    if travel == 0 || max_offset <= 0 {
        return 0;
    }
    let clamped = offset.clamp(0, max_offset) as i64;
    ((clamped * travel as i64) / max_offset as i64) as i32
}

/// The offset a drag to `pos` (track-local pixels) asks for. The inverse of
/// [`scrollbar_thumb_pos`], so dragging the thumb to where it would have been
/// drawn at some offset gives that offset back.
pub fn scrollbar_offset_at(travel: i32, pos: i32, max_offset: i32) -> i32 {
    if travel <= 0 || max_offset <= 0 {
        return 0;
    }
    let clamped = pos.clamp(0, travel) as i64;
    ((clamped * max_offset as i64) / travel as i64) as i32
}

/// Thumb and track tones for a scrollbar, blended toward the theme's text
/// colour by how much attention the bar is asking for.
///
/// Blending toward `text` rather than a fixed highlight means a light theme
/// darkens its thumb and a dark theme lightens it — either way the contrast
/// against the track rises, which is the entire point of the highlight.
pub fn scrollbar_tones(theme: &Theme, hovered: bool, active: bool) -> (u32, u32) {
    let amount = if active {
        0.5
    } else if hovered {
        0.3
    } else {
        0.0
    };
    (
        lerp_color(theme.scrollbar_thumb, theme.text, amount),
        lerp_color(theme.scrollbar_track, theme.text, amount * 0.4),
    )
}

/// Both window scrollbars resolved in one pass: the track each one runs in,
/// and the thumb that shows where the content currently is.
///
/// Drawing and hit-testing both read this, so a thumb can never be drawn in
/// one place and grabbed in another — the failure mode that makes scrollbars
/// feel broken in ways nobody can point at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollbarGeometry {
    /// `(track, thumb)` in screen space.
    pub vertical: Option<(Rect, Rect)>,
    /// `(track, thumb)` in screen space.
    pub horizontal: Option<(Rect, Rect)>,
}

impl ScrollbarGeometry {
    /// Resolve both bars against the window and the current scroll offsets.
    /// A thumb is laid out *inside* its track with a pixel of margin, so it
    /// reads as a pill rather than a filled channel.
    pub fn resolve(
        win_w: i32,
        win_h: i32,
        content_w: i32,
        content_h: i32,
        scroll_x: i32,
        scroll_y: i32,
    ) -> Self {
        let (v_track, h_track) = scrollbar_tracks(win_w, win_h, content_w, content_h);

        let vertical = v_track.and_then(|t| {
            let max = max_scroll(content_h, win_h);
            let len = scrollbar_thumb_len(t.h, win_h, content_h, max);
            if len <= 0 {
                return None;
            }
            let pos = scrollbar_thumb_pos(t.h, len, scroll_y, max);
            Some((
                t,
                Rect {
                    x: t.x + 1,
                    y: t.y + pos,
                    w: (t.w - 2).max(1),
                    h: len,
                },
            ))
        });

        let horizontal = h_track.and_then(|t| {
            let max = max_scroll(content_w, win_w);
            let len = scrollbar_thumb_len(t.w, win_w, content_w, max);
            if len <= 0 {
                return None;
            }
            let pos = scrollbar_thumb_pos(t.w, len, scroll_x, max);
            Some((
                t,
                Rect {
                    x: t.x + pos,
                    y: t.y + 1,
                    w: len,
                    h: (t.h - 2).max(1),
                },
            ))
        });

        Self {
            vertical,
            horizontal,
        }
    }

    /// Which bar the pointer is over, if either. A thumb beats the track it
    /// sits in, so a press that lands on the thumb is a grab and never a jump.
    pub fn axis_at(&self, pos: (i32, i32)) -> Option<ScrollbarAxis> {
        let (px, py) = pos;
        if let Some((_, thumb)) = self.vertical {
            if thumb.contains(px, py) {
                return Some(ScrollbarAxis::Vertical);
            }
        }
        if let Some((_, thumb)) = self.horizontal {
            if thumb.contains(px, py) {
                return Some(ScrollbarAxis::Horizontal);
            }
        }
        if let Some((track, _)) = self.vertical {
            if track.contains(px, py) {
                return Some(ScrollbarAxis::Vertical);
            }
        }
        if let Some((track, _)) = self.horizontal {
            if track.contains(px, py) {
                return Some(ScrollbarAxis::Horizontal);
            }
        }
        None
    }

    /// Whether the pointer is on a *thumb* rather than bare track.
    pub fn on_thumb(&self, pos: (i32, i32)) -> bool {
        let (px, py) = pos;
        self.vertical
            .iter()
            .chain(self.horizontal.iter())
            .any(|(_, thumb)| thumb.contains(px, py))
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
    /// From `disabled = true`: the control still lays out and draws, but dimmed
    /// — it is not hoverable, never shows the pointer cursor and never
    /// dispatches, and it absorbs clicks instead of letting them fall through
    /// to whatever sits behind it.
    pub disabled: bool,
    /// From `tooltip = "..."`: a label drawn as a floating card near the
    /// cursor while this box is hovered. Purely an overlay, so it is never
    /// measured and cannot shift layout.
    pub tooltip: Option<String>,
    /// Whether a `checkbox` or `toggle` is switched on. Carried on the box
    /// rather than looked up from the element tree for the same reason the
    /// options are: the drawing, the press and the value written back all have
    /// to agree about one snapshot, and a value that changed underneath a
    /// painted control would draw a mark for a state that is no longer there.
    /// `false` for every other tag.
    pub checked: bool,
    /// The number a `slider` is showing, and the grid it may rest on. Carried
    /// on the box for the same reason the boolean state is: the drawing, the
    /// drag and the value written back all have to agree about one snapshot,
    /// and a value that changed underneath a dragging handle would draw the
    /// handle at a position the number no longer names. `None` for every
    /// other tag.
    pub slider: Option<SliderSpec>,
    /// The choice list of a `select`, in order, empty for every other tag.
    /// Carried on the box rather than looked up from the element tree so the
    /// open popup, its hit test and its keyboard highlight all read the same
    /// snapshot the width was measured from — a list that changed underneath
    /// a painted popup would highlight a row that is no longer there.
    pub options: Vec<String>,
    /// Whether this element can be dragged to reorder.
    pub draggable: bool,
    /// Element properties map preserved from the AST (e.g. w, h, variant, align, bg, selected).
    pub props: std::collections::HashMap<String, String>,
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
pub fn draw_rect_alpha_clipped(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
    alpha: f32,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    if let Some(c) = clip {
        if let Some(r) = intersect(Rect { x, y, w, h }, c) {
            draw_rect_alpha(buf, stride, height, r.x, r.y, r.w, r.h, color, alpha);
        }
    } else {
        draw_rect_alpha(buf, stride, height, x, y, w, h, color, alpha);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_rect_clipped(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    draw_rect_alpha_clipped(buf, stride, height, clip, x, y, w, h, color, 1.0);
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

/// How long a hover ramp takes to travel from the resting tone to the hovered
/// one and back. Short enough to read as instant feedback, long enough that
/// the eye sees a change rather than a jump.
const HOVER_RAMP_SECS: f32 = 0.12;

/// One animation tick while something is moving (~60fps). Only ever used as a
/// deadline while an animation is actually outstanding — an idle window
/// schedules nothing and never wakes for it.
const RAMP_TICK: Duration = Duration::from_millis(16);

/// Frame duration for an indeterminate spinner rotating smoothly (~30fps).
const SPINNER_TICK: Duration = Duration::from_millis(33);

/// Ceiling on how much time a single frame may claim to have covered. Without
/// it, the first frame after a long idle would carry the whole stall as `dt`
/// and a ramp that had not started yet would teleport to its end state.
const MAX_ANIM_STEP: Duration = Duration::from_millis(33);

/// Caret blink: on for this long, off for the same again, repeating. The
/// classic rhythm, and the reason a focused field needs a clock at all.
const CARET_BLINK_ON: Duration = Duration::from_millis(265);
const CARET_BLINK_OFF: Duration = Duration::from_millis(265);

/// The window's only timer: a single wake-up deadline that every timed part of
/// the UI agrees on. The event loop sleeps in `ControlFlow::Wait` while this is
/// idle, so a still interface costs no CPU, and switches to `WaitUntil` only
/// while a ramp or a caret blink is outstanding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FrameClock {
    next: Option<Instant>,
}

impl FrameClock {
    fn new() -> Self {
        Self::default()
    }

    /// The moment the next scheduled frame is due, if one is outstanding.
    fn next_deadline(&self) -> Option<Instant> {
        self.next
    }

    fn is_idle(&self) -> bool {
        self.next.is_none()
    }

    /// Bring the next wake-up forward to at most `now + after`. A later ask
    /// never postpones an earlier one: the caret's next toggle arriving
    /// mid-ramp must not delay the ramp's own last frame.
    fn wake_in(&mut self, now: Instant, after: Duration) {
        let at = now + after;
        self.next = Some(match self.next {
            Some(existing) => existing.min(at),
            None => at,
        });
    }

    /// Drop the outstanding wake-up, having drawn the frame it promised.
    fn clear(&mut self) {
        self.next = None;
    }
}

/// Signed distance from pixel `(px, py)` to a rounded rectangle: negative
/// inside the shape, `0.0` on its edge, positive outside — measured in whole
/// pixels. This is the shared basis for [`rounded_rect_coverage`] (a 1px band
/// around the edge) and for the drop shadow's falloff, so a shadow's halo
/// follows exactly the same arc the visible corners do.
#[inline]
fn rounded_rect_sdf(x: i32, y: i32, w: i32, h: i32, r: i32, px: i32, py: i32) -> f32 {
    // Never round more than half the widget, otherwise the shape inverts.
    let r = (r as f32).min(w as f32 / 2.0).min(h as f32 / 2.0).max(0.0);
    let hw = w as f32 / 2.0;
    let hh = h as f32 / 2.0;
    let dx = (px as f32 + 0.5 - (x as f32 + hw)).abs();
    let dy = (py as f32 + 0.5 - (y as f32 + hh)).abs();
    let qx = dx - hw + r;
    let qy = dy - hh + r;
    qx.max(qy).min(0.0) + qx.max(0.0).hypot(qy.max(0.0)) - r
}

/// Anti-aliased coverage of a rounded rectangle at the centre of pixel
/// `(px, py)` using the standard rounded-box signed distance field.
///
/// Returns `1.0` well inside, `0.0` outside, and a graded value across the
/// ~1px arc so corners are smooth instead of stair-stepped.
#[inline]
fn rounded_rect_coverage(x: i32, y: i32, w: i32, h: i32, r: i32, px: i32, py: i32) -> f32 {
    (0.5 - rounded_rect_sdf(x, y, w, h, r, px, py)).clamp(0.0, 1.0)
}

/// Soft drop shadow: a halo around the rounded rect at `(x, y)` that fades to
/// nothing over `blur` pixels, nudged down/right by `(ox, oy)` so the shape
/// reads as floating above the surface.
///
/// The falloff is a smoothstep over the *signed* distance rather than the
/// traditional stack of concentric alpha rings, so it costs one pass and has
/// no visible banding.
///
/// Callers must draw the shape itself **after** this (and after anything the
/// shadow could touch), otherwise the dark halo lands on top of a neighbouring
/// box. [`NativeApp::draw_shadows`] keeps that ordering for bubbles.
#[allow(clippy::too_many_arguments)]
pub fn draw_rounded_rect_shadow_clipped(
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
    blur: i32,
    ox: i32,
    oy: i32,
) {
    if w <= 0 || h <= 0 || blur <= 0 || alpha <= 0.0 {
        return;
    }
    let r = r.min(w / 2).min(h / 2);
    // The halo reaches `blur` past the *offset* shape, so scan that grown box.
    let sx = x + ox - blur;
    let sy = y + oy - blur;
    let sw = w + 2 * blur;
    let sh = h + 2 * blur;

    let x_start = sx.max(0) as usize;
    let y_start = sy.max(0) as usize;
    let x_end = ((sx + sw).max(0) as usize).min(stride);
    let y_end = ((sy + sh).max(0) as usize).min(height);
    if x_start >= x_end || y_start >= y_end {
        return;
    }
    let inv = 1.0 / blur as f32;

    for row in y_start..y_end {
        let row_offset = row * stride;
        for col in x_start..x_end {
            let (px, py) = (col as i32, row as i32);
            if let Some(c) = clip {
                if px < c.x || py < c.y || px >= c.x + c.w || py >= c.y + c.h {
                    continue;
                }
            }
            let d = rounded_rect_sdf(x + ox, y + oy, w, h, r, px, py);
            if d >= blur as f32 {
                continue;
            }
            // Under the shape the halo is at full strength (the shape itself
            // covers it); outside it ramps down to zero with a soft shoulder.
            let t = if d <= 0.0 { 1.0 } else { 1.0 - d * inv };
            let a = (alpha * t * t * (3.0 - 2.0 * t)).clamp(0.0, 1.0);
            if a <= 0.0 {
                continue;
            }
            blend_px(buf, row_offset + col, color, a);
        }
    }
}

/// Drop-shadow parameters for a box, keyed by its tag: `(blur, alpha, ox, oy)`.
///
/// Bubbles are the only card-like chrome so far, so they are the only things
/// that float above the surface. Controls (buttons/inputs) stay flat — they
/// already read as raised through their border and hover tone, and giving
/// every widget a halo would flatten the hierarchy rather than build it.
///
/// The blur is deliberately smaller than the feed's default 8px gap so the
/// halo of one bubble stops short of the next: no stacked dark seams down the
/// column.
pub fn shadow_for(tag: &str) -> Option<(i32, f32, i32, i32)> {
    if tag.starts_with("bubble:") {
        Some((6, 0.5, 0, 2))
    } else {
        None
    }
}

/// Paint every box's drop shadow, before anything else is drawn.
///
/// Shadows are deliberately their own pass rather than part of each box's arm:
/// a box drawn later must never have its halo land on top of a box drawn
/// earlier, or two stacked bubbles would leave a grey seam between them. Free
/// of `self` because `render` already holds `&mut` on the surface buffer.
pub fn draw_shadows(
    boxes: &[LayoutBox],
    scroll_x: i32,
    scroll_y: i32,
    buffer: &mut [u32],
    stride: usize,
    height: usize,
    theme: &Theme,
) {
    for lbox in boxes {
        let Some((blur, alpha, ox, oy)) = shadow_for(&lbox.tag) else {
            continue;
        };
        // Off-screen boxes are still considered: their halo can legally reach
        // into view even when the box itself sits just outside.
        let clip = lbox.screen_clip(scroll_x, scroll_y);
        let rx = lbox.rect.x - scroll_x - lbox.pan.0;
        let ry = lbox.rect.y - scroll_y - lbox.pan.1;
        draw_rounded_rect_shadow_clipped(
            buffer,
            stride,
            height,
            clip,
            rx,
            ry,
            lbox.rect.w,
            lbox.rect.h,
            10,
            theme.shadow,
            alpha,
            blur,
            ox,
            oy,
        );
    }
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

/// The `▾` a closed `select` wears. Drawn from rectangles rather than a
/// glyph, because a font is not obliged to carry that codepoint — and a
/// choice field that lost the one mark saying it opens would be a field
/// nobody dares click. Seven pixels wide, four tall: a filled triangle that
/// still reads at the default UI size.
pub fn draw_chevron_down(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    color: u32,
) {
    for (row, w) in [7i32, 5, 3, 1].into_iter().enumerate() {
        let row_rect = Rect {
            x: x + (7 - w) / 2,
            y: y + row as i32,
            w,
            h: 1,
        };
        let drawn = match clip {
            None => Some(row_rect),
            Some(c) => intersect(row_rect, c),
        };
        if let Some(r) = drawn {
            draw_rect(buf, stride, height, r.x, r.y, r.w, r.h, color);
        }
    }
}

/// The mark a switched-on `checkbox` wears, drawn one column at a time: two
/// arms meeting at a low vertex. A font need not carry ✓, and a control that
/// could not draw its own mark would say nothing about whether it is on.
///
/// `x, y` is the top-left of the mark's 11 × 8 box — the caller centres it in
/// the square it belongs to.
pub fn draw_check_mark(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    color: u32,
) {
    for i in 0..6i32 {
        // The arm that falls to the right, then the arm that climbs away
        // from it. Two pixels tall so the mark reads as a stroke rather than
        // a dotted suggestion at the size a checkbox actually is.
        let marks = [
            Rect {
                x: x + i,
                y: y + 4 + i,
                w: 1,
                h: 2,
            },
            Rect {
                x: x + 5 + i,
                y: y + 8 - i,
                w: 1,
                h: 2,
            },
        ];
        for m in marks {
            let drawn = match clip {
                None => Some(m),
                Some(c) => intersect(m, c),
            };
            if let Some(r) = drawn {
                draw_rect(buf, stride, height, r.x, r.y, r.w, r.h, color);
            }
        }
    }
}

/// Where the control sits inside its row: at the row's left edge, vertically
/// centred, in whatever size its tag draws. One function, so layout, the
/// drawing and a test cannot quietly disagree about where it is.
pub fn flag_control_rect(row: Rect, tag: &str) -> Rect {
    let h = if tag == "toggle" { TOGGLE_H } else { FLAG_BOX };
    Rect {
        x: row.x,
        y: row.y + ((row.h - h) / 2).max(0),
        w: flag_control_w(tag),
        h,
    }
}

/// One boolean control as the drawing needs it — where it sits and what
/// state it is in — so the render pass hands over a single value instead of
/// five loose ones, and a test can describe a control the same way without
/// building a window first.
#[derive(Debug, Clone, PartialEq)]
pub struct FlagView {
    /// `"checkbox"` or `"toggle"`. The two share every layer below the
    /// drawing, so the tag is only ever asked which clothes to wear.
    pub tag: String,
    /// The control's own rect, not the row's: the label beside it is the
    /// row's business, and only the row knows how wide it was made.
    pub rect: Rect,
    pub on: bool,
    /// How far the pointer has travelled from resting to hovered, `0.0` to
    /// `1.0`. Always zero for a control that is disabled, which never lights.
    pub hover_t: f32,
    pub disabled: bool,
}

impl FlagView {
    /// A control described from its row: `row` is the whole laid-out row in
    /// whatever space the caller is drawing in, and the control is placed
    /// inside it by [`flag_control_rect`].
    pub fn new(tag: &str, row: Rect, on: bool, hover_t: f32, disabled: bool) -> FlagView {
        FlagView {
            tag: tag.to_string(),
            rect: flag_control_rect(row, tag),
            on,
            hover_t,
            disabled,
        }
    }
}

/// One boolean control — the square of a `checkbox` or the track of a
/// `toggle`, with the mark or the knob inside it — painted entirely from
/// rectangles. Nothing here asks a font for a glyph, so a checkbox cannot
/// arrive as a row of letters and a switch cannot lose the one sign saying
/// which way it is set.
///
/// `rect` is the control itself, not the row it sits in: the label beside it
/// is drawn by the caller, because only the caller knows how wide the box was
/// made. Pure, so the drawing can be judged without a window.
pub fn draw_flag_control(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    theme: &Theme,
    flag: &FlagView,
) {
    let rect = flag.rect;
    let on = flag.on;
    let disabled = flag.disabled;
    let is_toggle = flag.tag == "toggle";
    let (cx, cy, cw, ch) = (rect.x, rect.y, rect.w, rect.h);
    // "On" wears the same blue a focused field wears, so the form has one
    // idea of *live* instead of two blues arguing over what an accent is.
    let accent = theme.input_border_focused;
    let (fill, mut edge) = if disabled {
        (lerp_color(theme.input_bg, theme.bg, 0.5), theme.border)
    } else if on {
        (accent, accent)
    } else {
        (theme.input_bg, theme.input_border)
    };
    // Hover lifts the border the way a choice field's does, so the control
    // that will flip is visible before it does. Disabled never lights — there
    // is nothing to look forward to — and a control already on has nothing
    // left to reveal.
    if !disabled && !on {
        edge = lerp_color(edge, accent, flag.hover_t);
    }

    if is_toggle {
        // A pill: radius half the height, so the two ends
        // curve as much as they possibly can and the track
        // reads as one continuous switch rather than a
        // rectangle with soft corners.
        draw_rounded_rect_box_clipped(
            buf,
            stride,
            height,
            clip,
            cx,
            cy,
            cw,
            ch,
            ch / 2,
            fill,
            edge,
        );
        // The knob travels rather than changes: on and off
        // are told apart by where it sits, which is the one
        // difference a switch has that does not depend on
        // reading anything.
        let knob = ch - 6;
        let inset = 3;
        let kx = if on {
            cx + cw - knob - inset
        } else {
            cx + inset
        };
        let knob_bg = if disabled {
            theme.border
        } else if on {
            theme.button_text
        } else {
            theme.text_dim
        };
        draw_rounded_rect_box_clipped(
            buf,
            stride,
            height,
            clip,
            kx,
            cy + inset,
            knob,
            knob,
            knob / 2,
            knob_bg,
            knob_bg,
        );
    } else {
        draw_rounded_rect_box_clipped(buf, stride, height, clip, cx, cy, cw, ch, 4, fill, edge);
        // The mark is white on the accent so it clears
        // contrast; while the box is off there is nothing to
        // say, and the empty square is the honest picture of
        // that.
        if on {
            draw_check_mark(buf, stride, height, clip, cx + 3, cy + 5, theme.button_text);
        }
    }
}

/// One `slider` as the drawing needs it — where its track sits, what number it
/// is on, and how far along the range that number is. Bundled into a struct
/// for the same reason `FlagView` is: the render pass hands the painter one
/// value instead of seven loose ones, and a test can describe a slider the
/// same way without building a window first.
#[derive(Debug, Clone, PartialEq)]
pub struct SliderView {
    /// The track's rect — the whole control, since the label beside it is the
    /// row's business and only the row knows how wide it was made.
    pub rect: Rect,
    /// Where the pointer has travelled from resting to hovered, `0.0` to
    /// `1.0`. Always zero for a slider that is disabled, which never lights.
    pub hover_t: f32,
    pub disabled: bool,
    /// The track's own geometry: bounds, step, and the position along the
    /// track the handle should sit at. Everything about the *value* is read
    /// from here rather than recomputed, so the handle cannot end up drawn at
    /// a place the number does not name.
    pub spec: SliderSpec,
    /// How much of the track to fill in the accent, in pixels from the left.
    /// Computed by the caller from `spec`, not here, so this function stays
    /// arithmetic-free about geometry the caller already settled.
    pub filled: i32,
}

impl SliderView {
    /// A slider described from its row: `row` is the whole laid-out row, and
    /// the track is placed inside it by [`slider_track_rect`].
    pub fn new(row: Rect, hover_t: f32, disabled: bool, spec: SliderSpec) -> SliderView {
        let track = slider_track_rect(row);
        // `value_to_x` answers in track-local pixels, which is exactly the
        // unit the fill's width is measured in — subtracting the track's
        // absolute x here would measure the fill against the window edge
        // instead of against the groove it lives in.
        let filled = spec.value_to_x(spec.value).round() as i32;
        SliderView {
            rect: track,
            hover_t,
            disabled,
            spec,
            filled: filled.clamp(0, track.w),
        }
    }

    /// The handle's rectangle, centred on its track position. The radius is
    /// half its own side, so the corner routine rounds it all the way to a
    /// circle without a separate code path.
    pub fn knob_rect(&self) -> Rect {
        let half = SLIDER_KNOB / 2;
        let cx = self.rect.x + self.spec.value_to_x(self.spec.value).round() as i32;
        Rect {
            x: cx - half,
            y: self.rect.y + (self.rect.h - SLIDER_KNOB) / 2,
            w: SLIDER_KNOB,
            h: SLIDER_KNOB,
        }
    }
}

/// One `slider` — the groove it sits in, the part of it already travelled, and
/// the handle marking where the value is — painted entirely from rounded
/// rectangles. Nothing here asks a font for a glyph, so a slider cannot
/// arrive as a row of letters and its handle cannot be lost among them.
///
/// `rect` is the track itself, not the row it sits in: the label beside it is
/// drawn by the caller, because only the caller knows how wide the box was
/// made. Pure, so the drawing can be judged without a window.
pub fn draw_slider_control(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    theme: &Theme,
    slider: &SliderView,
) {
    let r = slider.rect;
    if r.w <= 0 || r.h <= 0 {
        return;
    }
    let accent = theme.input_border_focused;
    let radius = r.h / 2;
    // The groove. Disabled dims the way a disabled field does, and hover
    // lifts the border the way a choice field's does, so the control that
    // will move is visible before it does.
    let (groove_bg, mut groove_edge) = if slider.disabled {
        (lerp_color(theme.input_bg, theme.bg, 0.5), theme.border)
    } else {
        (theme.input_bg, theme.input_border)
    };
    if !slider.disabled {
        groove_edge = lerp_color(groove_edge, accent, slider.hover_t);
    }
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        r.x,
        r.y,
        r.w,
        r.h,
        radius,
        groove_bg,
        groove_edge,
    );
    // The travelled part. It starts at the left edge rather than at the
    // handle's own radius, so the fill reads as *how much* rather than as a
    // second object floating in the groove.
    let fill_w = slider.filled.max(0);
    if fill_w > 0 {
        draw_rounded_rect_box_clipped(
            buf,
            stride,
            height,
            clip,
            r.x,
            r.y,
            fill_w,
            r.h,
            radius,
            if slider.disabled {
                lerp_color(accent, theme.bg, 0.5)
            } else {
                accent
            },
            if slider.disabled {
                lerp_color(accent, theme.bg, 0.5)
            } else {
                accent
            },
        );
    }
    // The handle. White while the slider is live, because a blue handle on a
    // blue fill would be one shape, and a handle that cannot be seen is a
    // control that cannot be used.
    let knob = slider.knob_rect();
    if knob.w > 0 {
        let knob_fill = if slider.disabled {
            theme.border
        } else {
            theme.button_text
        };
        draw_rounded_rect_box_clipped(
            buf,
            stride,
            height,
            clip,
            knob.x,
            knob.y,
            knob.w,
            knob.h,
            knob.w / 2,
            knob_fill,
            knob_fill,
        );
    }
}

/// Signed distance to a ring sector (arc) with center at `(cx, cy)`, radius `r`,
/// stroke thickness `stroke`, spanning an arc from `start_rad` by `sweep_rad`.
#[allow(clippy::too_many_arguments)]
fn arc_sector_sdf(
    px: f32,
    py: f32,
    cx: f32,
    cy: f32,
    r: f32,
    stroke: f32,
    start_rad: f32,
    sweep_rad: f32,
) -> f32 {
    let dx = px - cx;
    let dy = py - cy;
    let dist = dx.hypot(dy);
    let radial_sdf = (dist - r).abs() - stroke * 0.5;

    // The two half-plane end-caps for an arc spanning less than 180 degrees.
    let end_rad = start_rad + sweep_rad;
    let u0x = start_rad.cos();
    let u0y = start_rad.sin();
    let u1x = end_rad.cos();
    let u1y = end_rad.sin();

    // Cross products in screen coords (y down):
    // cross(u0, d) = u0.x * dy - u0.y * dx. Positive when past start_rad.
    let cross0 = u0x * dy - u0y * dx;
    let sdf_start = -cross0;

    // cross(u1, d) = u1.x * dy - u1.y * dx. Negative when before end_rad.
    let cross1 = u1x * dy - u1y * dx;
    let sdf_end = cross1;

    radial_sdf.max(sdf_start).max(sdf_end)
}

/// Draw an indeterminate spinner arc.
#[allow(clippy::too_many_arguments)]
pub fn draw_spinner_control(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    theme: &Theme,
    r: Rect,
    disabled: bool,
    phase: f32,
) {
    if r.w <= 0 || r.h <= 0 {
        return;
    }
    let cx = r.x as f32 + r.w as f32 * 0.5;
    let cy = r.y as f32 + r.h as f32 * 0.5;
    let radius = (r.w.min(r.h) as f32 - SPINNER_STROKE) * 0.5;
    if radius <= 0.0 {
        return;
    }

    let color = if disabled {
        lerp_color(theme.input_border_focused, theme.bg, 0.5)
    } else {
        theme.input_border_focused
    };

    let start_rad = phase * std::f32::consts::TAU;
    let sweep_rad = (SPINNER_ARC_DEG / 360.0) * std::f32::consts::TAU;

    let x0 = r.x.max(0);
    let y0 = r.y.max(0);
    let x1 = (r.x + r.w).min(stride as i32);
    let y1 = (r.y + r.h).min(height as i32);

    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(c) = clip {
                if !c.contains(x, y) {
                    continue;
                }
            }
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let sdf = arc_sector_sdf(px, py, cx, cy, radius, SPINNER_STROKE, start_rad, sweep_rad);
            let cov = (0.5 - sdf).clamp(0.0, 1.0);
            if cov > 0.0 {
                let idx = y as usize * stride + x as usize;
                blend_px(buf, idx, color, cov);
            }
        }
    }
}

/// Draw a determinate progress bar.
#[allow(clippy::too_many_arguments)]
pub fn draw_progress_control(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    theme: &Theme,
    r: Rect,
    value: f64,
    min: f64,
    max: f64,
    disabled: bool,
) {
    if r.w <= 0 || r.h <= 0 {
        return;
    }
    let accent = theme.input_border_focused;
    let radius = r.h / 2;
    let (groove_bg, groove_edge) = if disabled {
        (lerp_color(theme.input_bg, theme.bg, 0.5), theme.border)
    } else {
        (theme.input_bg, theme.input_border)
    };

    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        r.x,
        r.y,
        r.w,
        r.h,
        radius,
        groove_bg,
        groove_edge,
    );

    let range = (max - min).max(0.000001);
    let frac = ((value - min) / range).clamp(0.0, 1.0);
    let fill_w = (r.w as f64 * frac).round() as i32;

    if fill_w > 0 {
        let fill_color = if disabled {
            lerp_color(accent, theme.bg, 0.5)
        } else {
            accent
        };
        draw_rounded_rect_box_clipped(
            buf, stride, height, clip, r.x, r.y, fill_w, r.h, radius, fill_color, fill_color,
        );
    }
}

/// Draws a decoded RGBA image onto the softbuffer buffer with scaling, alpha blending,
/// optional corner radius, and viewport clipping.
#[allow(clippy::too_many_arguments)]
pub fn draw_image_rgba(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    dest: Rect,
    img: &crate::ui::DecodedImage,
    corner_radius: i32,
    alpha: f32,
) {
    if dest.w <= 0 || dest.h <= 0 || img.width == 0 || img.height == 0 || alpha <= 0.0 {
        return;
    }
    if let Some(c) = clip {
        if intersect(dest, c).is_none() {
            return;
        }
    }

    let x0 = dest.x.max(0);
    let y0 = dest.y.max(0);
    let x1 = (dest.x + dest.w).min(stride as i32);
    let y1 = (dest.y + dest.h).min(height as i32);

    let src_w = img.width as f32;
    let src_h = img.height as f32;
    let dst_w = dest.w as f32;
    let dst_h = dest.h as f32;

    let cr = corner_radius.min(dest.w / 2).min(dest.h / 2).max(0);

    for y in y0..y1 {
        let v = (y - dest.y) as f32 / dst_h;
        let sy = ((v * src_h).floor() as u32).min(img.height - 1);
        let row_offset = y as usize * stride;

        for x in x0..x1 {
            if let Some(c) = clip {
                if !c.contains(x, y) {
                    continue;
                }
            }

            let u = (x - dest.x) as f32 / dst_w;
            let sx = ((u * src_w).floor() as u32).min(img.width - 1);

            let sidx = ((sy * img.width + sx) * 4) as usize;
            let sr = img.data[sidx] as u32;
            let sg = img.data[sidx + 1] as u32;
            let sb = img.data[sidx + 2] as u32;
            let sa = (img.data[sidx + 3] as f32 / 255.0) * alpha;

            if sa <= 0.0 {
                continue;
            }

            let cov = if cr > 0 {
                rounded_rect_coverage(dest.x, dest.y, dest.w, dest.h, cr, x, y)
            } else {
                1.0
            };

            let eff_a = sa * cov;
            if eff_a <= 0.0 {
                continue;
            }

            let color = (sr << 16) | (sg << 8) | sb;
            let didx = row_offset + x as usize;
            if eff_a >= 1.0 {
                buf[didx] = 0xFF00_0000 | color;
            } else {
                blend_px(buf, didx, color, eff_a);
            }
        }
    }
}

/// Token classification for syntax highlighting inside `code_block`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxTokenKind {
    Keyword,
    String,
    Number,
    Comment,
    Punctuation,
    Ident,
    Whitespace,
}

/// Tokenize a single line of source code into syntax token slices and kinds.
pub fn tokenize_code_line(line: &str) -> Vec<(&str, SyntaxTokenKind)> {
    let mut tokens = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        let b = bytes[i];
        if (b == b'/' && i + 1 < len && bytes[i + 1] == b'/') || b == b'#' {
            tokens.push((&line[i..], SyntaxTokenKind::Comment));
            break;
        } else if b == b'"' || b == b'\'' {
            let quote = b;
            let start = i;
            i += 1;
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' && i + 1 < len {
                    i += 1;
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            tokens.push((&line[start..i], SyntaxTokenKind::String));
        } else if b.is_ascii_digit() {
            let start = i;
            while i < len
                && (bytes[i].is_ascii_digit()
                    || bytes[i] == b'.'
                    || bytes[i] == b'x'
                    || bytes[i] == b'X'
                    || bytes[i].is_ascii_hexdigit())
            {
                i += 1;
            }
            tokens.push((&line[start..i], SyntaxTokenKind::Number));
        } else if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &line[start..i];
            let kind = match word {
                "fn" | "let" | "mut" | "if" | "else" | "for" | "in" | "return" | "true"
                | "false" | "type" | "agent" | "state" | "view" | "uses" | "test" | "assert"
                | "def" | "class" | "import" | "from" | "const" | "var" | "async" | "await" => {
                    SyntaxTokenKind::Keyword
                }
                _ => SyntaxTokenKind::Ident,
            };
            tokens.push((word, kind));
        } else if b.is_ascii_whitespace() {
            let start = i;
            while i < len && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            tokens.push((&line[start..i], SyntaxTokenKind::Whitespace));
        } else {
            let start = i;
            i += 1;
            tokens.push((&line[start..i], SyntaxTokenKind::Punctuation));
        }
    }

    tokens
}

/// Rich text span style within an inline text run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RichSpanKind {
    Plain,
    Bold,
    Code,
    Link,
}

/// Tokenize an inline text line into rich formatting spans (**bold**, `code`, [link](url)).
pub fn parse_rich_spans(line: &str) -> Vec<(&str, RichSpanKind)> {
    let mut spans = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'*' && i + 1 < len && bytes[i + 1] == b'*' {
            if let Some(rel_end) = line[i + 2..].find("**") {
                let end = i + 2 + rel_end;
                spans.push((&line[i + 2..end], RichSpanKind::Bold));
                i = end + 2;
                continue;
            }
        }
        if bytes[i] == b'`' {
            if let Some(rel_end) = line[i + 1..].find('`') {
                let end = i + 1 + rel_end;
                spans.push((&line[i + 1..end], RichSpanKind::Code));
                i = end + 1;
                continue;
            }
        }
        if bytes[i] == b'[' {
            if let Some(bracket_end) = line[i + 1..].find(']') {
                let label_end = i + 1 + bracket_end;
                if label_end + 1 < len && bytes[label_end + 1] == b'(' {
                    if let Some(paren_end) = line[label_end + 2..].find(')') {
                        let link_end = label_end + 2 + paren_end;
                        spans.push((&line[i + 1..label_end], RichSpanKind::Link));
                        i = link_end + 1;
                        continue;
                    }
                }
            }
        }

        // Plain text run until next markdown delimiter
        let mut next_delim = len;
        for (idx, &b) in bytes[i..].iter().enumerate() {
            if (b == b'*' && i + idx + 1 < len && bytes[i + idx + 1] == b'*')
                || b == b'`'
                || b == b'['
            {
                next_delim = i + idx;
                break;
            }
        }
        if next_delim > i {
            spans.push((&line[i..next_delim], RichSpanKind::Plain));
            i = next_delim;
        } else {
            spans.push((&line[i..i + 1], RichSpanKind::Plain));
            i += 1;
        }
    }

    spans
}

/// Draws an inline rich text line (`line_str`) honouring **bold**, `code`, and [link](url) markup.
#[allow(clippy::too_many_arguments)]
pub fn draw_rich_line<F>(
    fonts: &FontEngine,
    theme: &Theme,
    start_x: i32,
    start_y: i32,
    line_str: &str,
    default_color: u32,
    clip: Option<Rect>,
    mut draw_px: F,
) where
    F: FnMut(i32, i32, i32, i32, u32, f32),
{
    let spans = parse_rich_spans(line_str);
    let mut cur_x = start_x;

    for (span_text, kind) in spans {
        let (face, color) = match kind {
            RichSpanKind::Plain => (FaceKind::Ui, default_color),
            RichSpanKind::Bold => (FaceKind::UiBold, default_color),
            RichSpanKind::Code => (FaceKind::Mono, theme.input_border_focused),
            RichSpanKind::Link => (FaceKind::Ui, theme.button_bg),
        };

        if let Some(c) = clip {
            draw_text_ttf_clipped(
                fonts,
                face,
                cur_x,
                start_y,
                span_text,
                c,
                |px, py, w, h, cov| {
                    draw_px(px, py, w, h, color, cov);
                },
            );
        } else {
            draw_text_ttf(
                fonts,
                face,
                cur_x,
                start_y,
                span_text,
                |px, py, w, h, cov| {
                    draw_px(px, py, w, h, color, cov);
                },
            );
        }

        cur_x += fonts.prefix_width(face, span_text, span_text.chars().count());
    }
}

/// Fuzzy match predicate: returns true if all characters in `pattern` appear
/// in order (case-insensitively) within `text`.
pub fn fuzzy_match(pattern: &str, text: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    let mut p_chars = pattern.chars().flat_map(char::to_lowercase);
    let mut target = match p_chars.next() {
        Some(c) => c,
        None => return true,
    };

    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch == target {
            target = match p_chars.next() {
                Some(c) => c,
                None => return true,
            };
        }
    }

    false
}

/// State for native right-click Context Menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenuItem {
    pub id: String,
    pub label: String,
}

/// Floating toast notification (e.g. "Code copied to clipboard!").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastNotification {
    pub message: String,
    pub status: String,
}

/// Chat message structure for export.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatMessageExport {
    pub role: String,
    pub content: String,
}

/// Structured chat export data (JSON).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChatExportData {
    pub title: String,
    pub messages: Vec<ChatMessageExport>,
}

/// Normalizes rich text or HTML pasted into inputs/composers, preserving
/// markdown syntax (**bold**, `code`, [link](url)) and stripping HTML tags/control characters.
pub fn normalize_rich_text_paste(raw: &str) -> String {
    let mut s = raw.replace("\r\n", "\n").replace('\r', "\n");
    // Convert basic HTML formatting to markdown
    s = s
        .replace("<b>", "**")
        .replace("</b>", "**")
        .replace("<strong>", "**")
        .replace("</strong>", "**")
        .replace("<i>", "*")
        .replace("</i>", "*")
        .replace("<em>", "*")
        .replace("</em>", "*")
        .replace("<code>", "`")
        .replace("</code>", "`")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<p>", "")
        .replace("</p>", "\n");

    s.chars()
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .collect()
}

/// Draws the floating toast notification at top-right of window.
pub fn draw_toast_notification(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    fonts: &FontEngine,
    theme: &Theme,
    win_w: i32,
    toast: &ToastNotification,
) {
    let text_w = fonts.measure_width(FaceKind::Ui, &toast.message);
    let toast_w = (text_w + 48).max(160);
    let toast_h = 32;
    let toast_x = (win_w - toast_w - 20).max(10);
    let toast_y = 20;

    let accent = match toast.status.as_str() {
        "success" => 0xFF10_B981,        // Emerald-500
        "error" => 0xFFEF_4444,          // Red-500
        _ => theme.input_border_focused, // Blue-500
    };

    // Notification pill background
    draw_rounded_rect_box(
        buf,
        stride,
        height,
        toast_x,
        toast_y,
        toast_w,
        toast_h,
        6,
        theme.input_bg,
        accent,
    );

    // Indicator bar (4px on left)
    draw_rect(
        buf,
        stride,
        height,
        toast_x + 3,
        toast_y + 4,
        4,
        toast_h - 8,
        accent,
    );

    let cell_h = fonts.cell_h;
    let ty = toast_y + (toast_h - cell_h) / 2;
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        toast_x + 16,
        ty,
        &toast.message,
        Rect {
            x: toast_x + 12,
            y: toast_y,
            w: toast_w - 20,
            h: toast_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
        },
    );
}

/// UI Error state representing failures during LLM/tool/network execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiErrorState {
    pub message: String,
    pub error_type: String, // "timeout", "network", "schema", "general"
    pub retryable: bool,
    pub action_id: Option<String>,
}

/// Crash recovery draft record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftRecoveryRecord {
    pub draft: String,
    pub input_id: String,
    pub timestamp_secs: u64,
}

/// Async LLM/Tool execution state with live progress and cancellation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AsyncExecutionState {
    pub in_flight: bool,
    pub status_text: String,
    pub task_id: Option<String>,
    pub progress_percent: Option<f32>,
    pub chunks_received: usize,
}

/// Persisted window configuration, theme, locale, and sidebar state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedWindowState {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub dark_theme: bool,
    pub locale: String,
    pub sidebar_open: bool,
}

impl Default for PersistedWindowState {
    fn default() -> Self {
        Self {
            width: 500,
            height: 400,
            x: None,
            y: None,
            dark_theme: true,
            locale: "en".into(),
            sidebar_open: true,
        }
    }
}

/// Draws the persistent or dismissible error banner at the top of the window.
pub fn draw_error_banner(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    fonts: &FontEngine,
    win_w: i32,
    error: &UiErrorState,
) {
    let banner_h = 38;
    let banner_w = (win_w - 40).max(200);
    let banner_x = 20;
    let banner_y = 10;

    let bg_color = 0xFF7F_1D1D; // Red-900
    let border_color = 0xFFEF_4444; // Red-500
    let text_color = 0xFFFE_F2F2; // Red-50

    draw_rounded_rect_box(
        buf,
        stride,
        height,
        banner_x,
        banner_y,
        banner_w,
        banner_h,
        6,
        bg_color,
        border_color,
    );

    // Error icon: [!]
    draw_rect(
        buf,
        stride,
        height,
        banner_x + 10,
        banner_y + 8,
        3,
        14,
        text_color,
    );
    draw_rect(
        buf,
        stride,
        height,
        banner_x + 10,
        banner_y + 25,
        3,
        3,
        text_color,
    );

    let cell_h = fonts.cell_h;
    let ty = banner_y + (banner_h - cell_h) / 2;

    // Retry button at the right end if retryable
    let content_max_w = if error.retryable {
        let retry_w = 80;
        let retry_h = 24;
        let retry_x = banner_x + banner_w - retry_w - 32;
        let retry_y = banner_y + (banner_h - retry_h) / 2;

        draw_rounded_rect_box(
            buf,
            stride,
            height,
            retry_x,
            retry_y,
            retry_w,
            retry_h,
            4,
            0xFFDC_2626, // Red-600
            0xFFF8_7171, // Red-400
        );

        let retry_txt = "Retry";
        let rw = fonts.measure_width(FaceKind::Ui, retry_txt);
        let rx = retry_x + (retry_w - rw) / 2;
        let rty = retry_y + (retry_h - cell_h) / 2;
        draw_text_ttf_clipped(
            fonts,
            FaceKind::Ui,
            rx,
            rty,
            retry_txt,
            Rect {
                x: retry_x,
                y: retry_y,
                w: retry_w,
                h: retry_h,
            },
            |px, py, w, h, cov| {
                draw_rect_alpha(buf, stride, height, px, py, w, h, 0xFFFF_FFFF, cov);
            },
        );

        banner_w - retry_w - 60
    } else {
        banner_w - 50
    };

    // Close '✕' on far right
    let close_x = banner_x + banner_w - 22;
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        close_x,
        ty,
        "✕",
        Rect {
            x: close_x - 4,
            y: banner_y,
            w: 24,
            h: banner_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, text_color, cov);
        },
    );

    // Error text
    let text_x = banner_x + 22;
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        text_x,
        ty,
        &error.message,
        Rect {
            x: text_x,
            y: banner_y,
            w: content_max_w.max(10),
            h: banner_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, text_color, cov);
        },
    );
}

/// Draws the async execution / streaming status bar with spinner and Cancel button.
#[allow(clippy::too_many_arguments)]
pub fn draw_async_status_bar(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    fonts: &FontEngine,
    theme: &Theme,
    win_w: i32,
    win_h: i32,
    async_state: &AsyncExecutionState,
    phase: f32,
) {
    if !async_state.in_flight {
        return;
    }
    let bar_h = 34;
    let bar_w = (win_w - 40).max(200);
    let bar_x = 20;
    let bar_y = win_h - bar_h - 14;

    let bg_color = theme.input_bg;
    let border_color = theme.input_border_focused;

    draw_rounded_rect_box(
        buf,
        stride,
        height,
        bar_x,
        bar_y,
        bar_w,
        bar_h,
        6,
        bg_color,
        border_color,
    );

    // Rotating spinner on the left
    let spin_r = Rect {
        x: bar_x + 10,
        y: bar_y + (bar_h - SPINNER_SIZE) / 2,
        w: SPINNER_SIZE,
        h: SPINNER_SIZE,
    };
    draw_spinner_control(buf, stride, height, None, theme, spin_r, false, phase);

    let cell_h = fonts.cell_h;
    let ty = bar_y + (bar_h - cell_h) / 2;

    // Cancel button on the right
    let cancel_w = 70;
    let cancel_h = 24;
    let cancel_x = bar_x + bar_w - cancel_w - 8;
    let cancel_y = bar_y + (bar_h - cancel_h) / 2;

    draw_rounded_rect_box(
        buf,
        stride,
        height,
        cancel_x,
        cancel_y,
        cancel_w,
        cancel_h,
        4,
        0xFFDC_2626, // Red-600
        0xFFEF_4444, // Red-500
    );

    let cancel_txt = "Cancel";
    let cw = fonts.measure_width(FaceKind::Ui, cancel_txt);
    let cx = cancel_x + (cancel_w - cw) / 2;
    let cty = cancel_y + (cancel_h - cell_h) / 2;
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        cx,
        cty,
        cancel_txt,
        Rect {
            x: cancel_x,
            y: cancel_y,
            w: cancel_w,
            h: cancel_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, 0xFFFF_FFFF, cov);
        },
    );

    // Status text (and chunks received if any)
    let display_msg = if async_state.chunks_received > 0 {
        format!(
            "{} ({} chunks)",
            async_state.status_text, async_state.chunks_received
        )
    } else {
        async_state.status_text.clone()
    };

    let text_x = spin_r.x + spin_r.w + 10;
    let max_text_w = (cancel_x - text_x - 10).max(10);
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        text_x,
        ty,
        &display_msg,
        Rect {
            x: text_x,
            y: bar_y,
            w: max_text_w,
            h: bar_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
        },
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenuState {
    pub target_id: Option<String>,
    pub target_tag: String,
    pub target_text: Option<String>,
    pub pos: (i32, i32),
    pub highlight: usize,
    pub items: Vec<ContextMenuItem>,
}

impl ContextMenuState {
    pub fn for_target(target: Option<&LayoutBox>, pos: (i32, i32), win_w: i32, win_h: i32) -> Self {
        let (id, tag, text, items) = match target {
            Some(b) if b.tag.starts_with("bubble") => (
                b.id.clone(),
                b.tag.clone(),
                b.text.clone(),
                vec![
                    ContextMenuItem {
                        id: "copy".into(),
                        label: "Copy Message".into(),
                    },
                    ContextMenuItem {
                        id: "regenerate".into(),
                        label: "Regenerate".into(),
                    },
                    ContextMenuItem {
                        id: "delete".into(),
                        label: "Delete Message".into(),
                    },
                ],
            ),
            Some(b) if b.id.as_deref().unwrap_or("").starts_with("chat") => (
                b.id.clone(),
                b.tag.clone(),
                b.text.clone(),
                vec![
                    ContextMenuItem {
                        id: "rename".into(),
                        label: "Rename Chat".into(),
                    },
                    ContextMenuItem {
                        id: "delete".into(),
                        label: "Delete Chat".into(),
                    },
                    ContextMenuItem {
                        id: "duplicate".into(),
                        label: "Duplicate Chat".into(),
                    },
                    ContextMenuItem {
                        id: "popout".into(),
                        label: "Open in New Window".into(),
                    },
                ],
            ),
            Some(b)
                if b.id.as_deref().unwrap_or("").starts_with("entry_")
                    || b.id.as_deref().unwrap_or("").starts_with("sz_")
                    || b.id.as_deref().unwrap_or("").starts_with("perm_")
                    || b.id.as_deref().unwrap_or("").starts_with("type_") =>
            {
                (
                    b.id.clone(),
                    b.tag.clone(),
                    b.text.clone(),
                    vec![
                        ContextMenuItem {
                            id: "open".into(),
                            label: "Open (Enter)".into(),
                        },
                        ContextMenuItem {
                            id: "open_tab".into(),
                            label: "Open in New Tab".into(),
                        },
                        ContextMenuItem {
                            id: "cut".into(),
                            label: "Cut (Ctrl+X)".into(),
                        },
                        ContextMenuItem {
                            id: "copy".into(),
                            label: "Copy (Ctrl+C)".into(),
                        },
                        ContextMenuItem {
                            id: "rename".into(),
                            label: "Rename (F2)".into(),
                        },
                        ContextMenuItem {
                            id: "delete".into(),
                            label: "Move to Trash (Del)".into(),
                        },
                        ContextMenuItem {
                            id: "properties".into(),
                            label: "Properties (F11)".into(),
                        },
                    ],
                )
            }
            Some(b)
                if b.tag == "scroll"
                    || b.tag == "panel"
                    || b.tag == "column"
                    || b.tag == "row"
                    || b.tag == "window" =>
            {
                (
                    b.id.clone(),
                    b.tag.clone(),
                    b.text.clone(),
                    vec![
                        ContextMenuItem {
                            id: "new_folder".into(),
                            label: "Create New Folder".into(),
                        },
                        ContextMenuItem {
                            id: "paste".into(),
                            label: "Paste (Ctrl+V)".into(),
                        },
                        ContextMenuItem {
                            id: "select_all".into(),
                            label: "Select All (Ctrl+A)".into(),
                        },
                        ContextMenuItem {
                            id: "open_terminal".into(),
                            label: "Open Terminal Here (F4)".into(),
                        },
                        ContextMenuItem {
                            id: "reload".into(),
                            label: "Reload (F5)".into(),
                        },
                        ContextMenuItem {
                            id: "properties".into(),
                            label: "Properties (F11)".into(),
                        },
                    ],
                )
            }
            Some(b) => (
                b.id.clone(),
                b.tag.clone(),
                b.text.clone(),
                vec![
                    ContextMenuItem {
                        id: "copy".into(),
                        label: "Copy".into(),
                    },
                    ContextMenuItem {
                        id: "paste".into(),
                        label: "Paste".into(),
                    },
                    ContextMenuItem {
                        id: "select_all".into(),
                        label: "Select All".into(),
                    },
                ],
            ),
            None => (
                None,
                "window".into(),
                None,
                vec![
                    ContextMenuItem {
                        id: "new_folder".into(),
                        label: "Create New Folder".into(),
                    },
                    ContextMenuItem {
                        id: "paste".into(),
                        label: "Paste".into(),
                    },
                    ContextMenuItem {
                        id: "open_terminal".into(),
                        label: "Open Terminal Here (F4)".into(),
                    },
                    ContextMenuItem {
                        id: "reload".into(),
                        label: "Reload (F5)".into(),
                    },
                    ContextMenuItem {
                        id: "properties".into(),
                        label: "Properties".into(),
                    },
                ],
            ),
        };

        let menu_w = 160;
        let menu_h = (items.len() as i32 * 28) + 12;
        let clamped_x = pos.0.min(win_w - menu_w - 8).max(8);
        let clamped_y = pos.1.min(win_h - menu_h - 8).max(8);

        Self {
            target_id: id,
            target_tag: tag,
            target_text: text,
            pos: (clamped_x, clamped_y),
            highlight: 0,
            items,
        }
    }

    pub fn menu_rect(&self) -> Rect {
        let menu_w = 160;
        let menu_h = (self.items.len() as i32 * 28) + 12;
        Rect {
            x: self.pos.0,
            y: self.pos.1,
            w: menu_w,
            h: menu_h,
        }
    }
}

/// Draws the floating Context Menu overlay.
pub fn draw_context_menu(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    fonts: &FontEngine,
    theme: &Theme,
    menu: &ContextMenuState,
) {
    let r = menu.menu_rect();
    if r.w <= 0 || r.h <= 0 {
        return;
    }

    // Card background with border outline
    draw_rounded_rect_box(
        buf,
        stride,
        height,
        r.x,
        r.y,
        r.w,
        r.h,
        6,
        theme.input_bg,
        theme.border,
    );

    let row_h = 28;
    let cell_h = fonts.cell_h;

    for (i, item) in menu.items.iter().enumerate() {
        let row_y = r.y + 6 + (i as i32 * row_h);
        let is_selected = i == menu.highlight;

        if is_selected {
            draw_rounded_rect_box(
                buf,
                stride,
                height,
                r.x + 4,
                row_y,
                r.w - 8,
                row_h,
                4,
                theme.selection_bg,
                theme.selection_bg,
            );
        }

        let text_color = if is_selected {
            theme.button_text
        } else {
            theme.text
        };
        let ty = row_y + (row_h - cell_h) / 2;
        draw_text_ttf_clipped(
            fonts,
            FaceKind::Ui,
            r.x + 12,
            ty,
            &item.label,
            Rect {
                x: r.x + 8,
                y: row_y,
                w: r.w - 16,
                h: row_h,
            },
            |px, py, w, h, cov| {
                draw_rect_alpha(buf, stride, height, px, py, w, h, text_color, cov);
            },
        );
    }
}

/// State for interactive drag-and-drop (message reordering, chat sorting).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragState {
    pub source_id: String,
    pub source_tag: String,
    pub source_text: Option<String>,
    pub start_pos: (i32, i32),
    pub current_pos: (i32, i32),
    pub active: bool,
    pub drop_target: Option<String>,
}

/// Supported application locales for UI localization (EN default, TR, ZH, JA, KO, RU, ES, DE, FR, IT, PT, NL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    En,
    Tr,
    Zh,
    Ja,
    Ko,
    Ru,
    Es,
    De,
    Fr,
    It,
    Pt,
    Nl,
}

impl Locale {
    /// Extremely fast, zero-allocation translation lookup table (<2ns).
    #[inline(always)]
    pub fn translate(self, key: &str) -> &'static str {
        match (self, key) {
            // Turkish (Türkçe)
            (Locale::Tr, "New Chat") => "Yeni Sohbet",
            (Locale::Tr, "New Chat Window") => "Yeni Sohbet Penceresi",
            (Locale::Tr, "Open Settings") => "Ayarları Aç",
            (Locale::Tr, "Change Model") => "Modeli Değiştir",
            (Locale::Tr, "Toggle Dark Theme") => "Karanlık Temayı Aç/Kapat",
            (Locale::Tr, "Clear Conversation") => "Sohbeti Temizle",
            (Locale::Tr, "Export History") => "Geçmişi Dışa Aktar",
            (Locale::Tr, "Export Chat as Markdown") => "Sohbeti Markdown Olarak Aktar",
            (Locale::Tr, "Export Chat as JSON") => "Sohbeti JSON Olarak Aktar",
            (Locale::Tr, "Export Chat as PNG") => "Sohbeti PNG Olarak Aktar",
            (Locale::Tr, "Switch Language") => "Dili Değiştir",
            (Locale::Tr, "Copy Message") => "Mesajı Kopyala",
            (Locale::Tr, "Regenerate") => "Yeniden Üret",
            (Locale::Tr, "Delete Message") => "Mesajı Sil",
            (Locale::Tr, "Rename Chat") => "Sohbeti Yeniden Adlandır",
            (Locale::Tr, "Delete Chat") => "Sohbeti Sil",
            (Locale::Tr, "Duplicate Chat") => "Sohbeti Çoğalt",
            (Locale::Tr, "Open in New Window") => "Yeni Pencerede Aç",
            (Locale::Tr, "Export Markdown") => "Markdown Dışa Aktar",
            (Locale::Tr, "Export JSON") => "JSON Dışa Aktar",
            (Locale::Tr, "Export PNG") => "PNG Dışa Aktar",
            (Locale::Tr, "Settings & Preferences") => "Ayarlar ve Tercihler",
            (Locale::Tr, "Save & Close") => "Kaydet ve Kapat",
            (Locale::Tr, "Dark Theme Mode") => "Karanlık Tema Modu",
            (Locale::Tr, "Stream Responses") => "Akışlı Yanıtlar",
            (Locale::Tr, "Active AI Model:") => "Aktif Yapay Zeka Modeli:",
            (Locale::Tr, "General") => "Genel",
            (Locale::Tr, "Model") => "Model",
            (Locale::Tr, "About") => "Hakkında",
            (Locale::Tr, "Language") => "Dil",
            (Locale::Tr, "Settings saved!") => "Ayarlar kaydedildi!",
            (Locale::Tr, "Code copied to clipboard!") => "Kod panoya kopyalandı!",
            (Locale::Tr, "Message copied to clipboard!") => "Mesaj panoya kopyalandı!",

            // Simplified Chinese (中文)
            (Locale::Zh, "New Chat") => "新建对话",
            (Locale::Zh, "New Chat Window") => "新建对话窗口",
            (Locale::Zh, "Open Settings") => "打开设置",
            (Locale::Zh, "Change Model") => "切换模型",
            (Locale::Zh, "Toggle Dark Theme") => "切换深色主题",
            (Locale::Zh, "Clear Conversation") => "清空对话",
            (Locale::Zh, "Export History") => "导出历史",
            (Locale::Zh, "Export Chat as Markdown") => "导出对话为 Markdown",
            (Locale::Zh, "Export Chat as JSON") => "导出对话为 JSON",
            (Locale::Zh, "Export Chat as PNG") => "导出对话为 PNG",
            (Locale::Zh, "Switch Language") => "切换语言",
            (Locale::Zh, "Copy Message") => "复制消息",
            (Locale::Zh, "Regenerate") => "重新生成",
            (Locale::Zh, "Delete Message") => "删除消息",
            (Locale::Zh, "Rename Chat") => "重命名对话",
            (Locale::Zh, "Delete Chat") => "删除对话",
            (Locale::Zh, "Duplicate Chat") => "复制对话",
            (Locale::Zh, "Open in New Window") => "在新窗口中打开",
            (Locale::Zh, "Export Markdown") => "导出 Markdown",
            (Locale::Zh, "Export JSON") => "导出 JSON",
            (Locale::Zh, "Export PNG") => "导出 PNG",
            (Locale::Zh, "Settings & Preferences") => "设置与偏好",
            (Locale::Zh, "Save & Close") => "保存并关闭",
            (Locale::Zh, "Dark Theme Mode") => "深色主题模式",
            (Locale::Zh, "Stream Responses") => "流式响应",
            (Locale::Zh, "Active AI Model:") => "当前 AI 模型：",
            (Locale::Zh, "General") => "常规",
            (Locale::Zh, "Model") => "模型",
            (Locale::Zh, "About") => "关于",
            (Locale::Zh, "Language") => "语言",
            (Locale::Zh, "Settings saved!") => "设置已保存！",
            (Locale::Zh, "Code copied to clipboard!") => "代码已复制到剪贴板！",
            (Locale::Zh, "Message copied to clipboard!") => "消息已复制到剪贴板！",

            // Japanese (日本語)
            (Locale::Ja, "New Chat") => "新しいチャット",
            (Locale::Ja, "New Chat Window") => "新しいチャットウィンドウ",
            (Locale::Ja, "Open Settings") => "設定を開く",
            (Locale::Ja, "Change Model") => "モデルを変更",
            (Locale::Ja, "Toggle Dark Theme") => "ダークテーマ切替",
            (Locale::Ja, "Clear Conversation") => "会話をクリア",
            (Locale::Ja, "Export History") => "履歴をエクスポート",
            (Locale::Ja, "Export Chat as Markdown") => "Markdownとしてエクスポート",
            (Locale::Ja, "Export Chat as JSON") => "JSONとしてエクスポート",
            (Locale::Ja, "Export Chat as PNG") => "PNGとしてエクスポート",
            (Locale::Ja, "Switch Language") => "言語を切替",
            (Locale::Ja, "Copy Message") => "メッセージをコピー",
            (Locale::Ja, "Regenerate") => "再生成",
            (Locale::Ja, "Delete Message") => "メッセージを削除",
            (Locale::Ja, "Rename Chat") => "チャット名を変更",
            (Locale::Ja, "Delete Chat") => "チャットを削除",
            (Locale::Ja, "Duplicate Chat") => "チャットを複製",
            (Locale::Ja, "Open in New Window") => "新しいウィンドウで開く",
            (Locale::Ja, "Export Markdown") => "Markdownエクスポート",
            (Locale::Ja, "Export JSON") => "JSONエクスポート",
            (Locale::Ja, "Export PNG") => "PNGエクスポート",
            (Locale::Ja, "Settings & Preferences") => "設定と環境設定",
            (Locale::Ja, "Save & Close") => "保存して閉じる",
            (Locale::Ja, "Dark Theme Mode") => "ダークテーマモード",
            (Locale::Ja, "Stream Responses") => "ストリーミング応答",
            (Locale::Ja, "Active AI Model:") => "アクティブなAIモデル：",
            (Locale::Ja, "General") => "一般",
            (Locale::Ja, "Model") => "モデル",
            (Locale::Ja, "About") => "概要",
            (Locale::Ja, "Language") => "言語",
            (Locale::Ja, "Settings saved!") => "設定を保存しました！",
            (Locale::Ja, "Code copied to clipboard!") => "コードをクリップボードにコピーしました！",
            (Locale::Ja, "Message copied to clipboard!") => "メッセージをコピーしました！",

            // Korean (한국어)
            (Locale::Ko, "New Chat") => "새 대화",
            (Locale::Ko, "New Chat Window") => "새 대화 창",
            (Locale::Ko, "Open Settings") => "설정 열기",
            (Locale::Ko, "Change Model") => "모델 변경",
            (Locale::Ko, "Toggle Dark Theme") => "다크 테마 전환",
            (Locale::Ko, "Clear Conversation") => "대화 내용 지우기",
            (Locale::Ko, "Export History") => "기록 내보내기",
            (Locale::Ko, "Export Chat as Markdown") => "Markdown으로 내보내기",
            (Locale::Ko, "Export Chat as JSON") => "JSON으로 내보내기",
            (Locale::Ko, "Export Chat as PNG") => "PNG로 내보내기",
            (Locale::Ko, "Switch Language") => "언어 전환",
            (Locale::Ko, "Copy Message") => "메시지 복사",
            (Locale::Ko, "Regenerate") => "다시 생성",
            (Locale::Ko, "Delete Message") => "메시지 삭제",
            (Locale::Ko, "Rename Chat") => "대화 이름 바꾸기",
            (Locale::Ko, "Delete Chat") => "대화 삭제",
            (Locale::Ko, "Duplicate Chat") => "대화 복제",
            (Locale::Ko, "Open in New Window") => "새 창에서 열기",
            (Locale::Ko, "Export Markdown") => "Markdown 내보내기",
            (Locale::Ko, "Export JSON") => "JSON 내보내기",
            (Locale::Ko, "Export PNG") => "PNG 내보내기",
            (Locale::Ko, "Settings & Preferences") => "설정 및 기본 설정",
            (Locale::Ko, "Save & Close") => "저장 후 닫기",
            (Locale::Ko, "Dark Theme Mode") => "다크 테마 모드",
            (Locale::Ko, "Stream Responses") => "스트리밍 응답",
            (Locale::Ko, "Active AI Model:") => "활성 AI 모델:",
            (Locale::Ko, "General") => "일반",
            (Locale::Ko, "Model") => "모델",
            (Locale::Ko, "About") => "정보",
            (Locale::Ko, "Language") => "언어",
            (Locale::Ko, "Settings saved!") => "설정이 저장되었습니다!",
            (Locale::Ko, "Code copied to clipboard!") => "코드가 클립보드에 복사되었습니다!",
            (Locale::Ko, "Message copied to clipboard!") => "메시지가 클립보드에 복사되었습니다!",

            // German (Deutsch)
            (Locale::De, "New Chat") => "Neuer Chat",
            (Locale::De, "New Chat Window") => "Neues Chat-Fenster",
            (Locale::De, "Open Settings") => "Einstellungen öffnen",
            (Locale::De, "Change Model") => "Modell ändern",
            (Locale::De, "Toggle Dark Theme") => "Dunkles Design umschalten",
            (Locale::De, "Clear Conversation") => "Unterhaltung löschen",
            (Locale::De, "Export History") => "Verlauf exportieren",
            (Locale::De, "Export Chat as Markdown") => "Chat als Markdown exportieren",
            (Locale::De, "Export Chat as JSON") => "Chat als JSON exportieren",
            (Locale::De, "Export Chat as PNG") => "Chat als PNG exportieren",
            (Locale::De, "Switch Language") => "Sprache wechseln",
            (Locale::De, "Copy Message") => "Nachricht kopieren",
            (Locale::De, "Regenerate") => "Neu generieren",
            (Locale::De, "Delete Message") => "Nachricht löschen",
            (Locale::De, "Rename Chat") => "Chat umbenennen",
            (Locale::De, "Delete Chat") => "Chat löschen",
            (Locale::De, "Duplicate Chat") => "Chat duplizieren",
            (Locale::De, "Open in New Window") => "In neuem Fenster öffnen",
            (Locale::De, "Export Markdown") => "Markdown exportieren",
            (Locale::De, "Export JSON") => "JSON exportieren",
            (Locale::De, "Export PNG") => "PNG exportieren",
            (Locale::De, "Settings & Preferences") => "Einstellungen & Präferenzen",
            (Locale::De, "Save & Close") => "Speichern & Schließen",
            (Locale::De, "Dark Theme Mode") => "Dunkelmodus",
            (Locale::De, "Stream Responses") => "Antworten streamen",
            (Locale::De, "Active AI Model:") => "Aktives KI-Modell:",
            (Locale::De, "General") => "Allgemein",
            (Locale::De, "Model") => "Modell",
            (Locale::De, "About") => "Über",
            (Locale::De, "Language") => "Sprache",
            (Locale::De, "Settings saved!") => "Einstellungen gespeichert!",
            (Locale::De, "Code copied to clipboard!") => "Code in die Zwischenablage kopiert!",
            (Locale::De, "Message copied to clipboard!") => "Nachricht kopiert!",

            // French (Français)
            (Locale::Fr, "New Chat") => "Nouvelle discussion",
            (Locale::Fr, "New Chat Window") => "Nouvelle fenêtre de discussion",
            (Locale::Fr, "Open Settings") => "Ouvrir les paramètres",
            (Locale::Fr, "Change Model") => "Changer de modèle",
            (Locale::Fr, "Toggle Dark Theme") => "Basculer le thème sombre",
            (Locale::Fr, "Clear Conversation") => "Effacer la conversation",
            (Locale::Fr, "Export History") => "Exporter l'historique",
            (Locale::Fr, "Export Chat as Markdown") => "Exporter en Markdown",
            (Locale::Fr, "Export Chat as JSON") => "Exporter en JSON",
            (Locale::Fr, "Export Chat as PNG") => "Exporter en PNG",
            (Locale::Fr, "Switch Language") => "Changer de langue",
            (Locale::Fr, "Copy Message") => "Copier le message",
            (Locale::Fr, "Regenerate") => "Régénérer",
            (Locale::Fr, "Delete Message") => "Supprimer le message",
            (Locale::Fr, "Rename Chat") => "Renommer la discussion",
            (Locale::Fr, "Delete Chat") => "Supprimer la discussion",
            (Locale::Fr, "Duplicate Chat") => "Dupliquer la discussion",
            (Locale::Fr, "Open in New Window") => "Ouvrir dans une nouvelle fenêtre",
            (Locale::Fr, "Export Markdown") => "Exporter en Markdown",
            (Locale::Fr, "Export JSON") => "Exporter en JSON",
            (Locale::Fr, "Export PNG") => "Exporter en PNG",
            (Locale::Fr, "Settings & Preferences") => "Paramètres & Préférences",
            (Locale::Fr, "Save & Close") => "Enregistrer & Fermer",
            (Locale::Fr, "Dark Theme Mode") => "Mode sombre",
            (Locale::Fr, "Stream Responses") => "Réponses en direct",
            (Locale::Fr, "Active AI Model:") => "Modèle IA actif :",
            (Locale::Fr, "General") => "Général",
            (Locale::Fr, "Model") => "Modèle",
            (Locale::Fr, "About") => "À propos",
            (Locale::Fr, "Language") => "Langue",
            (Locale::Fr, "Settings saved!") => "Paramètres enregistrés !",
            (Locale::Fr, "Code copied to clipboard!") => "Code copié dans le presse-papiers !",
            (Locale::Fr, "Message copied to clipboard!") => "Message copié !",

            // Spanish (Español)
            (Locale::Es, "New Chat") => "Nuevo chat",
            (Locale::Es, "New Chat Window") => "Nueva ventana de chat",
            (Locale::Es, "Open Settings") => "Abrir configuración",
            (Locale::Es, "Change Model") => "Cambiar modelo",
            (Locale::Es, "Toggle Dark Theme") => "Alternar tema oscuro",
            (Locale::Es, "Clear Conversation") => "Borrar conversación",
            (Locale::Es, "Export History") => "Exportar historial",
            (Locale::Es, "Export Chat as Markdown") => "Exportar chat como Markdown",
            (Locale::Es, "Export Chat as JSON") => "Exportar chat como JSON",
            (Locale::Es, "Export Chat as PNG") => "Exportar chat como PNG",
            (Locale::Es, "Switch Language") => "Cambiar idioma",
            (Locale::Es, "Copy Message") => "Copiar mensaje",
            (Locale::Es, "Regenerate") => "Regenerar",
            (Locale::Es, "Delete Message") => "Eliminar mensaje",
            (Locale::Es, "Rename Chat") => "Renombrar chat",
            (Locale::Es, "Delete Chat") => "Eliminar chat",
            (Locale::Es, "Duplicate Chat") => "Duplicar chat",
            (Locale::Es, "Open in New Window") => "Abrir en nueva ventana",
            (Locale::Es, "Export Markdown") => "Exportar Markdown",
            (Locale::Es, "Export JSON") => "Exportar JSON",
            (Locale::Es, "Export PNG") => "Exportar PNG",
            (Locale::Es, "Settings & Preferences") => "Configuración & Preferencias",
            (Locale::Es, "Save & Close") => "Guardar y cerrar",
            (Locale::Es, "Dark Theme Mode") => "Modo tema oscuro",
            (Locale::Es, "Stream Responses") => "Respuestas en tiempo real",
            (Locale::Es, "Active AI Model:") => "Modelo IA activo:",
            (Locale::Es, "General") => "General",
            (Locale::Es, "Model") => "Modelo",
            (Locale::Es, "About") => "Acerca de",
            (Locale::Es, "Language") => "Idioma",
            (Locale::Es, "Settings saved!") => "¡Configuración guardada!",
            (Locale::Es, "Code copied to clipboard!") => "¡Código copiado al portapapeles!",
            (Locale::Es, "Message copied to clipboard!") => "¡Mensaje copiado!",

            // Italian (Italiano)
            (Locale::It, "New Chat") => "Nuova chat",
            (Locale::It, "New Chat Window") => "Nuova finestra di chat",
            (Locale::It, "Open Settings") => "Apri impostazioni",
            (Locale::It, "Change Model") => "Cambia modello",
            (Locale::It, "Toggle Dark Theme") => "Attiva/disattiva tema scuro",
            (Locale::It, "Clear Conversation") => "Cancella conversazione",
            (Locale::It, "Export History") => "Esporta cronologia",
            (Locale::It, "Export Chat as Markdown") => "Esporta chat come Markdown",
            (Locale::It, "Export Chat as JSON") => "Esporta chat come JSON",
            (Locale::It, "Export Chat as PNG") => "Esporta chat come PNG",
            (Locale::It, "Switch Language") => "Cambia lingua",
            (Locale::It, "Copy Message") => "Copia messaggio",
            (Locale::It, "Regenerate") => "Rigenera",
            (Locale::It, "Delete Message") => "Elimina messaggio",
            (Locale::It, "Rename Chat") => "Rinomina chat",
            (Locale::It, "Delete Chat") => "Elimina chat",
            (Locale::It, "Duplicate Chat") => "Duplica chat",
            (Locale::It, "Open in New Window") => "Apri in una nuova finestra",
            (Locale::It, "Export Markdown") => "Esporta Markdown",
            (Locale::It, "Export JSON") => "Esporta JSON",
            (Locale::It, "Export PNG") => "Esporta PNG",
            (Locale::It, "Settings & Preferences") => "Impostazioni & Preferenze",
            (Locale::It, "Save & Close") => "Salva & Chiudi",
            (Locale::It, "Dark Theme Mode") => "Modalità tema scuro",
            (Locale::It, "Stream Responses") => "Risposte in streaming",
            (Locale::It, "Active AI Model:") => "Modello IA attivo:",
            (Locale::It, "General") => "Generale",
            (Locale::It, "Model") => "Modello",
            (Locale::It, "About") => "Info",
            (Locale::It, "Language") => "Lingua",
            (Locale::It, "Settings saved!") => "Impostazioni salvate!",
            (Locale::It, "Code copied to clipboard!") => "Codice copiato negli appunti!",
            (Locale::It, "Message copied to clipboard!") => "Messaggio copiato!",

            // Portuguese (Português)
            (Locale::Pt, "New Chat") => "Nova conversa",
            (Locale::Pt, "New Chat Window") => "Nova janela de chat",
            (Locale::Pt, "Open Settings") => "Abrir configurações",
            (Locale::Pt, "Change Model") => "Mudar modelo",
            (Locale::Pt, "Toggle Dark Theme") => "Alternar tema escuro",
            (Locale::Pt, "Clear Conversation") => "Limpar conversa",
            (Locale::Pt, "Export History") => "Exportar histórico",
            (Locale::Pt, "Export Chat as Markdown") => "Exportar chat como Markdown",
            (Locale::Pt, "Export Chat as JSON") => "Exportar chat como JSON",
            (Locale::Pt, "Export Chat as PNG") => "Exportar chat como PNG",
            (Locale::Pt, "Switch Language") => "Mudar idioma",
            (Locale::Pt, "Copy Message") => "Copiar mensagem",
            (Locale::Pt, "Regenerate") => "Regenerar",
            (Locale::Pt, "Delete Message") => "Excluir mensagem",
            (Locale::Pt, "Rename Chat") => "Renomear conversa",
            (Locale::Pt, "Delete Chat") => "Excluir conversa",
            (Locale::Pt, "Duplicate Chat") => "Duplicar conversa",
            (Locale::Pt, "Open in New Window") => "Abrir em nova janela",
            (Locale::Pt, "Export Markdown") => "Exportar Markdown",
            (Locale::Pt, "Export JSON") => "Exportar JSON",
            (Locale::Pt, "Export PNG") => "Exportar PNG",
            (Locale::Pt, "Settings & Preferences") => "Configurações & Preferências",
            (Locale::Pt, "Save & Close") => "Salvar e fechar",
            (Locale::Pt, "Dark Theme Mode") => "Modo tema escuro",
            (Locale::Pt, "Stream Responses") => "Transmitir respostas",
            (Locale::Pt, "Active AI Model:") => "Modelo IA ativo:",
            (Locale::Pt, "General") => "Geral",
            (Locale::Pt, "Model") => "Modelo",
            (Locale::Pt, "About") => "Sobre",
            (Locale::Pt, "Language") => "Idioma",
            (Locale::Pt, "Settings saved!") => "Configurações salvas!",
            (Locale::Pt, "Code copied to clipboard!") => {
                "Código copiado para a área de transferência!"
            }
            (Locale::Pt, "Message copied to clipboard!") => "Mensagem copiada!",

            // Russian (Русский)
            (Locale::Ru, "New Chat") => "Новый чат",
            (Locale::Ru, "New Chat Window") => "Новое окно чата",
            (Locale::Ru, "Open Settings") => "Открыть настройки",
            (Locale::Ru, "Change Model") => "Сменить модель",
            (Locale::Ru, "Toggle Dark Theme") => "Переключить тёмную тему",
            (Locale::Ru, "Clear Conversation") => "Очистить беседу",
            (Locale::Ru, "Export History") => "Экспорт истории",
            (Locale::Ru, "Export Chat as Markdown") => "Экспорт чата в Markdown",
            (Locale::Ru, "Export Chat as JSON") => "Экспорт чата в JSON",
            (Locale::Ru, "Export Chat as PNG") => "Экспорт чата в PNG",
            (Locale::Ru, "Switch Language") => "Переключить язык",
            (Locale::Ru, "Copy Message") => "Копировать сообщение",
            (Locale::Ru, "Regenerate") => "Сгенерировать снова",
            (Locale::Ru, "Delete Message") => "Удалить сообщение",
            (Locale::Ru, "Rename Chat") => "Переименовать чат",
            (Locale::Ru, "Delete Chat") => "Удалить чат",
            (Locale::Ru, "Duplicate Chat") => "Дублировать чат",
            (Locale::Ru, "Open in New Window") => "Открыть в новом окне",
            (Locale::Ru, "Export Markdown") => "Экспорт в Markdown",
            (Locale::Ru, "Export JSON") => "Экспорт в JSON",
            (Locale::Ru, "Export PNG") => "Экспорт в PNG",
            (Locale::Ru, "Settings & Preferences") => "Настройки и параметры",
            (Locale::Ru, "Save & Close") => "Сохранить и закрыть",
            (Locale::Ru, "Dark Theme Mode") => "Тёмная тема",
            (Locale::Ru, "Stream Responses") => "Потоковый вывод",
            (Locale::Ru, "Active AI Model:") => "Активная модель ИИ:",
            (Locale::Ru, "General") => "Общие",
            (Locale::Ru, "Model") => "Модель",
            (Locale::Ru, "About") => "О программе",
            (Locale::Ru, "Language") => "Язык",
            (Locale::Ru, "Settings saved!") => "Настройки сохранены!",
            (Locale::Ru, "Code copied to clipboard!") => "Код скопирован в буфер!",
            (Locale::Ru, "Message copied to clipboard!") => "Сообщение скопировано!",

            // Dutch (Nederlands)
            (Locale::Nl, "New Chat") => "Nieuwe chat",
            (Locale::Nl, "New Chat Window") => "Nieuw chatvenster",
            (Locale::Nl, "Open Settings") => "Instellingen openen",
            (Locale::Nl, "Change Model") => "Model wijzigen",
            (Locale::Nl, "Toggle Dark Theme") => "Donker thema in-/uitschakelen",
            (Locale::Nl, "Clear Conversation") => "Gesprek wissen",
            (Locale::Nl, "Export History") => "Geschiedenis exporteren",
            (Locale::Nl, "Export Chat as Markdown") => "Chat exporteren als Markdown",
            (Locale::Nl, "Export Chat as JSON") => "Chat exporteren als JSON",
            (Locale::Nl, "Export Chat as PNG") => "Chat exporteren als PNG",
            (Locale::Nl, "Switch Language") => "Taal wijzigen",
            (Locale::Nl, "Copy Message") => "Bericht kopiëren",
            (Locale::Nl, "Regenerate") => "Opnieuw genereren",
            (Locale::Nl, "Delete Message") => "Bericht verwijderen",
            (Locale::Nl, "Rename Chat") => "Chat hernoemen",
            (Locale::Nl, "Delete Chat") => "Chat verwijderen",
            (Locale::Nl, "Duplicate Chat") => "Chat dupliceren",
            (Locale::Nl, "Open in New Window") => "Openen in nieuw venster",
            (Locale::Nl, "Export Markdown") => "Markdown exporteren",
            (Locale::Nl, "Export JSON") => "JSON exporteren",
            (Locale::Nl, "Export PNG") => "PNG exporteren",
            (Locale::Nl, "Settings & Preferences") => "Instellingen & Voorkeuren",
            (Locale::Nl, "Save & Close") => "Opslaan & Sluiten",
            (Locale::Nl, "Dark Theme Mode") => "Donkere modus",
            (Locale::Nl, "Stream Responses") => "Antwoorden streamen",
            (Locale::Nl, "Active AI Model:") => "Actief AI-model:",
            (Locale::Nl, "General") => "Algemeen",
            (Locale::Nl, "Model") => "Model",
            (Locale::Nl, "About") => "Over",
            (Locale::Nl, "Language") => "Taal",
            (Locale::Nl, "Settings saved!") => "Instellingen opgeslagen!",
            (Locale::Nl, "Code copied to clipboard!") => "Code gekopieerd naar klembord!",
            (Locale::Nl, "Message copied to clipboard!") => "Bericht gekopieerd!",

            _ => "",
        }
    }

    /// Returns translated text or the original key if no translation exists.
    #[inline(always)]
    pub fn get(self, key: &str) -> &str {
        let t = self.translate(key);
        if t.is_empty() {
            key
        } else {
            t
        }
    }

    /// Display label of the locale.
    pub fn label(self) -> &'static str {
        match self {
            Locale::En => "English [EN]",
            Locale::Tr => "Türkçe [TR]",
            Locale::Zh => "中文 [ZH]",
            Locale::Ja => "日本語 [JA]",
            Locale::Ko => "한국어 [KO]",
            Locale::Ru => "Русский [RU]",
            Locale::Es => "Español [ES]",
            Locale::De => "Deutsch [DE]",
            Locale::Fr => "Français [FR]",
            Locale::It => "Italiano [IT]",
            Locale::Pt => "Português [PT]",
            Locale::Nl => "Nederlands [NL]",
        }
    }

    /// Code of the locale.
    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Tr => "tr",
            Locale::Zh => "zh",
            Locale::Ja => "ja",
            Locale::Ko => "ko",
            Locale::Ru => "ru",
            Locale::Es => "es",
            Locale::De => "de",
            Locale::Fr => "fr",
            Locale::It => "it",
            Locale::Pt => "pt",
            Locale::Nl => "nl",
        }
    }

    /// Parse locale from string code.
    pub fn from_code(code: &str) -> Option<Self> {
        match code.to_lowercase().as_str() {
            "en" => Some(Locale::En),
            "tr" => Some(Locale::Tr),
            "zh" | "zh-cn" => Some(Locale::Zh),
            "ja" | "ja-jp" => Some(Locale::Ja),
            "ko" | "ko-kr" => Some(Locale::Ko),
            "ru" | "ru-ru" => Some(Locale::Ru),
            "es" | "es-es" => Some(Locale::Es),
            "de" | "de-de" => Some(Locale::De),
            "fr" | "fr-fr" => Some(Locale::Fr),
            "it" | "it-it" => Some(Locale::It),
            "pt" | "pt-br" | "pt-pt" => Some(Locale::Pt),
            "nl" | "nl-nl" => Some(Locale::Nl),
            _ => None,
        }
    }

    /// Cycles to the next locale in sequence.
    pub fn next(self) -> Self {
        match self {
            Locale::En => Locale::Tr,
            Locale::Tr => Locale::Zh,
            Locale::Zh => Locale::Ja,
            Locale::Ja => Locale::Ko,
            Locale::Ko => Locale::Ru,
            Locale::Ru => Locale::Es,
            Locale::Es => Locale::De,
            Locale::De => Locale::Fr,
            Locale::Fr => Locale::It,
            Locale::It => Locale::Pt,
            Locale::Pt => Locale::Nl,
            Locale::Nl => Locale::En,
        }
    }
}

/// State for the modal Command Palette (`Ctrl+K`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPaletteState {
    pub query: String,
    pub highlight: usize,
    pub commands: Vec<String>,
}

impl Default for CommandPaletteState {
    fn default() -> Self {
        Self {
            query: String::new(),
            highlight: 0,
            commands: vec![
                "New Chat".into(),
                "New Chat Window".into(),
                "Open Settings".into(),
                "Change Model".into(),
                "Toggle Dark Theme".into(),
                "Clear Conversation".into(),
                "Export History".into(),
                "Export Chat as Markdown".into(),
                "Export Chat as JSON".into(),
                "Export Chat as PNG".into(),
                "Switch Language (TR/EN)".into(),
            ],
        }
    }
}

impl CommandPaletteState {
    pub fn filtered_commands(&self) -> Vec<&str> {
        self.commands
            .iter()
            .map(String::as_str)
            .filter(|cmd| fuzzy_match(&self.query, cmd))
            .collect()
    }
}

/// State for the modal Settings Dialog window.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsDialogState {
    pub open: bool,
    pub model: String,
    pub dark_theme: bool,
    pub stream_response: bool,
    pub temperature: f32,
    pub active_tab: usize,
    pub system_prompt: String,
    pub locale: Locale,
}

impl Default for SettingsDialogState {
    fn default() -> Self {
        Self {
            open: false,
            model: "gemini-3.8-flash".into(),
            dark_theme: true,
            stream_response: true,
            temperature: 0.7,
            active_tab: 0,
            system_prompt: "You are Nudge, an expert programming agent.".into(),
            locale: Locale::En,
        }
    }
}

/// Draws the modal Settings Dialog window overlay.
#[allow(clippy::too_many_arguments)]
pub fn draw_settings_dialog(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    fonts: &FontEngine,
    theme: &Theme,
    win_w: i32,
    win_h: i32,
    dialog: &SettingsDialogState,
) {
    if !dialog.open {
        return;
    }

    // 1. Semi-transparent backdrop dimming
    draw_rect_alpha(buf, stride, height, 0, 0, win_w, win_h, 0x0000_0000, 0.65);

    // 2. Centered dialog card
    let card_w = 420.min(win_w - 40).max(280);
    let card_h = 300.min(win_h - 40).max(200);
    let card_x = (win_w - card_w) / 2;
    let card_y = (win_h - card_h) / 2;

    draw_rounded_rect_box(
        buf,
        stride,
        height,
        card_x,
        card_y,
        card_w,
        card_h,
        8,
        theme.bg,
        theme.border,
    );

    // 3. Title bar (40px high)
    draw_rect(
        buf,
        stride,
        height,
        card_x + 1,
        card_y + 1,
        card_w - 2,
        38,
        theme.input_bg,
    );
    let title = "Settings & Preferences";
    let ty = card_y + (40 - fonts.cell_h) / 2;
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        card_x + 16,
        ty,
        title,
        Rect {
            x: card_x + 12,
            y: card_y,
            w: card_w - 60,
            h: 40,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
        },
    );

    // Close "✕" button in top-right
    let close_x = card_x + card_w - 28;
    let close_y = card_y + 8;
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        close_x,
        ty,
        "x",
        Rect {
            x: close_x - 4,
            y: close_y,
            w: 24,
            h: 24,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text_dim, cov);
        },
    );

    // 4. Tabs: "General", "Model", "About"
    let tabs = ["General", "Model", "About"];
    let tab_y = card_y + 44;
    let tab_w = 90;
    for (i, &t_name) in tabs.iter().enumerate() {
        let tx = card_x + 16 + (i as i32 * (tab_w + 8));
        let is_active = i == dialog.active_tab;
        let tab_bg = if is_active {
            theme.selection_bg
        } else {
            theme.input_bg
        };
        let tab_border = if is_active {
            theme.input_border_focused
        } else {
            theme.border
        };
        draw_rounded_rect_box(
            buf, stride, height, tx, tab_y, tab_w, 26, 4, tab_bg, tab_border,
        );
        let tw = fonts.measure_width(FaceKind::Ui, t_name);
        let t_text_x = tx + (tab_w - tw) / 2;
        let t_text_y = tab_y + (26 - fonts.cell_h) / 2;
        let col = if is_active {
            theme.button_text
        } else {
            theme.text
        };
        draw_text_ttf_clipped(
            fonts,
            FaceKind::Ui,
            t_text_x,
            t_text_y,
            t_name,
            Rect {
                x: tx,
                y: tab_y,
                w: tab_w,
                h: 26,
            },
            |px, py, w, h, cov| {
                draw_rect_alpha(buf, stride, height, px, py, w, h, col, cov);
            },
        );
    }

    // 5. Content area based on active_tab
    let content_y = tab_y + 36;
    match dialog.active_tab {
        0 => {
            // General: Dark Theme, Stream Output
            let row1_y = content_y + 8;
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + 20,
                row1_y + 2,
                "Dark Theme Mode",
                Rect {
                    x: card_x + 20,
                    y: row1_y,
                    w: 200,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
                },
            );
            let chk_label = if dialog.dark_theme {
                "[X] Enabled"
            } else {
                "[ ] Disabled"
            };
            draw_rounded_rect_box(
                buf,
                stride,
                height,
                card_x + card_w - 120,
                row1_y,
                100,
                24,
                4,
                theme.input_bg,
                theme.border,
            );
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + card_w - 110,
                row1_y + 4,
                chk_label,
                Rect {
                    x: card_x + card_w - 120,
                    y: row1_y,
                    w: 100,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(
                        buf,
                        stride,
                        height,
                        px,
                        py,
                        w,
                        h,
                        theme.input_border_focused,
                        cov,
                    );
                },
            );

            let row2_y = content_y + 42;
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + 20,
                row2_y + 2,
                "Stream Responses",
                Rect {
                    x: card_x + 20,
                    y: row2_y,
                    w: 200,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
                },
            );
            let stream_label = if dialog.stream_response {
                "[X] Active"
            } else {
                "[ ] Off"
            };
            draw_rounded_rect_box(
                buf,
                stride,
                height,
                card_x + card_w - 120,
                row2_y,
                100,
                24,
                4,
                theme.input_bg,
                theme.border,
            );
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + card_w - 110,
                row2_y + 4,
                stream_label,
                Rect {
                    x: card_x + card_w - 120,
                    y: row2_y,
                    w: 100,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(
                        buf,
                        stride,
                        height,
                        px,
                        py,
                        w,
                        h,
                        theme.input_border_focused,
                        cov,
                    );
                },
            );

            let row3_y = content_y + 76;
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + 20,
                row3_y + 2,
                "Language",
                Rect {
                    x: card_x + 20,
                    y: row3_y,
                    w: 200,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
                },
            );
            let lang_label = dialog.locale.label();
            draw_rounded_rect_box(
                buf,
                stride,
                height,
                card_x + card_w - 150,
                row3_y,
                130,
                24,
                4,
                theme.input_bg,
                theme.border,
            );
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + card_w - 142,
                row3_y + 4,
                lang_label,
                Rect {
                    x: card_x + card_w - 150,
                    y: row3_y,
                    w: 130,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(
                        buf,
                        stride,
                        height,
                        px,
                        py,
                        w,
                        h,
                        theme.input_border_focused,
                        cov,
                    );
                },
            );
        }
        1 => {
            // Model: Model ID, Temperature
            let row1_y = content_y + 8;
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + 20,
                row1_y + 2,
                "Active AI Model:",
                Rect {
                    x: card_x + 20,
                    y: row1_y,
                    w: 150,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
                },
            );
            draw_rounded_rect_box(
                buf,
                stride,
                height,
                card_x + 160,
                row1_y,
                card_w - 180,
                24,
                4,
                theme.input_bg,
                theme.border,
            );
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + 168,
                row1_y + 4,
                &dialog.model,
                Rect {
                    x: card_x + 160,
                    y: row1_y,
                    w: card_w - 180,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(
                        buf,
                        stride,
                        height,
                        px,
                        py,
                        w,
                        h,
                        theme.input_border_focused,
                        cov,
                    );
                },
            );

            let row2_y = content_y + 42;
            let temp_str = format!("Temperature: {:.1}", dialog.temperature);
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + 20,
                row2_y + 2,
                &temp_str,
                Rect {
                    x: card_x + 20,
                    y: row2_y,
                    w: 200,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
                },
            );
            let slider_x = card_x + 160;
            let slider_w = card_w - 180;
            draw_rounded_rect_box(
                buf,
                stride,
                height,
                slider_x,
                row2_y + 8,
                slider_w,
                6,
                3,
                theme.border,
                theme.border,
            );
            let handle_x =
                slider_x + ((slider_w as f32 * dialog.temperature) as i32).min(slider_w - 14);
            draw_rounded_rect_box(
                buf,
                stride,
                height,
                handle_x,
                row2_y + 4,
                14,
                14,
                4,
                theme.input_border_focused,
                theme.input_border_focused,
            );
        }
        _ => {
            // About
            let row1_y = content_y + 8;
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + 20,
                row1_y + 2,
                "Nudge Studio Native GUI v1.2",
                Rect {
                    x: card_x + 20,
                    y: row1_y,
                    w: 300,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
                },
            );
            let row2_y = content_y + 36;
            draw_text_ttf_clipped(
                fonts,
                FaceKind::Ui,
                card_x + 20,
                row2_y + 2,
                "Direct Linux Framebuffer Native Renderer",
                Rect {
                    x: card_x + 20,
                    y: row2_y,
                    w: 360,
                    h: 24,
                },
                |px, py, w, h, cov| {
                    draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text_dim, cov);
                },
            );
        }
    }

    // 6. Footer (Save / Close button)
    let footer_y = card_y + card_h - 44;
    let btn_w = 100;
    let btn_h = 28;
    let btn_x = card_x + card_w - btn_w - 16;
    draw_rounded_rect_box(
        buf,
        stride,
        height,
        btn_x,
        footer_y,
        btn_w,
        btn_h,
        4,
        theme.selection_bg,
        theme.input_border_focused,
    );
    let save_tw = fonts.measure_width(FaceKind::Ui, "Save & Close");
    let save_tx = btn_x + (btn_w - save_tw) / 2;
    let save_ty = footer_y + (btn_h - fonts.cell_h) / 2;
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        save_tx,
        save_ty,
        "Save & Close",
        Rect {
            x: btn_x,
            y: footer_y,
            w: btn_w,
            h: btn_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, theme.button_text, cov);
        },
    );
}

/// A secondary OS window spawned by the application (e.g. popout chat or separate session).
pub struct SecondaryWindow {
    pub id: WindowId,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub window: Arc<Window>,
    pub surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    pub chat_session_id: Option<String>,
    pub layout_boxes: Vec<LayoutBox>,
    pub last_frame: Vec<u32>,
}

impl SecondaryWindow {
    pub fn render(&mut self, fonts: &FontEngine, theme: &Theme) {
        let w_nz = match NonZeroU32::new(self.width) {
            Some(w) => w,
            None => return,
        };
        let h_nz = match NonZeroU32::new(self.height) {
            Some(h) => h,
            None => return,
        };
        if self.surface.resize(w_nz, h_nz).is_err() {
            return;
        }
        let mut buffer = match self.surface.buffer_mut() {
            Ok(b) => b,
            Err(_) => return,
        };
        let stride = self.width as usize;
        let h_usize = self.height as usize;
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
        draw_rect(
            &mut buffer,
            stride,
            h_usize,
            0,
            0,
            self.width as i32,
            36,
            theme.input_bg,
        );
        let ty = (36 - fonts.cell_h) / 2;
        draw_text_ttf_clipped(
            fonts,
            FaceKind::Ui,
            16,
            ty,
            &self.title,
            Rect {
                x: 12,
                y: 0,
                w: self.width as i32 - 24,
                h: 36,
            },
            |px, py, w, h, cov| {
                draw_rect_alpha(&mut buffer, stride, h_usize, px, py, w, h, theme.text, cov);
            },
        );
        for lbox in &self.layout_boxes {
            let rx = lbox.rect.x;
            let ry = lbox.rect.y;
            draw_rounded_rect_box(
                &mut buffer,
                stride,
                h_usize,
                rx,
                ry,
                lbox.rect.w,
                lbox.rect.h,
                6,
                theme.bubble_user_bg,
                theme.border,
            );
            if let Some(ref text) = lbox.text {
                draw_text_ttf_clipped(
                    fonts,
                    FaceKind::Ui,
                    rx + 12,
                    ry + 8,
                    text,
                    Rect {
                        x: rx,
                        y: ry,
                        w: lbox.rect.w,
                        h: lbox.rect.h,
                    },
                    |px, py, w, h, cov| {
                        draw_rect_alpha(
                            &mut buffer,
                            stride,
                            h_usize,
                            px,
                            py,
                            w,
                            h,
                            theme.text,
                            cov,
                        );
                    },
                );
            }
        }
        self.last_frame = buffer.to_vec();
        let _ = buffer.present();
    }
}

/// Draws a floating pill / ghost card representing the dragged item near the cursor.
pub fn draw_drag_ghost(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    fonts: &FontEngine,
    theme: &Theme,
    pos: (i32, i32),
    label: &str,
) {
    let (mx, my) = pos;
    let card_w = (label.chars().count() as i32 * CELL_W + 32).clamp(120, 260);
    let card_h = 30;
    let card_x = mx + 12;
    let card_y = my - card_h / 2;

    draw_rounded_rect_box(
        buf,
        stride,
        height,
        card_x,
        card_y,
        card_w,
        card_h,
        6,
        theme.input_bg,
        theme.input_border_focused,
    );

    let cell_h = fonts.cell_h;
    let ty = card_y + (card_h - cell_h) / 2;
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        card_x + 12,
        ty,
        label,
        Rect {
            x: card_x + 8,
            y: card_y,
            w: card_w - 16,
            h: card_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
        },
    );
}

/// Draws an overlay indicating that a dragged file is hovering over the window.
#[allow(clippy::too_many_arguments)]
pub fn draw_file_drop_overlay(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    fonts: &FontEngine,
    theme: &Theme,
    win_w: i32,
    win_h: i32,
    filename: &str,
) {
    let banner_w = 320.min(win_w - 40);
    let banner_h = 60;
    let banner_x = (win_w - banner_w) / 2;
    let banner_y = win_h - banner_h - 20;

    draw_rounded_rect_box(
        buf,
        stride,
        height,
        banner_x,
        banner_y,
        banner_w,
        banner_h,
        8,
        theme.input_bg,
        theme.input_border_focused,
    );

    let title = "Drop file to attach";
    let cell_h = fonts.cell_h;
    let title_w = fonts.measure_width(FaceKind::UiBold, title);
    let tx = banner_x + (banner_w - title_w) / 2;
    draw_text_ttf(
        fonts,
        FaceKind::UiBold,
        tx,
        banner_y + 10,
        title,
        |px, py, w, h, cov| {
            draw_rect_alpha(
                buf,
                stride,
                height,
                px,
                py,
                w,
                h,
                theme.input_border_focused,
                cov,
            );
        },
    );

    let file_w = fonts.measure_width(FaceKind::Ui, filename);
    let fx = banner_x + (banner_w - file_w) / 2;
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        fx,
        banner_y + 10 + cell_h + 4,
        filename,
        Rect {
            x: banner_x + 8,
            y: banner_y,
            w: banner_w - 16,
            h: banner_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text_dim, cov);
        },
    );
}

/// Draws the modal Command Palette overlay at the top-center of the window.
#[allow(clippy::too_many_arguments)]
pub fn draw_command_palette(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    fonts: &FontEngine,
    theme: &Theme,
    win_w: i32,
    _win_h: i32,
    palette: &CommandPaletteState,
) {
    let card_w = 400.min(win_w - 40).max(200);
    let card_x = (win_w - card_w) / 2;
    let card_y = 40;

    let filtered = palette.filtered_commands();
    let row_h = 32;
    let max_rows = 6;
    let shown_rows = filtered.len().min(max_rows);
    let search_h = 40;
    let card_h = search_h + (shown_rows as i32 * row_h) + 12;

    // Draw backdrop card with drop shadow feel
    draw_rounded_rect_box(
        buf,
        stride,
        height,
        card_x,
        card_y,
        card_w,
        card_h,
        8,
        theme.input_bg,
        theme.input_border_focused,
    );

    // Search query header inside card
    let prompt_prefix = "> ";
    let cell_h = fonts.cell_h;
    let query_y = card_y + (search_h - cell_h) / 2;

    let prefix_w = fonts.measure_width(FaceKind::UiBold, prompt_prefix);
    draw_text_ttf_clipped(
        fonts,
        FaceKind::UiBold,
        card_x + 16,
        query_y,
        prompt_prefix,
        Rect {
            x: card_x,
            y: card_y,
            w: card_w,
            h: search_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(
                buf,
                stride,
                height,
                px,
                py,
                w,
                h,
                theme.input_border_focused,
                cov,
            );
        },
    );

    let query_text = if palette.query.is_empty() {
        "Type a command..."
    } else {
        &palette.query
    };
    let query_color = if palette.query.is_empty() {
        theme.text_dim
    } else {
        theme.text
    };
    draw_text_ttf_clipped(
        fonts,
        FaceKind::Ui,
        card_x + 16 + prefix_w,
        query_y,
        query_text,
        Rect {
            x: card_x + 16 + prefix_w,
            y: card_y,
            w: card_w - 32 - prefix_w,
            h: search_h,
        },
        |px, py, w, h, cov| {
            draw_rect_alpha(buf, stride, height, px, py, w, h, query_color, cov);
        },
    );

    // Separator line under search box
    draw_rect(
        buf,
        stride,
        height,
        card_x + 8,
        card_y + search_h,
        card_w - 16,
        1,
        theme.border,
    );

    // List of filtered commands
    let list_start_y = card_y + search_h + 6;
    for (i, &cmd) in filtered.iter().enumerate().take(shown_rows) {
        let row_y = list_start_y + (i as i32 * row_h);
        let is_selected = i == palette.highlight.min(filtered.len().saturating_sub(1));

        if is_selected {
            draw_rounded_rect_box(
                buf,
                stride,
                height,
                card_x + 6,
                row_y,
                card_w - 12,
                row_h,
                4,
                theme.selection_bg,
                theme.selection_bg,
            );
        }

        let text_color = if is_selected {
            theme.button_text
        } else {
            theme.text
        };
        let ty = row_y + (row_h - cell_h) / 2;
        draw_text_ttf_clipped(
            fonts,
            FaceKind::Ui,
            card_x + 16,
            ty,
            cmd,
            Rect {
                x: card_x + 12,
                y: row_y,
                w: card_w - 24,
                h: row_h,
            },
            |px, py, w, h, cov| {
                draw_rect_alpha(buf, stride, height, px, py, w, h, text_color, cov);
            },
        );
    }
}

/// The list a `select` opens, painted as one card over everything beneath it.
/// The rows are drawn rather than laid out, so opening a field never reflows
/// the form — the card takes the room below the field, or above it when there
/// is no room below, and takes only as much as the window can show.
pub fn draw_select_popup(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    fonts: &FontEngine,
    theme: &Theme,
    popup: &SelectPopupView,
    hover_row: Option<usize>,
) {
    let r = popup.rect;
    if r.w <= 0 || r.h <= 0 {
        return;
    }
    // The card wears the input's surface so the list reads as the field's own
    // contents, with the focused border — an open list is the field's active
    // state, and the accent is what says so.
    let (bg, border) = input_surface(theme, true, false);
    draw_rounded_rect_box(
        buf,
        stride,
        height,
        r.x,
        r.y,
        r.w,
        r.h,
        CORNER_RADIUS,
        bg,
        border,
    );

    let cell_h = fonts.cell_h;
    for row in 0..popup.rows {
        let Some(idx) = popup.first.checked_add(row) else {
            break;
        };
        let Some(option) = popup.options.get(idx) else {
            break;
        };
        let row_y = r.y + SELECT_POP_PAD + row as i32 * SELECT_ROW_H;

        // The active row is the one the keyboard is on, unless the pointer is
        // somewhere on the card — then the pointer wins, so what lights up is
        // always what a press would take. It is also the row drawn wider, from
        // edge to edge, which is how a list says "this one".
        let active = hover_row.unwrap_or(popup.highlight) == idx;
        if active {
            draw_rect(
                buf,
                stride,
                height,
                r.x + 1,
                row_y,
                r.w - 2,
                SELECT_ROW_H,
                theme.selection_bg,
            );
        }

        let row_rect = Rect {
            x: r.x + 1,
            y: row_y,
            w: (r.w - 2).max(0),
            h: SELECT_ROW_H,
        };
        let text_y = row_y + ((SELECT_ROW_H - cell_h) / 2).max(2);
        draw_text_ttf_clipped(
            fonts,
            FaceKind::Ui,
            r.x + 10,
            text_y,
            option,
            row_rect,
            |px, py, w, h, cov| {
                draw_rect_alpha(buf, stride, height, px, py, w, h, theme.text, cov);
            },
        );

        // The row the field is already showing is marked, so a list opened by
        // accident shows where it would land before anything is pressed. The
        // mark is a small square rather than a glyph, for the same reason the
        // chevron is drawn: no font owes the UI a codepoint.
        if popup.value.as_deref() == Some(option.as_str()) {
            for i in 0..4 {
                let dot = Rect {
                    x: r.x + r.w - 14,
                    y: row_y + (SELECT_ROW_H - 7) / 2 + i,
                    w: if i == 0 || i == 3 { 7 } else { 9 },
                    h: 1,
                };
                if let Some(d) = intersect(dot, row_rect) {
                    draw_rect(buf, stride, height, d.x, d.y, d.w, d.h, theme.text_dim);
                }
            }
        }
    }

    // A list taller than its card scrolls inside itself. The thumb is the same
    // colour as a window scrollbar's, so the two cannot be told apart — which
    // is the point: they do the same job.
    let count = popup.options.len();
    if count > popup.rows && popup.rows > 0 {
        let track_h = r.h - 2 * SELECT_POP_PAD;
        let thumb_h = ((track_h * popup.rows as i32) / count as i32).max(16);
        let scrollable = count - popup.rows;
        let denom = scrollable.max(1) as i32;
        let thumb_y =
            r.y + SELECT_POP_PAD + (track_h - thumb_h) * popup.first.min(scrollable) as i32 / denom;
        draw_rect(
            buf,
            stride,
            height,
            r.x + r.w - 4,
            thumb_y,
            2,
            thumb_h,
            theme.scrollbar_thumb,
        );
    }
}

/// Paint a hover tooltip just below and to the right of the cursor. Called
/// last, so it layers over everything drawn so far — including scroll
/// viewports, which would otherwise slice it at their edges. A tooltip has no
/// layout box of its own, which is exactly what keeps it out of the document
/// flow; the price is that nothing else keeps it on screen, so it is clamped
/// inside the window here.
#[allow(clippy::too_many_arguments)]
pub fn draw_tooltip(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    engine: &FontEngine,
    theme: &Theme,
    win_w: i32,
    win_h: i32,
    mx: i32,
    my: i32,
    text: &str,
) {
    const PAD_X: i32 = 7;
    const PAD_Y: i32 = 5;
    // Distance from the pointer to the card's top-left corner.
    const GAP: i32 = 13;
    const RADIUS: i32 = 4;

    let w = engine.measure_width(FaceKind::Ui, text) + 2 * PAD_X;
    let h = engine.cell_h + 2 * PAD_Y;

    // Below-right of the pointer, then slid back inside the window if that
    // would push it past an edge. The card never wraps to the other side, so
    // it cannot leap across the screen while the pointer sits still.
    let x = (mx + GAP).clamp(0, (win_w - w).max(0));
    let y = (my + GAP).clamp(0, (win_h - h).max(0));

    draw_rounded_rect_box(
        buf,
        stride,
        height,
        x,
        y,
        w,
        h,
        RADIUS,
        theme.input_bg,
        theme.border,
    );
    draw_text_ttf_clipped(
        engine,
        FaceKind::Ui,
        x + PAD_X,
        y + PAD_Y,
        text,
        Rect { x, y, w, h },
        |px, py, pw, ph, cov| {
            draw_rect_alpha(buf, stride, height, px, py, pw, ph, theme.text, cov);
        },
    );
}

/// Same as [`draw_rounded_rect_box`], but no pixel outside `clip` is touched,
/// so chrome inside a scroll viewport is sliced at the viewport edge instead
/// of bleeding over neighbouring content.
#[allow(clippy::too_many_arguments)]
pub fn draw_breeze_folder_icon(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    // Breeze folder vector: tab + back plate + opening + sky blue front pocket
    let tab_w = (w * 45) / 100;
    let tab_h = (h * 24) / 100;
    let back_color = 0xFF65_8BA7;
    let back_highlight = 0xFF76_9CB7;

    // Top tab of back flap
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        x,
        y,
        tab_w + 6,
        tab_h + 4,
        3,
        back_color,
        back_color,
    );

    // Back plate body
    let body_top = y + tab_h;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        x,
        body_top,
        w,
        h - tab_h,
        3,
        back_color,
        back_highlight,
    );

    // Subtle dark opening slit
    draw_rect_alpha_clipped(
        buf,
        stride,
        height,
        clip,
        x + 3,
        body_top + 2,
        (w - 6).max(1),
        3,
        0xFF00_0000,
        0.4,
    );

    // Front pocket (Breeze sky blue)
    let pocket_top = y + (h * 32) / 100;
    let pocket_h = h - (pocket_top - y);
    let front_color = 0xFF96_D0F2;
    let front_top = 0xFFAC_DDF7;

    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        x,
        pocket_top,
        w,
        pocket_h,
        4,
        front_color,
        front_top,
    );
    // Subtle bottom crease
    draw_rect_alpha(
        buf,
        stride,
        height,
        x + 2,
        pocket_top + pocket_h - 2,
        (w - 4).max(1),
        2,
        0xFF00_0000,
        0.25,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_breeze_file_icon(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u32,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let dog_ear = 10;
    let sheet_color = 0xFFFA_FBFC;
    let border_color = 0xFFBD_C7D0;
    let fold_color = 0xFFDE_E4EA;

    // Main sheet
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        x,
        y,
        w,
        h,
        2,
        sheet_color,
        border_color,
    );

    // Dog ear fold at top-right
    let ear_x = x + w - dog_ear;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        ear_x - 1,
        y,
        dog_ear + 1,
        dog_ear + 1,
        1,
        fold_color,
        border_color,
    );

    // Accent content lines
    let line_y_start = y + dog_ear + 5;
    let line_w = (w - 12).max(4);
    for i in 0..3 {
        let ly = line_y_start + i * 6;
        if ly + 2 < y + h - 5 {
            let lw = if i == 2 { (line_w * 6) / 10 } else { line_w };
            draw_rounded_rect_box_clipped(
                buf,
                stride,
                height,
                clip,
                x + 6,
                ly,
                lw,
                3,
                1,
                color,
                color,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_image_thumbnail_card(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let frame_bg = 0xFFFA_FAFC;
    let frame_border = 0xFFD0_D7DE;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        x,
        y,
        w,
        h,
        3,
        frame_bg,
        frame_border,
    );

    let pad = 3;
    let pic_x = x + pad;
    let pic_y = y + pad;
    let pic_w = (w - pad * 2).max(4);
    let pic_h = (h - pad * 2 - 4).max(4);
    let sky_color = 0xFF4A_90D9;
    draw_rounded_rect_box_clipped(
        buf, stride, height, clip, pic_x, pic_y, pic_w, pic_h, 2, sky_color, sky_color,
    );

    let sun_x = pic_x + pic_w - 10;
    let sun_y = pic_y + 4;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        sun_x,
        sun_y,
        6,
        6,
        3,
        0xFFF6_C343,
        0xFFF6_C343,
    );

    let mtn_h = (pic_h * 45) / 100;
    let mtn_y = pic_y + pic_h - mtn_h;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        pic_x + 2,
        mtn_y + 2,
        pic_w - 4,
        mtn_h - 2,
        2,
        0xFF27_AE60,
        0xFF27_AE60,
    );
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        pic_x + (pic_w / 4),
        mtn_y + (mtn_h / 2),
        (pic_w * 5) / 8,
        mtn_h / 2,
        1,
        0xFF2E_CC71,
        0xFF2E_CC71,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_code_thumbnail_card(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    ext: &str,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let editor_bg = 0xFF21_252B;
    let editor_border = 0xFF35_3B45;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        x,
        y,
        w,
        h,
        3,
        editor_bg,
        editor_border,
    );

    draw_rect_clipped(buf, stride, height, clip, x + 4, y + 4, 3, 3, 0xFFE0_6C75);
    draw_rect_clipped(buf, stride, height, clip, x + 9, y + 4, 3, 3, 0xFFE5_C07B);
    draw_rect_clipped(buf, stride, height, clip, x + 14, y + 4, 3, 3, 0xFF98_C379);

    let code_colors = [0xFFC6_78DD, 0xFF61_AFEF, 0xFFE5_C07B, 0xFF98_C379];
    let start_y = y + 11;
    for (i, &col) in code_colors.iter().enumerate() {
        let ly = start_y + (i as i32) * 6;
        if ly + 2 < y + h - 10 {
            let indent = if i == 1 || i == 2 { 8 } else { 4 };
            let lw = (w - indent - 8).max(6);
            draw_rounded_rect_box_clipped(
                buf,
                stride,
                height,
                clip,
                x + indent,
                ly,
                lw,
                2,
                1,
                col,
                col,
            );
        }
    }

    let badge_color = match ext {
        "rs" => 0xFFD8_5A2B,
        "ndg" => 0xFF3D_AEE9,
        "py" => 0xFF37_76AB,
        "js" | "ts" => 0xFFF7_DF1E,
        "json" | "toml" | "yaml" => 0xFF98_C379,
        _ => 0xFF5C_6370,
    };
    let badge_w = (w * 35) / 100;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        x + w - badge_w - 3,
        y + h - 8,
        badge_w,
        5,
        2,
        badge_color,
        badge_color,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_audio_thumbnail_card(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let sleeve_bg = 0xFF2B_2D42;
    let sleeve_border = 0xFF4A_4E69;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        x,
        y,
        w,
        h,
        4,
        sleeve_bg,
        sleeve_border,
    );

    let center_x = x + w / 2;
    let center_y = y + h / 2;
    let disc_radius = (w.min(h) / 2 - 4).max(6);
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        center_x - disc_radius,
        center_y - disc_radius,
        disc_radius * 2,
        disc_radius * 2,
        disc_radius,
        0xFF1A_1A24,
        0xFF3A_3A4A,
    );

    let label_radius = disc_radius / 2;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        center_x - label_radius,
        center_y - label_radius,
        label_radius * 2,
        label_radius * 2,
        label_radius,
        0xFFE0_56FD,
        0xFFBE_2EED,
    );

    draw_rect_clipped(
        buf,
        stride,
        height,
        clip,
        center_x - 1,
        center_y - 1,
        2,
        2,
        0xFFFF_FFFF,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn draw_video_thumbnail_card(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let slate_bg = 0xFF1E_232A;
    let slate_border = 0xFF3E_4754;
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        x,
        y,
        w,
        h,
        3,
        slate_bg,
        slate_border,
    );

    for i in 0..4 {
        let hole_y = y + 4 + i * 8;
        if hole_y + 4 < y + h - 2 {
            draw_rect_clipped(buf, stride, height, clip, x + 2, hole_y, 2, 4, 0xFFFF_FFFF);
            draw_rect_clipped(
                buf,
                stride,
                height,
                clip,
                x + w - 4,
                hole_y,
                2,
                4,
                0xFFFF_FFFF,
            );
        }
    }

    let scr_x = x + 7;
    let scr_y = y + 5;
    let scr_w = (w - 14).max(6);
    let scr_h = (h - 10).max(6);
    draw_rounded_rect_box_clipped(
        buf,
        stride,
        height,
        clip,
        scr_x,
        scr_y,
        scr_w,
        scr_h,
        2,
        0xFF0D_1117,
        0xFF2D_333B,
    );

    let play_x = scr_x + scr_w / 2 - 3;
    let play_y = scr_y + scr_h / 2 - 5;
    for dy in 0..10 {
        let span = if dy <= 5 { dy + 1 } else { 10 - dy };
        draw_rect_clipped(
            buf,
            stride,
            height,
            clip,
            play_x,
            play_y + dy,
            span,
            1,
            0xFF3D_AEE9,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_archive_thumbnail_card(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let box_bg = 0xFFD3_5400;
    let box_border = 0xFFE6_7E22;
    draw_rounded_rect_box_clipped(buf, stride, height, clip, x, y, w, h, 3, box_bg, box_border);

    let zip_x = x + w / 2 - 3;
    draw_rect_clipped(
        buf,
        stride,
        height,
        clip,
        zip_x,
        y + 2,
        6,
        h - 4,
        0xFF2C_3E50,
    );

    for i in 0..(h - 8) / 4 {
        let zy = y + 4 + i * 4;
        let tooth_x = if i % 2 == 0 { zip_x } else { zip_x + 3 };
        draw_rect_clipped(buf, stride, height, clip, tooth_x, zy, 3, 2, 0xFFBD_C3C7);
    }
}

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

/// Draws an accessible, high-contrast outer keyboard focus ring around a component.
/// Complies with WCAG 2.4.7 (Focus Visible) and WCAG 1.4.11 (Non-text Contrast).
#[allow(clippy::too_many_arguments)]
pub fn draw_focus_ring(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    ring_color: u32,
) {
    if w <= 0 || h <= 0 || stride == 0 || height == 0 || buf.len() < stride * height {
        return;
    }
    let offset = 2;
    let ring_thickness = 2;
    let ox = x - offset;
    let oy = y - offset;
    let ow = w + offset * 2;
    let oh = h + offset * 2;
    let or = (radius + offset).min(ow / 2).min(oh / 2);

    let ix = ox + ring_thickness;
    let iy = oy + ring_thickness;
    let iw = ow - ring_thickness * 2;
    let ih = oh - ring_thickness * 2;
    let ir = (or - ring_thickness).max(0);

    let x_start = ox.max(0) as usize;
    let y_start = oy.max(0) as usize;
    let x_end = ((ox + ow).max(0) as usize).min(stride);
    let y_end = ((oy + oh).max(0) as usize).min(height);

    for row in y_start..y_end {
        let row_offset = row * stride;
        for col in x_start..x_end {
            let (px, py) = (col as i32, row as i32);
            if let Some(c) = clip {
                if px < c.x || py < c.y || px >= c.x + c.w || py >= c.y + c.h {
                    continue;
                }
            }
            let outer = rounded_rect_coverage(ox, oy, ow, oh, or, px, py);
            if outer <= 0.0 {
                continue;
            }
            let inner = if iw > 0 && ih > 0 {
                rounded_rect_coverage(ix, iy, iw, ih, ir, px, py)
            } else {
                0.0
            };
            let ring_a = (outer * (1.0 - inner)).clamp(0.0, 1.0);
            if ring_a > 0.0 {
                if ring_a >= 1.0 {
                    buf[row_offset + col] = ring_color;
                } else {
                    blend_px(buf, row_offset + col, ring_color, ring_a);
                }
            }
        }
    }
}

/// WCAG 2.1 relative luminance calculation of an ARGB/RGB color.
pub fn relative_luminance(color: u32) -> f32 {
    let r = ((color >> 16) & 0xFF) as f32 / 255.0;
    let g = ((color >> 8) & 0xFF) as f32 / 255.0;
    let b = (color & 0xFF) as f32 / 255.0;

    let to_linear = |v: f32| -> f32 {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };

    0.2126 * to_linear(r) + 0.7152 * to_linear(g) + 0.0722 * to_linear(b)
}

/// Computes the WCAG 2.1 contrast ratio between two colors (range: 1.0 to 21.0).
pub fn contrast_ratio(c1: u32, c2: u32) -> f32 {
    let l1 = relative_luminance(c1);
    let l2 = relative_luminance(c2);
    let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Checks whether a foreground/background color pair satisfies WCAG 2.1 AA (4.5:1 normal, 3.0:1 large/UI).
pub fn wcag_aa_compliant(fg: u32, bg: u32, is_large_or_ui: bool) -> bool {
    let ratio = contrast_ratio(fg, bg);
    if is_large_or_ui {
        ratio >= 3.0
    } else {
        ratio >= 4.5
    }
}

/// Checks whether a foreground/background color pair satisfies WCAG 2.1 AAA (7.0:1 normal, 4.5:1 large/UI).
pub fn wcag_aaa_compliant(fg: u32, bg: u32, is_large_or_ui: bool) -> bool {
    let ratio = contrast_ratio(fg, bg);
    if is_large_or_ui {
        ratio >= 4.5
    } else {
        ratio >= 7.0
    }
}

/// Accessible semantic roles for screen readers and accessibility inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessibleRole {
    Window,
    Dialog,
    Button,
    TextInput,
    Checkbox,
    Slider,
    Select,
    StaticText,
    Heading,
    StatusLine,
    List,
    ListItem,
    CodeBlock,
    ScrollArea,
    Unknown,
}

impl AccessibleRole {
    pub fn as_str(self) -> &'static str {
        match self {
            AccessibleRole::Window => "window",
            AccessibleRole::Dialog => "dialog",
            AccessibleRole::Button => "button",
            AccessibleRole::TextInput => "textbox",
            AccessibleRole::Checkbox => "checkbox",
            AccessibleRole::Slider => "slider",
            AccessibleRole::Select => "combobox",
            AccessibleRole::StaticText => "text",
            AccessibleRole::Heading => "heading",
            AccessibleRole::StatusLine => "status",
            AccessibleRole::List => "list",
            AccessibleRole::ListItem => "listitem",
            AccessibleRole::CodeBlock => "code",
            AccessibleRole::ScrollArea => "region",
            AccessibleRole::Unknown => "generic",
        }
    }
}

/// Accessible node hierarchy for screen readers and UI accessibility audit.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessibleNode {
    pub id: Option<String>,
    pub role: AccessibleRole,
    pub name: String,
    pub value: Option<String>,
    pub rect: Rect,
    pub disabled: bool,
    pub focused: bool,
    pub checked: Option<bool>,
    pub children: Vec<AccessibleNode>,
}

/// Linear interpolation between two ARGB colours: `t = 0.0` is `top`,
/// `t = 1.0` is `bottom`, blended per channel. The alpha byte is carried over
/// from `top`, so a fully opaque gradient writes straight through without a
/// needless blend.
#[inline]
fn lerp_color(top: u32, bottom: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |a: u32, b: u32| (a as f32 + (b as f32 - a as f32) * t).round() as u32;
    let r = ch((top >> 16) & 0xFF, (bottom >> 16) & 0xFF);
    let g = ch((top >> 8) & 0xFF, (bottom >> 8) & 0xFF);
    let b = ch(top & 0xFF, bottom & 0xFF);
    (((top >> 24) & 0xFF) << 24) | (r << 16) | (g << 8) | b
}

/// Normalised position of pixel `(px, py)` along a gradient that runs from
/// `top` to `bottom` over the rect `(x, y, w, h)`: `0.0` on the leading edge
/// and `1.0` on the trailing one. Vertical gradients run **down** (so a button
/// is lighter at the top, the way light falls on a raised surface), horizontal
/// ones run right.
///
/// Measured from pixel *position*, not centre, so the first and last row/column
/// land on exactly `0.0` and `1.0` — the end colours appear verbatim, which
/// keeps them assertable instead of merely approximate.
#[inline]
fn gradient_t(px: i32, py: i32, x: i32, y: i32, w: i32, h: i32, horizontal: bool) -> f32 {
    let (pos, span) = if horizontal {
        ((px - x) as f32, (w - 1) as f32)
    } else {
        ((py - y) as f32, (h - 1) as f32)
    };
    if span <= 0.0 {
        0.0
    } else {
        (pos / span).clamp(0.0, 1.0)
    }
}

/// Rectangle filled with a linear gradient, `top` → `bottom`: down the left
/// edge by default, or across the top when `horizontal` is set. Square corners
/// and no anti-aliasing — use [`draw_rounded_rect_gradient_box_clipped`] for
/// shapes that need a radius.
#[allow(clippy::too_many_arguments)]
pub fn draw_rect_gradient_clipped(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    top: u32,
    bottom: u32,
    horizontal: bool,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    if let Some(c) = clip {
        if intersect(Rect { x, y, w, h }, c).is_none() {
            return;
        }
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
            let t = gradient_t(px, py, x, y, w, h, horizontal);
            buf[row_offset + col] = lerp_color(top, bottom, t);
        }
    }
}

/// Same as [`draw_rounded_rect_box_clipped`], but the interior ramps from
/// `top` to `bottom` instead of using one flat `fill`.
///
/// The 1px ring stays a **flat** `border`: at one pixel wide a gradient reads
/// as a smudge rather than an edge, and a constant outline is what makes the
/// shape's silhouette legible against a gradient fill. The ramp is measured
/// across the interior rather than the whole shape, so both end colours land
/// on pixels you can actually see.
#[allow(clippy::too_many_arguments)]
pub fn draw_rounded_rect_gradient_box_clipped(
    buf: &mut [u32],
    stride: usize,
    height: usize,
    clip: Option<Rect>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    r: i32,
    top: u32,
    bottom: u32,
    border: u32,
    horizontal: bool,
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

    // Interior after removing the 1px border on every side — this is the part
    // that actually receives paint, so the ramp is measured across *it*: the
    // topmost filled row lands on exactly `top` and the bottommost on exactly
    // `bottom`. Measuring across the whole shape instead would hide both end
    // colours underneath the border ring.
    let (ix, iy, iw, ih) = (x + 1, y + 1, w - 2, h - 2);
    let (gx, gy, gw, gh) = if iw > 0 && ih > 0 {
        (ix, iy, iw, ih)
    } else {
        // Degenerate (1–2px) shape: there is no ring worth drawing separately,
        // so ramp across whatever remains.
        (x, y, w, h)
    };

    if r <= 0 {
        // Square fallback: fill only the interior, then ring it, so the result
        // matches the flat helper byte-for-byte while keeping the ramp pinned
        // to visible pixels. The gradient is computed over the *unclipped*
        // interior, so a box sliced by a viewport keeps the ramp it would have
        // had in full.
        match clip.and_then(|c| intersect(Rect { x, y, w, h }, c)) {
            None => {
                if clip.is_none() {
                    draw_rect_gradient_clipped(
                        buf, stride, height, None, gx, gy, gw, gh, top, bottom, horizontal,
                    );
                    draw_rect_outline(buf, stride, height, x, y, w, h, border);
                }
            }
            Some(rc) => {
                draw_rect_gradient_clipped(
                    buf,
                    stride,
                    height,
                    Some(rc),
                    gx,
                    gy,
                    gw,
                    gh,
                    top,
                    bottom,
                    horizontal,
                );
                draw_rect_outline(buf, stride, height, rc.x, rc.y, rc.w, rc.h, border);
            }
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
                let fill = lerp_color(top, bottom, gradient_t(px, py, gx, gy, gw, gh, horizontal));
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
    pub max_w: i32,
    pub max_h: i32,
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
        max_w,
        max_h: 1200,
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
            let align = elem
                .props
                .get("align")
                .map(|s| s.as_str())
                .unwrap_or("start");
            let expand = elem.props.get("expand").is_some_and(|v| v == "true");
            let explicit_w = elem
                .props
                .get("width")
                .or_else(|| elem.props.get("w"))
                .and_then(|s| s.parse::<i32>().ok());
            let explicit_h = elem
                .props
                .get("height")
                .or_else(|| elem.props.get("h"))
                .and_then(|s| s.parse::<i32>().ok());

            let start_box_idx = boxes.len();
            let mut child_y = y + padding;
            let mut max_child_w = 0;

            for (child_idx, child) in elem.children.iter().enumerate() {
                let subsequent_h: i32 = elem
                    .children
                    .iter()
                    .skip(child_idx + 1)
                    .map(|c| {
                        let eh = c
                            .props
                            .get("height")
                            .or_else(|| c.props.get("h"))
                            .and_then(|s| s.parse::<i32>().ok())
                            .unwrap_or(28);
                        let pad = c
                            .props
                            .get("padding")
                            .and_then(|s| s.parse::<i32>().ok())
                            .unwrap_or(0);
                        eh + pad * 2 + gap
                    })
                    .sum();
                let saved_max_h = ctx.max_h;
                if let Some(eh) = explicit_h {
                    ctx.max_h = (y + eh - subsequent_h).max(child_y + 20);
                } else {
                    ctx.max_h = (ctx.max_h - subsequent_h).max(y + padding + 60);
                }
                let col_avail_w = explicit_w.unwrap_or(max_w) - 2 * padding;
                let (cw, ch) = layout_element_ctx(
                    child,
                    x + padding,
                    child_y,
                    col_avail_w,
                    boxes,
                    ctx,
                    pan,
                    clips,
                );
                ctx.max_h = saved_max_h;
                max_child_w = max_child_w.max(cw);
                child_y += ch + gap;
            }

            let total_h = if elem.children.is_empty() {
                2 * padding
            } else {
                child_y - gap + padding - y
            };
            let explicit_w = elem
                .props
                .get("width")
                .or_else(|| elem.props.get("w"))
                .and_then(|s| s.parse::<i32>().ok());
            let total_w = if let Some(ew) = explicit_w {
                ew
            } else if expand {
                max_w
            } else {
                max_child_w + 2 * padding
            };
            let expand_h = elem.props.get("expand_h").is_some_and(|v| v == "true")
                || elem
                    .props
                    .get("height")
                    .map(|s| s == "auto" || s == "100%")
                    .unwrap_or(false);
            let final_h = if let Some(eh) = explicit_h {
                eh
            } else if expand_h {
                (ctx.max_h - y - padding).max(total_h)
            } else {
                total_h
            };

            // Post-pass: apply cross-axis alignment (`align` = "center" | "end")
            if align == "center" || align == "end" {
                let avail_w = total_w - 2 * padding;
                for b in &mut boxes[start_box_idx..] {
                    let offset = if align == "center" {
                        (avail_w - b.rect.w) / 2
                    } else {
                        avail_w - b.rect.w
                    };
                    if offset > 0 {
                        b.rect.x += offset;
                    }
                }
            }

            if elem.props.contains_key("bg") {
                boxes.insert(
                    start_box_idx,
                    LayoutBox {
                        rect: Rect {
                            x,
                            y,
                            w: total_w,
                            h: final_h,
                        },
                        tag: "panel".into(),
                        id: elem.id.clone(),
                        text: None,
                        on_click: None,
                        on_change: None,
                        on_key: None,
                        pan,
                        clips: clips_owned(clips),
                        pad: 0,
                        disabled: false,
                        tooltip: None,
                        checked: false,
                        slider: None,
                        options: Vec::new(),
                        draggable: false,
                        props: elem.props.clone(),
                    },
                );
            }

            (total_w, final_h)
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
            let justify = elem
                .props
                .get("justify")
                .map(|s| s.as_str())
                .unwrap_or("start");
            let align = elem
                .props
                .get("align")
                .map(|s| s.as_str())
                .unwrap_or("start");
            let expand = elem.props.get("expand").is_some_and(|v| v == "true");

            let start_box_idx = boxes.len();
            let mut child_x = x + padding;
            let mut max_child_h = 0;
            let mut child_box_ranges = Vec::new();

            for (child_idx, child) in elem.children.iter().enumerate() {
                let idx_before = boxes.len();
                let child_expand = child.props.get("expand").is_some_and(|v| v == "true");
                let child_max_w = if child_expand {
                    let subsequent_w: i32 = elem
                        .children
                        .iter()
                        .skip(child_idx + 1)
                        .map(|c| {
                            let ew = c
                                .props
                                .get("width")
                                .or_else(|| c.props.get("w"))
                                .and_then(|s| s.parse::<i32>().ok())
                                .unwrap_or(0);
                            let pad = c
                                .props
                                .get("padding")
                                .and_then(|s| s.parse::<i32>().ok())
                                .unwrap_or(0);
                            ew + pad * 2 + gap
                        })
                        .sum();
                    let rem_expanding = elem
                        .children
                        .iter()
                        .skip(child_idx)
                        .filter(|c| c.props.get("expand").is_some_and(|v| v == "true"))
                        .count()
                        .max(1) as i32;
                    let avail = (max_w - (child_x - x) - padding - subsequent_w).max(60);
                    (avail / rem_expanding).max(60)
                } else {
                    max_w
                };
                let (cw, ch) = layout_element_ctx(
                    child,
                    child_x,
                    y + padding,
                    child_max_w,
                    boxes,
                    ctx,
                    pan,
                    clips,
                );
                let idx_after = boxes.len();
                child_box_ranges.push((idx_before..idx_after, cw));
                max_child_h = max_child_h.max(ch);
                child_x += cw + gap;
            }

            let content_w = if elem.children.is_empty() {
                0
            } else {
                child_x - gap - (x + padding)
            };
            let explicit_w = elem
                .props
                .get("width")
                .or_else(|| elem.props.get("w"))
                .and_then(|s| s.parse::<i32>().ok());
            let explicit_h = elem
                .props
                .get("height")
                .or_else(|| elem.props.get("h"))
                .and_then(|s| s.parse::<i32>().ok());
            let total_w = if let Some(ew) = explicit_w {
                ew
            } else if expand || justify == "between" || justify == "end" || justify == "center" {
                max_w
            } else {
                content_w + 2 * padding
            };
            let expand_h = elem.props.get("expand_h").is_some_and(|v| v == "true")
                || elem
                    .props
                    .get("height")
                    .map(|s| s == "auto" || s == "100%")
                    .unwrap_or(false);
            let total_h = if let Some(eh) = explicit_h {
                eh
            } else if expand_h {
                (ctx.max_h - y - padding).max(max_child_h + 2 * padding)
            } else {
                max_child_h + 2 * padding
            };

            // Post-pass: apply main-axis distribution (`justify`) and cross-axis alignment (`align`)
            let avail_w = total_w - 2 * padding;
            let count = elem.children.len();

            if (justify == "between" && count > 1) || justify == "end" || justify == "center" {
                let extra_w = (avail_w - content_w).max(0);
                if justify == "end" {
                    for b in &mut boxes[start_box_idx..] {
                        b.rect.x += extra_w;
                    }
                } else if justify == "center" {
                    let offset = extra_w / 2;
                    for b in &mut boxes[start_box_idx..] {
                        b.rect.x += offset;
                    }
                } else if justify == "between" && count > 1 {
                    let bonus_gap = extra_w / (count as i32 - 1);
                    for (i, (range, _)) in child_box_ranges.iter().enumerate() {
                        let shift = (i as i32) * bonus_gap;
                        for b in &mut boxes[range.clone()] {
                            b.rect.x += shift;
                        }
                    }
                }
            }

            let avail_h = total_h - 2 * padding;
            for (child_idx, (range, _)) in child_box_ranges.iter().enumerate() {
                if let Some(child_elem) = elem.children.get(child_idx) {
                    if child_elem
                        .props
                        .get("expand_h")
                        .is_some_and(|v| v == "true")
                    {
                        for b in &mut boxes[range.clone()] {
                            if b.tag == "divider"
                                || b.tag == "panel"
                                || b.tag == "column"
                                || b.tag == "scroll"
                            {
                                b.rect.h = b.rect.h.max(avail_h);
                            }
                        }
                    }
                }
            }

            if align == "center" || align == "end" {
                for b in &mut boxes[start_box_idx..] {
                    let offset = if align == "center" {
                        (avail_h - b.rect.h) / 2
                    } else {
                        avail_h - b.rect.h
                    };
                    if offset > 0 {
                        b.rect.y += offset;
                    }
                }
            }

            if elem.props.contains_key("bg") {
                boxes.insert(
                    start_box_idx,
                    LayoutBox {
                        rect: Rect {
                            x,
                            y,
                            w: total_w,
                            h: total_h,
                        },
                        tag: "panel".into(),
                        id: elem.id.clone(),
                        text: None,
                        on_click: None,
                        on_change: None,
                        on_key: None,
                        pan,
                        clips: clips_owned(clips),
                        pad: 0,
                        disabled: false,
                        tooltip: None,
                        checked: false,
                        slider: None,
                        options: Vec::new(),
                        draggable: false,
                        props: elem.props.clone(),
                    },
                );
            }

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
            let expand_h = elem.props.get("expand").is_some_and(|v| v == "true")
                || elem.props.get("expand_h").is_some_and(|v| v == "true")
                || elem
                    .props
                    .get("height")
                    .map(|s| s == "auto" || s == "100%")
                    .unwrap_or(false);
            let explicit_h = elem
                .props
                .get("height")
                .or_else(|| elem.props.get("h"))
                .and_then(|s| s.parse::<i32>().ok());
            let viewport_h = if let Some(eh) = explicit_h {
                eh
            } else if expand_h {
                (ctx.max_h - y - padding).max(120)
            } else {
                240
            };
            let explicit_w = elem
                .props
                .get("width")
                .or_else(|| elem.props.get("w"))
                .and_then(|s| s.parse::<i32>().ok());
            let viewport_w = explicit_w.unwrap_or(max_w).clamp(1, max_w.max(1));

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
                disabled: false,
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                checked: false,
                slider: None,
                draggable: elem.props.get("draggable").is_some_and(|s| is_truthy(s)),
                props: elem.props.clone(),
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
                disabled: false,
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                checked: false,
                slider: None,
                draggable: elem.props.get("draggable").is_some_and(|s| is_truthy(s))
                    || elem.id.is_some(),
                props: elem.props.clone(),
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
            let explicit_w = elem
                .props
                .get("width")
                .or_else(|| elem.props.get("w"))
                .and_then(|s| s.parse::<i32>().ok());
            let explicit_h = elem
                .props
                .get("height")
                .or_else(|| elem.props.get("h"))
                .and_then(|s| s.parse::<i32>().ok());
            let variant = elem
                .props
                .get("variant")
                .map(|s| s.as_str())
                .unwrap_or("default");
            let expand = elem.props.get("expand").is_some_and(|v| v == "true");
            let w = if let Some(ew) = explicit_w {
                ew
            } else if expand {
                max_w
            } else if variant == "toolbar" || variant == "icon" {
                (text_w + 16).max(28)
            } else if variant == "flat" || variant == "ghost" {
                text_w + 12
            } else {
                (text_w + 32).max(80)
            };
            let h = if let Some(eh) = explicit_h {
                eh
            } else if variant == "toolbar"
                || variant == "icon"
                || variant == "flat"
                || variant == "ghost"
                || variant == "item"
            {
                28
            } else {
                (CELL_H + 16).max(36)
            };
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
                disabled: elem.props.get("disabled").is_some_and(|s| is_truthy(s)),
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                checked: false,
                slider: None,
                draggable: elem.props.get("draggable").is_some_and(|s| is_truthy(s)),
                props: elem.props.clone(),
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
            let explicit_w = elem
                .props
                .get("width")
                .or_else(|| elem.props.get("w"))
                .and_then(|s| s.parse::<i32>().ok());
            let explicit_h = elem
                .props
                .get("height")
                .or_else(|| elem.props.get("h"))
                .and_then(|s| s.parse::<i32>().ok());
            let expand = elem.props.get("expand").is_some_and(|v| v == "true");
            let w = if let Some(ew) = explicit_w {
                ew
            } else if expand {
                max_w
            } else {
                220.max(max_line_len * CELL_W + 24)
            };
            let h = explicit_h.unwrap_or((CELL_H + 16).max(lines * CELL_H + 16));
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
                disabled: elem.props.get("disabled").is_some_and(|s| is_truthy(s)),
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                checked: false,
                slider: None,
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        // A closed choice field. Its width is settled by the widest thing it
        // can ever show — the current label or any option — so opening the list
        // never resizes the field, and a row always fits the box that chose it.
        // The list itself is an overlay drawn by the frame, never laid out here.
        "select" => {
            let value = elem
                .text
                .clone()
                .or_else(|| elem.props.get("value").cloned())
                .unwrap_or_default();
            let options: Vec<String> = elem
                .props
                .get("options")
                .map(|o| {
                    o.split('\n')
                        .filter(|l| !l.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let widest = options.iter().map(|o| o.chars().count()).max().unwrap_or(0);
            let text_w = value.chars().count().max(widest) as i32 * CELL_W;
            let w = (text_w + 56).max(140);
            let h = (CELL_H + 16).max(36);
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "select".into(),
                id: elem.id.clone(),
                text: Some(value),
                on_click: None,
                on_change: elem.on_change.clone(),
                on_key: elem.on_key.clone(),
                pan,
                clips: clips_owned(clips),
                pad: 0,
                disabled: elem.props.get("disabled").is_some_and(|s| is_truthy(s)),
                tooltip: elem.props.get("tooltip").cloned(),
                options,
                checked: false,
                slider: None,
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        // A boolean control: the square or the track, a gap, then the label
        // beside it — all one box, so pressing the words presses the control.
        // Making the row the control is deliberate: text next to a switch
        // that does nothing when clicked is text people learn to avoid.
        "checkbox" | "toggle" => {
            let label = elem
                .text
                .clone()
                .or_else(|| elem.props.get("label").cloned())
                .unwrap_or_default();
            let control_w = flag_control_w(&elem.tag);
            let text_w = label.chars().count() as i32 * CELL_W;
            let w = (control_w + FLAG_GAP + text_w + 8).max(control_w + 18);
            let h = FLAG_H;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: elem.tag.clone(),
                id: elem.id.clone(),
                text: Some(label),
                // No `on_click`: a press that flips the switch *is* the
                // report, so there is no second event to write and no way to
                // wonder which of two handlers fired.
                on_click: None,
                on_change: elem.on_change.clone(),
                on_key: elem.on_key.clone(),
                pan,
                clips: clips_owned(clips),
                pad: 0,
                // Absent means off, and the word is the one `is_truthy`
                // already reads as "on" — so the interpreter that wrote it
                // and the arm that reads it cannot disagree.
                checked: elem.props.get("value").is_some_and(|sv| is_truthy(sv)),
                slider: None,
                disabled: elem.props.get("disabled").is_some_and(|sv| is_truthy(sv)),
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        // A slider: the track, a gap, then the label beside it — again all one
        // box, so pressing the words presses the control. The track is a fixed
        // length on purpose: a dial that grew with its window would be a
        // different control in every layout, and the mapping from value to
        // handle position is the only thing the eye learns.
        "slider" => {
            let label = elem
                .text
                .clone()
                .or_else(|| elem.props.get("label").cloned())
                .unwrap_or_default();
            let text_w = label.chars().count() as i32 * CELL_W;
            let w = (SLIDER_TRACK_W + FLAG_GAP + text_w + 8).max(SLIDER_TRACK_W + 18);
            let h = SLIDER_H;
            // Bounds first, because the value is read back already snapped onto
            // the control's own grid: a position the program can never be
            // handed must not be the position the handle starts at either.
            let min: f64 = elem
                .props
                .get("min")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let max: f64 = elem
                .props
                .get("max")
                .and_then(|s| s.parse().ok())
                .unwrap_or(100.0);
            let given: f64 = elem
                .props
                .get("value")
                .and_then(|s| s.parse().ok())
                .unwrap_or(min);
            let step: f64 = elem
                .props
                .get("step")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0);
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: elem.tag.clone(),
                id: elem.id.clone(),
                text: Some(label),
                // No `on_click`: a drag that moves the handle *is* the report,
                // so there is no second event to write and no way to wonder
                // which of two handlers fired.
                on_click: None,
                on_change: elem.on_change.clone(),
                on_key: elem.on_key.clone(),
                pan,
                clips: clips_owned(clips),
                pad: 0,
                checked: false,
                slider: Some(SliderSpec {
                    value: crate::ui::slider_snap(given, min, max, step),
                    min,
                    max,
                    step,
                }),
                disabled: elem.props.get("disabled").is_some_and(|sv| is_truthy(sv)),
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        "progress" => {
            let label = elem
                .text
                .clone()
                .or_else(|| elem.props.get("label").cloned())
                .unwrap_or_default();
            let text_w = if label.is_empty() {
                0
            } else {
                FLAG_GAP + label.chars().count() as i32 * CELL_W + 8
            };
            let explicit_w = elem
                .props
                .get("width")
                .or_else(|| elem.props.get("w"))
                .and_then(|s| s.parse::<i32>().ok());
            let w = explicit_w.unwrap_or(PROGRESS_TRACK_W + text_w);
            let h = PROGRESS_H;
            let min: f64 = elem
                .props
                .get("min")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let max: f64 = elem
                .props
                .get("max")
                .and_then(|s| s.parse().ok())
                .unwrap_or(100.0);
            let val: f64 = elem
                .props
                .get("value")
                .and_then(|s| s.parse().ok())
                .unwrap_or(min);
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "progress".into(),
                id: elem.id.clone(),
                text: if label.is_empty() { None } else { Some(label) },
                on_click: None,
                on_change: None,
                on_key: None,
                pan,
                clips: clips_owned(clips),
                pad: 0,
                checked: false,
                slider: Some(SliderSpec {
                    value: val.clamp(min, max),
                    min,
                    max,
                    step: 1.0,
                }),
                disabled: elem.props.get("disabled").is_some_and(|sv| is_truthy(sv)),
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        "spinner" => {
            let label = elem
                .text
                .clone()
                .or_else(|| elem.props.get("label").cloned())
                .unwrap_or_default();
            let text_w = if label.is_empty() {
                0
            } else {
                FLAG_GAP + label.chars().count() as i32 * CELL_W + 8
            };
            let w = SPINNER_SIZE + text_w;
            let h = PROGRESS_H;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "spinner".into(),
                id: elem.id.clone(),
                text: if label.is_empty() { None } else { Some(label) },
                on_click: None,
                on_change: None,
                on_key: None,
                pan,
                clips: clips_owned(clips),
                pad: 0,
                checked: false,
                slider: None,
                disabled: elem.props.get("disabled").is_some_and(|sv| is_truthy(sv)),
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        "image" | "icon" => {
            let default_size = if elem.tag == "icon" { 24 } else { 48 };
            let w: i32 = elem
                .props
                .get("width")
                .and_then(|s| s.parse().ok())
                .unwrap_or(default_size);
            let h: i32 = elem
                .props
                .get("height")
                .and_then(|s| s.parse().ok())
                .unwrap_or(default_size);
            let src = elem.text.clone().or_else(|| elem.props.get("src").cloned());
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: elem.tag.clone(),
                id: elem.id.clone(),
                text: src,
                on_click: elem.on_click.clone(),
                on_change: None,
                on_key: None,
                pan,
                clips: clips_owned(clips),
                pad: 0,
                checked: false,
                slider: None,
                disabled: elem.props.get("disabled").is_some_and(|sv| is_truthy(sv)),
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        "tabs" => {
            let active = elem
                .text
                .clone()
                .or_else(|| elem.props.get("value").cloned())
                .unwrap_or_default();
            let options: Vec<String> = elem
                .props
                .get("options")
                .map(|o| {
                    o.split('\n')
                        .filter(|l| !l.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let mut total_w = 0;
            for opt in &options {
                let tw = opt.chars().count() as i32 * CELL_W + 24;
                total_w += tw;
            }
            let w = total_w.max(80);
            let h = 34;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "tabs".into(),
                id: elem.id.clone(),
                text: Some(active),
                on_click: None,
                on_change: elem.on_change.clone(),
                on_key: elem.on_key.clone(),
                pan,
                clips: clips_owned(clips),
                pad: 0,
                checked: false,
                slider: None,
                disabled: elem.props.get("disabled").is_some_and(|sv| is_truthy(sv)),
                tooltip: elem.props.get("tooltip").cloned(),
                options,
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        "code_block" => {
            let code = elem.text.clone().unwrap_or_default();
            let lines: Vec<&str> = code.split('\n').collect();
            let line_count = lines.len().max(1);
            let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let w = (max_line_len as i32 * CELL_W + 64).min(max_w).max(160);
            let h = (line_count as i32 * 20 + 24).max(44);
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "code_block".into(),
                id: elem.id.clone(),
                text: Some(code),
                on_click: elem.on_click.clone(),
                on_change: None,
                on_key: None,
                pan,
                clips: clips_owned(clips),
                pad: 12,
                checked: false,
                slider: None,
                disabled: false,
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        "divider" => {
            let orientation = elem
                .props
                .get("orientation")
                .map(|s| s.as_str())
                .unwrap_or("horizontal");
            let default_size = if orientation == "vertical" { 1 } else { 16 };
            let size = elem
                .props
                .get("size")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(default_size);
            let explicit_w = elem
                .props
                .get("width")
                .or_else(|| elem.props.get("w"))
                .and_then(|s| s.parse::<i32>().ok());
            let explicit_h = elem
                .props
                .get("height")
                .or_else(|| elem.props.get("h"))
                .and_then(|s| s.parse::<i32>().ok());
            let expand_h = elem.props.get("expand_h").is_some_and(|v| v == "true");
            let (w, h) = if orientation == "vertical" {
                let vh = explicit_h.unwrap_or_else(|| {
                    if expand_h {
                        (ctx.max_h - y).max(24)
                    } else {
                        24
                    }
                });
                (size.max(1), vh)
            } else {
                let hw = explicit_w.unwrap_or(max_w.max(20));
                (hw, size.max(1))
            };
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "divider".into(),
                id: elem.id.clone(),
                text: None,
                on_click: None,
                on_change: None,
                on_key: None,
                pan,
                clips: clips_owned(clips),
                pad: 0,
                checked: false,
                slider: None,
                disabled: false,
                tooltip: None,
                options: Vec::new(),
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        "spacer" => {
            let expand = elem.props.get("expand").is_some_and(|v| v == "true");
            let size = elem
                .props
                .get("size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(16);
            let w = if expand {
                max_w.max(size)
            } else {
                elem.props
                    .get("width")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(size)
            };
            let h = elem
                .props
                .get("height")
                .and_then(|s| s.parse().ok())
                .unwrap_or(size);
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "spacer".into(),
                id: elem.id.clone(),
                text: None,
                on_click: None,
                on_change: None,
                on_key: None,
                pan,
                clips: clips_owned(clips),
                pad: 0,
                checked: false,
                slider: None,
                disabled: false,
                tooltip: None,
                options: Vec::new(),
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        "badge" | "tag" => {
            let label = elem.text.clone().unwrap_or_default();
            let text_w = label.chars().count() as i32 * CELL_W;
            let w = (text_w + 16).min(max_w).max(32);
            let h = 22;
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: elem.tag.clone(),
                id: elem.id.clone(),
                text: Some(label),
                on_click: elem.on_click.clone(),
                on_change: None,
                on_key: None,
                pan,
                clips: clips_owned(clips),
                pad: 4,
                checked: false,
                slider: None,
                disabled: false,
                tooltip: elem.props.get("tooltip").cloned(),
                options: Vec::new(),
                draggable: false,
                props: elem.props.clone(),
            });
            (w, h)
        }
        "status_line" => {
            let message = elem.text.clone().unwrap_or_default();
            let text_w = message.chars().count() as i32 * CELL_W;
            let w = (text_w + 36).min(max_w).max(120);
            let h = 28;
            let status = elem
                .props
                .get("status")
                .cloned()
                .unwrap_or_else(|| "info".into());
            boxes.push(LayoutBox {
                rect: Rect { x, y, w, h },
                tag: "status_line".into(),
                id: elem.id.clone(),
                text: Some(message),
                on_click: elem.on_click.clone(),
                on_change: None,
                on_key: None,
                pan,
                clips: clips_owned(clips),
                pad: 8,
                checked: false,
                slider: None,
                disabled: false,
                tooltip: elem.props.get("tooltip").cloned(),
                options: vec![status],
                draggable: false,
                props: elem.props.clone(),
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

/// Stable dispatch key for a button: its `id`, else `on_click`, else a
/// layout-order index. Hover, the pressed state and the actual dispatch all
/// resolve through this one function, so a press cannot highlight one box and
/// fire another.
pub fn button_key(lbox: &LayoutBox, index: usize) -> String {
    lbox.id
        .clone()
        .or_else(|| lbox.on_click.clone())
        .unwrap_or_else(|| format!("__btn_{index}"))
}

/// Dispatch key of a `select` box: its `id` when it has one, else the layout
/// order. Hover, the open list and the press all resolve it the same way, so
/// the field that lit up is the one that opens.
pub fn select_key(lbox: &LayoutBox, index: usize) -> String {
    lbox.id.clone().unwrap_or_else(|| format!("__sel_{index}"))
}

/// Dispatch key of a boolean control: its `id` when it has one, else the
/// layout order. Hover, the pressed tone and the flip all resolve it the same
/// way, so the control that lit up is the one that changes.
pub fn flag_key(lbox: &LayoutBox, index: usize) -> String {
    lbox.id.clone().unwrap_or_else(|| format!("__flag_{index}"))
}

/// Side of the square a `checkbox` draws. Small enough to sit beside its
/// label without dominating it, large enough that the mark inside has room to
/// be a mark rather than a smudge.
pub const FLAG_BOX: i32 = 18;
/// Size of a `toggle` track: wide enough for the knob to travel visibly
/// between the two ends, so on and off cannot be told apart at a glance.
pub const TOGGLE_W: i32 = 44;
pub const TOGGLE_H: i32 = 24;
/// Distance between the control and the label that belongs to it.
pub const FLAG_GAP: i32 = 10;
/// Height of a whole boolean control — box or track, whatever is taller once
/// the label's line is accounted for. Fixed: neither control grows when the
/// label is long, it only widens.
pub const FLAG_H: i32 = 34;

/// Width a boolean control claims for its own part, before the label.
pub fn flag_control_w(tag: &str) -> i32 {
    if tag == "toggle" {
        TOGGLE_W
    } else {
        FLAG_BOX
    }
}

/// The number a `slider` is showing, and the grid it may rest on.
///
/// Four numbers rather than one because a slider has no other state: the
/// handle's position, the fill's length and the number handed to a handler
/// are all one thing seen at three moments, and letting them drift apart is
/// how a control ends up drawing a handle for a value it cannot produce.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderSpec {
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

impl SliderSpec {
    /// Where the pointer has to be for the handle to be at `value`: the whole
    /// track travels the whole range, inset by the handle's own radius so the
    /// handle cannot hang off either end and read as a different value.
    pub fn track_span(&self) -> (f32, f32) {
        let half = SLIDER_KNOB as f32 / 2.0;
        (half, (SLIDER_TRACK_W as f32 - half).max(half))
    }

    /// Handle centre x inside the track for a value. Pure, so the drawing, the
    /// drag's reverse mapping and every test read the same curve.
    pub fn value_to_x(&self, value: f64) -> f32 {
        let (lo, hi) = self.track_span();
        if self.max <= self.min {
            return lo;
        }
        let t = ((value - self.min) / (self.max - self.min)).clamp(0.0, 1.0);
        lo + t as f32 * (hi - lo)
    }

    /// The value a pointer at `x` inside the track reports, snapped onto the
    /// control's own grid. The inverse of `value_to_x` up to that snap, which
    /// is what lets a drag be stepped without the handle jumping.
    pub fn x_to_value(&self, x: f32) -> f64 {
        let (lo, hi) = self.track_span();
        if hi <= lo {
            return self.min;
        }
        let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
        crate::ui::slider_snap(
            self.min + t as f64 * (self.max - self.min),
            self.min,
            self.max,
            self.step,
        )
    }

    /// The value a drag by `dx` pixels from `value` lands on, snapped. Used
    /// with the grab offset recorded at press, so pressing the handle moves
    /// the number by as much as the finger did rather than teleporting it to
    /// wherever the pointer happened to be.
    pub fn value_after_dx(&self, dx: f32) -> f64 {
        self.x_to_value(self.value_to_x(self.value) + dx)
    }
}

/// One in-flight slider drag.
///
/// The grab offset is recorded rather than assuming the press was at the
/// handle's centre — otherwise the handle jumps the instant it is pressed,
/// which makes precise positioning impossible. The track's own left edge is
/// recorded too, so scrolling the form under a held handle cannot throw the
/// value by the amount the form moved.
#[derive(Debug, Clone, PartialEq)]
pub struct SliderDrag {
    /// How far the press landed from the handle's centre, in pixels. Signed,
    /// so a press on the handle's left edge does not throw the value the
    /// width of a handle to the right of the finger.
    pub grab_dx: f32,
    /// Screen-space x of the track's left edge when the press arrived.
    pub track_x: i32,
    /// The bounds, the step and the value the press landed on. The mapping
    /// from pointer to number is read from here, never re-derived from a tree
    /// that may have re-laid out mid-drag.
    pub origin: SliderSpec,
    /// The value last reported, so a drag that has not moved costs nothing.
    pub current: f64,
    /// The control's dispatch key — where the new number goes.
    pub id: String,
}

/// Dispatch key of a `slider`: its `id` when it has one, else the layout
/// order. Hover, the pressed tone and the drag all resolve it the same way,
/// so the control that lit up is the one that moves.
pub fn slider_key(lbox: &LayoutBox, index: usize) -> String {
    lbox.id
        .clone()
        .unwrap_or_else(|| format!("__slider_{index}"))
}

/// The track a `slider` draws: the full length, vertically centred in its row
/// and flush with the left edge, with the label to its right. Pure, so the
/// drawing, the press's hit test and the drag's arithmetic cannot disagree
/// about where the number lives.
pub fn slider_track_rect(row: Rect) -> Rect {
    Rect {
        x: row.x,
        // Never above its own row: a row shorter than the groove would
        // otherwise centre a negative offset and paint the control over
        // whatever sits above it.
        y: row.y + ((row.h - SLIDER_TRACK_H) / 2).max(0),
        w: SLIDER_TRACK_W,
        h: SLIDER_TRACK_H,
    }
}

/// Height of one row of an open choice list, in pixels.
pub const SELECT_ROW_H: i32 = 28;
/// Breathing room above the first row and below the last, so rows are not
/// glued to the card's border.
pub const SELECT_POP_PAD: i32 = 6;

/// The list an open `select` paints: its card, which end of the field it
/// hangs from, and how much of it the window can show. Owned, so the draw
/// pass can hold it while the surface is borrowed.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectPopupView {
    /// The open field's key, kept so a keyboard move can find its own state.
    pub key: String,
    /// Card in screen space, always inside the window.
    pub rect: Rect,
    pub options: Vec<String>,
    /// Index of the first row drawn, and how many rows fit.
    pub first: usize,
    pub rows: usize,
    /// Index of the row the keyboard is on — also what Enter commits.
    pub highlight: usize,
    /// What the field is currently showing, so the list can mark it.
    pub value: Option<String>,
    /// The field's dispatch id. `None` means the choice goes nowhere, and the
    /// list never opens in the first place.
    pub id: Option<String>,
}

/// Geometry of the list a `select` opens: below the field when the room is
/// there, above it when it is not, and clipped — never scrolled off the
/// window — when there is no room on either side. Pure, so the draw pass and
/// the click/keyboard hit tests cannot disagree about where a row is.
pub fn select_popup_rect(field: Rect, count: usize, win_w: i32, win_h: i32) -> Rect {
    let want_h = (count as i32) * SELECT_ROW_H + 2 * SELECT_POP_PAD;
    let w = field.w.clamp(0, win_w.max(0));
    let x = field.x.clamp(0, (win_w - w).max(0));
    let below_y = field.y + field.h;
    let space_below = (win_h - below_y).max(0);
    let space_above = field.y.max(0);
    let (y, avail) = if want_h <= space_below {
        (below_y, space_below)
    } else if want_h <= space_above {
        (field.y - want_h, space_above)
    } else if space_below >= space_above {
        (below_y, space_below)
    } else {
        (field.y - space_above, space_above)
    };
    let h = want_h.min(avail).max(0);
    Rect {
        x,
        y: y.clamp(0, (win_h - h).max(0)),
        w,
        h,
    }
}

/// How many whole rows a card of this height shows.
pub fn select_visible_rows(popup_h: i32) -> usize {
    ((popup_h - 2 * SELECT_POP_PAD) / SELECT_ROW_H).max(0) as usize
}

/// A list's first shown row, made legal: never past the end of the options.
/// The highlight is deliberately not dragged along — the wheel may scroll the
/// list away from it — so this is only a range clamp.
pub fn select_scroll_clamp(count: usize, visible: usize, scroll: usize) -> usize {
    scroll.min(count.saturating_sub(visible))
}

/// The smallest first shown row that still shows `highlight`, so a keyboard
/// move always lands on screen.
pub fn select_scroll_for_highlight(count: usize, visible: usize, highlight: usize) -> usize {
    if visible == 0 {
        return 0;
    }
    highlight
        .saturating_sub(visible - 1)
        .min(count.saturating_sub(visible))
}

/// Which option a pointer position sits on — `None` for the padding, the
/// thumb gutter, or outside the card.
pub fn select_row_at(popup: Rect, count: usize, first: usize, pos: (i32, i32)) -> Option<usize> {
    if !popup.contains(pos.0, pos.1) {
        return None;
    }
    let rel_y = pos.1 - popup.y - SELECT_POP_PAD;
    if rel_y < 0 {
        return None;
    }
    let row = (rel_y / SELECT_ROW_H) as usize;
    if row >= select_visible_rows(popup.h) {
        return None;
    }
    let idx = first + row;
    (idx < count).then_some(idx)
}

/// The `select` box with this dispatch key, with its layout order. Hover and
/// the open list both resolve through `select_key`, so an id-less field whose
/// order shifted still finds itself.
pub fn find_select_box<'a>(boxes: &'a [LayoutBox], key: &str) -> Option<(usize, &'a LayoutBox)> {
    boxes
        .iter()
        .enumerate()
        .find(|(i, lbox)| lbox.tag == "select" && select_key(lbox, *i) == key)
}

/// One open choice list, and where the keyboard is inside it. Keyed the same
/// way hover is, so a re-layout that reorders boxes cannot leave a list
/// attached to the wrong control.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenSelect {
    pub key: String,
    /// Index of the option the keyboard is on; also what Enter commits, and
    /// what the pointer moves to as it passes over rows.
    pub highlight: usize,
    /// Index of the first option drawn, for lists taller than the room left.
    pub scroll: usize,
}

/// The two vertical gradient ends for a button. `hover_t` is how far the
/// control has travelled from resting (`0.0`) to hovered (`1.0`); the resting
/// ramp Blue-400 → Blue-500 and the hovered ramp Blue-500 → Blue-600 are the
/// two ends of that blend, so an in-between frame is simply both channels
/// lerped by the same amount and the state change reads as the whole surface
/// shifting rather than one flat tone being swapped. Pressing drops one more
/// rung to Blue-600 → Blue-700, instantly — press is feedback, not decoration.
/// Disabled is not a fourth colour: it is the resting pair washed toward the
/// window background, keeping the hue while removing the call to action.
pub fn button_tones(theme: &Theme, hover_t: f32, pressed: bool, disabled: bool) -> (u32, u32) {
    if disabled {
        (
            lerp_color(theme.button_bg_top, theme.bg, 0.62),
            lerp_color(theme.button_bg, theme.bg, 0.62),
        )
    } else if pressed {
        (theme.button_bg_hover, theme.button_bg_pressed)
    } else {
        let t = hover_t.clamp(0.0, 1.0);
        (
            lerp_color(theme.button_bg_top, theme.button_bg, t),
            lerp_color(theme.button_bg, theme.button_bg_hover, t),
        )
    }
}

/// Helper to determine which button in `boxes` contains the cursor coordinates `pos`.
///
/// Disabled buttons are skipped: they must neither take the pointer cursor nor
/// be reportable as pressed. Note they still *absorb* clicks in
/// [`NativeApp::handle_click`], so a click on one cannot fall through to a
/// control behind it.
pub fn find_hovered_button_in_boxes(
    boxes: &[LayoutBox],
    scroll_x: i32,
    scroll_y: i32,
    pos: (i32, i32),
) -> Option<String> {
    let (mx, my) = pos;
    for (i, lbox) in boxes.iter().enumerate() {
        // A `select` is hoverable like a button — it opens on a press — so it
        // earns the same pointer cursor and hover ramp. Its key comes from
        // `select_key` rather than `button_key`, because a choice field has no
        // `on_click` to fall back on and the field's own arm reads hover under
        // that same name.
        // A boolean control hovers like a button too — a press flips it — but
        // only when it has an `id`: one with nowhere to report the new value
        // is decoration, and lighting up a control that will not answer the
        // click is a lie told in blue. A `slider` follows the same rule for
        // the same reason: a dial whose moves go nowhere is decoration, and
        // the handle lights only if the drag will be answered.
        let is_flag = matches!(lbox.tag.as_str(), "checkbox" | "toggle") && lbox.id.is_some();
        let is_slider = lbox.tag == "slider" && lbox.id.is_some();
        let is_tabs = lbox.tag == "tabs" && lbox.id.is_some() && !lbox.options.is_empty();
        let is_control = lbox.tag == "button"
            || is_flag
            || is_slider
            || is_tabs
            || (lbox.tag == "select" && lbox.id.is_some() && !lbox.options.is_empty());
        if is_control && !lbox.disabled {
            // Pan-aware screen rect; a button scrolled outside its scroll
            // viewport is not hoverable even where its rect still overlaps.
            let screen_rect = lbox.screen_rect(scroll_x, scroll_y);
            let clip_ok = lbox
                .screen_clip(scroll_x, scroll_y)
                .is_none_or(|c| c.contains(mx, my));
            if screen_rect.contains(mx, my) && clip_ok {
                let key = if lbox.tag == "select" {
                    select_key(lbox, i)
                } else if is_flag {
                    flag_key(lbox, i)
                } else if is_slider {
                    slider_key(lbox, i)
                } else {
                    button_key(lbox, i)
                };
                return Some(key);
            }
        }
    }
    None
}

/// The box whose `tooltip` should be showing, if the cursor sits on one. Used
/// for the overlay pass and for the pointer/text cursor decision, so any tag —
/// not just buttons — can carry a label. Iterated newest-first so that when
/// boxes overlap, the one drawn last (i.e. on top) wins.
pub fn find_tooltip_in_boxes(
    boxes: &[LayoutBox],
    scroll_x: i32,
    scroll_y: i32,
    width: i32,
    height: i32,
    pos: (i32, i32),
) -> Option<String> {
    let (mx, my) = pos;
    for lbox in boxes.iter().rev() {
        if lbox.tooltip.is_none() {
            continue;
        }
        if !lbox.visible_in(scroll_x, scroll_y, width, height) {
            continue;
        }
        if lbox.screen_rect(scroll_x, scroll_y).contains(mx, my)
            && lbox
                .screen_clip(scroll_x, scroll_y)
                .is_none_or(|c| c.contains(mx, my))
        {
            return lbox.tooltip.clone();
        }
    }
    None
}

/// The interactive text field under the cursor, if any. Disabled fields are
/// skipped so they keep the arrow instead of claiming the I-beam. Window
/// bounds matter here: a field scrolled past the window edge still has a
/// screen rectangle.
pub fn input_under_cursor(
    boxes: &[LayoutBox],
    scroll_x: i32,
    scroll_y: i32,
    width: i32,
    height: i32,
    pos: (i32, i32),
) -> bool {
    let (mx, my) = pos;
    for lbox in boxes.iter().rev() {
        if lbox.tag != "input" || lbox.disabled {
            continue;
        }
        if !lbox.visible_in(scroll_x, scroll_y, width, height) {
            continue;
        }
        if lbox.screen_rect(scroll_x, scroll_y).contains(mx, my)
            && lbox
                .screen_clip(scroll_x, scroll_y)
                .is_none_or(|c| c.contains(mx, my))
        {
            return true;
        }
    }
    false
}

/// The `(fill, border)` pair for a text field. Focus is what the border
/// signals; disabled is not a separate palette entry but the resting surface
/// washed halfway to the window background, the same treatment a disabled
/// button gets, so the two read as one idea rather than two coincidences. An
/// inert field is checked first so it can never borrow the focus ring.
pub fn input_surface(theme: &Theme, focused: bool, disabled: bool) -> (u32, u32) {
    if disabled {
        (lerp_color(theme.input_bg, theme.bg, 0.5), theme.border)
    } else if focused {
        (theme.input_bg, theme.input_border_focused)
    } else {
        (theme.input_bg, theme.input_border)
    }
}

// -----------------------------------------------------------------------------
// Application Handler and Event Loop
// -----------------------------------------------------------------------------

/// A cursor position that is not on any control in the tree.
///
/// Far enough outside that no box can be laid out there, so every "what is
/// under the pointer?" question answers the same empty way. It is used when
/// the pointer leaves the window: the platform stops sending moves, and a
/// position left parked on the last control would keep that control lit — and
/// its tooltip card painted — under a pointer that is no longer there.
const POINTER_OUTSIDE: (i32, i32) = (-100_000, -100_000);

// -----------------------------------------------------------------------------

/// Which cursor shape the window should be showing. A small enum of its own
/// rather than `CursorIcon` itself purely so it compares cheaply: mouse-move
/// events arrive several per pixel of travel and the window API should only be
/// touched when the shape actually changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    /// Nothing interactive under the cursor — the platform's usual arrow.
    Default,
    /// A live (non-disabled) button: the pointing hand.
    Pointer,
    /// A text field: the I-beam.
    Text,
    /// Over a scrollbar thumb, ready to take it: the open hand.
    Grab,
    /// A scrollbar thumb is being dragged: the closed hand.
    Grabbing,
}

impl CursorKind {
    fn icon(self) -> CursorIcon {
        match self {
            CursorKind::Default => CursorIcon::Default,
            CursorKind::Pointer => CursorIcon::Pointer,
            CursorKind::Text => CursorIcon::Text,
            CursorKind::Grab => CursorIcon::Grab,
            CursorKind::Grabbing => CursorIcon::Grabbing,
        }
    }
}

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
    /// Key of the button the left button went down on, held until release. It
    /// is resolved with the same helper hover uses, so the box that shows the
    /// pressed tone is the one that will dispatch.
    pub pressed_id: Option<String>,
    /// Whether a tooltip is on screen, so leaving a labelled box can be told
    /// apart from merely moving within one — only the exit needs a repaint to
    /// erase the card.
    pub tooltip_shown: bool,
    /// Cursor shape last pushed to the window, so unchanged moves cost nothing.
    pub cursor_kind: CursorKind,
    /// The window's single timer: the earliest moment anything timed is due.
    /// Idle means the event loop sleeps in `ControlFlow::Wait`.
    clock: FrameClock,
    /// How far each button has travelled toward its hovered tone, keyed by the
    /// same `button_key` the render loop resolves: `0.0` resting, `1.0`
    /// hovered. Entries fade to zero and are dropped, so only controls that
    /// are mid-ramp or still under the pointer are held.
    hover_ramps: HashMap<String, f32>,
    /// When the caret last moved. The blink phase derives from it, so typing
    /// or clicking restarts the visible phase instead of landing mid-blink.
    caret_blink_origin: Option<Instant>,
    /// When an active spinner was first laid out. The continuous rotation
    /// angle derives from it, and is dropped when no spinner remains visible.
    spinner_origin: Option<Instant>,
    /// When the previous animation tick ran, so the next one can measure `dt`.
    last_tick: Option<Instant>,
    /// Which window scrollbar the pointer is currently over, driving the hover
    /// highlight. Re-derived from geometry rather than remembered across
    /// layouts, so a resize cannot leave a stale bar lit.
    scrollbar_hover: Option<ScrollbarAxis>,
    /// Set while the pointer is down on a scrollbar: the axis being dragged,
    /// the grab point in track-local pixels (where inside the thumb the press
    /// landed, so the thumb does not jump to the pointer's centre), and the
    /// offset the drag started from.
    scrollbar_drag: Option<(ScrollbarAxis, i32, i32)>,
    pub region_scrollbar_drag: Option<(String, i32, i32, i32, i32)>,
    /// Set while the pointer is down on a slider: the control's dispatch key,
    /// the grab offset (how far the press landed from the handle's centre, so
    /// pressing the handle moves the number by as much as the finger did
    /// instead of teleporting it to the pointer), and the value that offset
    /// was measured against. Keyed the same way hover is, so the control that
    /// lit up is the one that moves.
    slider_drag: Option<SliderDrag>,
    pub layout_boxes: Vec<LayoutBox>,
    pub render_count: usize,
    pub test_runner: Option<fn(&mut NativeApp<D>, &ActiveEventLoop)>,
    pub focused_id: Option<String>,
    /// The `select` whose list is on screen, and where the keyboard is inside
    /// it. `None` means every list is shut. The list is an overlay — never a
    /// layout box — so this is the whole of its state.
    pub open_select: Option<OpenSelect>,
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
    /// Cache of decoded PNG images keyed by src URL or file path.
    pub image_cache: HashMap<String, crate::ui::DecodedImage>,
    /// State of the modal Command Palette (`Ctrl+K`).
    pub command_palette: Option<CommandPaletteState>,
    /// State of active drag-and-drop reorder operation.
    pub drag_state: Option<DragState>,
    /// Path of the most recently dropped OS file.
    pub last_dropped_file: Option<String>,
    /// Path of OS file currently hovered over window.
    pub hovered_file: Option<String>,
    /// State of the active right-click Context Menu.
    pub context_menu: Option<ContextMenuState>,
    /// Active UI locale (English / Turkish).
    pub locale: Locale,
    /// State of the modal Settings Dialog window.
    pub settings_dialog: SettingsDialogState,
    /// Floating toast notification.
    pub toast: Option<ToastNotification>,
    /// Last rendered framebuffer (for PNG export and inspection).
    pub last_frame: Vec<u32>,
    /// Path of the most recently exported chat file.
    pub last_exported_file: Option<String>,
    /// Active secondary OS windows (e.g. popout chat or multi-window sessions).
    pub secondary_windows: HashMap<WindowId, SecondaryWindow>,
    /// Pending window spawn requests.
    pub spawned_window_requests: Vec<WindowConfig>,
    /// Active error banner state (Section 5.1).
    pub current_error: Option<UiErrorState>,
    /// Background / async LLM execution and cancellation state (Section 5.2).
    pub async_state: AsyncExecutionState,
    /// Partial redraw dirty rectangle tracker (Section 5.5).
    pub dirty_rect: Option<Rect>,
    /// Sidebar visibility state (Section 5.4).
    pub sidebar_open: bool,
    /// Target and animated zoom scale percentage (Issue #262).
    pub target_zoom_pct: f32,
    pub current_zoom_pct: f32,
    /// Watch-Only Jarvis HUD overlay mode (RFC-2026-SOVEREIGN-HUD).
    pub hud_mode: bool,
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
            pressed_id: None,
            tooltip_shown: false,
            // Matches winit's own starting state, so the first shape is only
            // pushed once it differs from the platform default.
            cursor_kind: CursorKind::Default,
            clock: FrameClock::new(),
            hover_ramps: HashMap::new(),
            caret_blink_origin: None,
            spinner_origin: None,
            last_tick: None,
            scrollbar_hover: None,
            scrollbar_drag: None,
            region_scrollbar_drag: None,
            slider_drag: None,
            layout_boxes: Vec::new(),
            render_count: 0,
            test_runner: None,
            focused_id: None,
            open_select: None,
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
            image_cache: HashMap::new(),
            command_palette: None,
            drag_state: None,
            last_dropped_file: None,
            hovered_file: None,
            context_menu: None,
            locale: Locale::En,
            settings_dialog: SettingsDialogState::default(),
            toast: None,
            last_frame: Vec::new(),
            last_exported_file: None,
            secondary_windows: HashMap::new(),
            spawned_window_requests: Vec::new(),
            current_error: None,
            async_state: AsyncExecutionState::default(),
            dirty_rect: None,
            sidebar_open: true,
            target_zoom_pct: 100.0,
            current_zoom_pct: 100.0,
            hud_mode: false,
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

    /// Finds draggable box at screen position `pos`.
    pub fn draggable_box_at(&self, pos: (i32, i32)) -> Option<&LayoutBox> {
        let (mx, my) = pos;
        for lbox in self.layout_boxes.iter().rev() {
            if lbox.draggable && !lbox.disabled && lbox.id.is_some() {
                let screen_rect = lbox.screen_rect(self.scroll_x, self.scroll_y);
                let clip_ok = lbox
                    .screen_clip(self.scroll_x, self.scroll_y)
                    .is_none_or(|c| c.contains(mx, my));
                if screen_rect.contains(mx, my) && clip_ok {
                    return Some(lbox);
                }
            }
        }
        None
    }

    /// Finds drop target box under position `pos` excluding `source_id`.
    pub fn drop_target_at(&self, pos: (i32, i32), source_id: &str) -> Option<&LayoutBox> {
        let (mx, my) = pos;
        for lbox in self.layout_boxes.iter().rev() {
            if lbox.draggable && !lbox.disabled {
                if let Some(ref id) = lbox.id {
                    if id != source_id {
                        let screen_rect = lbox.screen_rect(self.scroll_x, self.scroll_y);
                        let clip_ok = lbox
                            .screen_clip(self.scroll_x, self.scroll_y)
                            .is_none_or(|c| c.contains(mx, my));
                        if screen_rect.contains(mx, my) && clip_ok {
                            return Some(lbox);
                        }
                    }
                }
            }
        }
        None
    }

    /// Programmatically simulates a drag-and-drop reorder operation.
    pub fn simulate_drag_and_drop(&mut self, source_id: &str, target_id: &str) {
        let reorder_key = format!("Reorder:{source_id}->{target_id}");
        self.dispatcher
            .dispatch_change(source_id, &format!("reorder:{target_id}"));
        self.dispatcher.dispatch_key(&reorder_key, None);
        self.render();
    }

    /// Programmatically simulates an OS file drop.
    pub fn simulate_file_drop(&mut self, path_str: &str) {
        self.last_dropped_file = Some(path_str.to_string());
        self.hovered_file = None;
        self.dispatcher.dispatch_change("file_drop", path_str);
        self.dispatcher
            .dispatch_key(&format!("FileDrop:{path_str}"), None);
        self.render();
    }

    /// Programmatically triggers a context menu action.
    pub fn trigger_context_menu_action(&mut self, action_id: &str, target_id: &str) {
        self.dispatcher
            .dispatch_change(target_id, &format!("context:{action_id}"));
        self.dispatcher
            .dispatch_key(&format!("ContextMenu:{action_id}:{target_id}"), None);
        self.render();
    }

    /// Opens the modal Settings Dialog window.
    pub fn open_settings_dialog(&mut self) {
        self.settings_dialog.open = true;
        self.dispatcher.dispatch_key("OpenSettings", None);
        self.render();
    }

    /// Closes the modal Settings Dialog window.
    pub fn close_settings_dialog(&mut self) {
        self.settings_dialog.open = false;
        self.dispatcher.dispatch_key("CloseSettings", None);
        self.render();
    }

    /// Sets the UI locale with immediate toast notification and event dispatch.
    pub fn set_locale(&mut self, locale: Locale) {
        self.locale = locale;
        self.settings_dialog.locale = locale;
        let label = locale.label();
        self.show_toast(&format!("Language / Dil: {label}"), "info");
        self.dispatcher
            .dispatch_key(&format!("LocaleChange:{:?}", locale), None);
        self.dispatcher.dispatch_change("locale", locale.code());
        self.render();
    }

    /// Cycles through supported languages instantly (<1ms).
    pub fn toggle_locale(&mut self) {
        let next = self.locale.next();
        self.set_locale(next);
    }

    /// Fast translation lookup for a string key in current locale.
    pub fn t<'a>(&self, key: &'a str) -> &'a str {
        let res = self.locale.get(key);
        if res == key {
            key
        } else {
            res
        }
    }

    /// Builds the complete semantic accessibility tree for screen readers and automated testing.
    pub fn build_accessible_tree(&self) -> AccessibleNode {
        let mut children = Vec::new();

        for lbox in &self.layout_boxes {
            let role = match lbox.tag.as_str() {
                "button" => AccessibleRole::Button,
                "input" => AccessibleRole::TextInput,
                "checkbox" | "toggle" => AccessibleRole::Checkbox,
                "slider" => AccessibleRole::Slider,
                "select" => AccessibleRole::Select,
                "text" => AccessibleRole::StaticText,
                "status_line" => AccessibleRole::StatusLine,
                "code_block" => AccessibleRole::CodeBlock,
                "scroll" => AccessibleRole::ScrollArea,
                _ => AccessibleRole::Unknown,
            };

            let name = lbox
                .text
                .clone()
                .or_else(|| lbox.tooltip.clone())
                .or_else(|| lbox.id.clone())
                .unwrap_or_default();

            let value = if lbox.tag == "input" {
                lbox.id
                    .as_deref()
                    .and_then(|id| self.text_inputs.get(id).map(|st| st.text.clone()))
                    .or_else(|| lbox.text.clone())
            } else if lbox.tag == "slider" {
                lbox.slider.as_ref().map(|s| s.value.to_string())
            } else {
                None
            };

            let focused = lbox
                .id
                .as_ref()
                .is_some_and(|id| self.focused_id.as_deref() == Some(id));
            let checked = if matches!(lbox.tag.as_str(), "checkbox" | "toggle") {
                Some(lbox.checked)
            } else {
                None
            };

            children.push(AccessibleNode {
                id: lbox.id.clone(),
                role,
                name,
                value,
                rect: lbox.rect,
                disabled: lbox.disabled,
                focused,
                checked,
                children: Vec::new(),
            });
        }

        if self.settings_dialog.open {
            children.push(AccessibleNode {
                id: Some("settings_dialog".into()),
                role: AccessibleRole::Dialog,
                name: "Settings & Preferences".into(),
                value: None,
                rect: Rect {
                    x: (self.width as i32 - 420) / 2,
                    y: (self.height as i32 - 300) / 2,
                    w: 420,
                    h: 300,
                },
                disabled: false,
                focused: true,
                checked: None,
                children: vec![AccessibleNode {
                    id: Some("settings_tab".into()),
                    role: AccessibleRole::ListItem,
                    name: match self.settings_dialog.active_tab {
                        0 => "General",
                        1 => "Model",
                        _ => "About",
                    }
                    .into(),
                    value: Some(self.settings_dialog.active_tab.to_string()),
                    rect: Rect {
                        x: 0,
                        y: 0,
                        w: 90,
                        h: 26,
                    },
                    disabled: false,
                    focused: false,
                    checked: None,
                    children: Vec::new(),
                }],
            });
        }

        AccessibleNode {
            id: Some("main_window".into()),
            role: AccessibleRole::Window,
            name: self.config.title.clone(),
            value: None,
            rect: Rect {
                x: 0,
                y: 0,
                w: self.width as i32,
                h: self.height as i32,
            },
            disabled: false,
            focused: self.focused_id.is_none(),
            checked: None,
            children,
        }
    }

    /// Requests spawning a separate chat window.
    pub fn request_spawn_window(
        &mut self,
        title: &str,
        width: u32,
        height: u32,
        _chat_id: Option<String>,
    ) {
        self.spawned_window_requests.push(WindowConfig {
            title: title.to_string(),
            width,
            height,
        });
        self.dispatcher
            .dispatch_key(&format!("SpawnWindow:{title}"), None);
        self.dispatcher.dispatch_change("window_spawned", title);
        self.show_toast(&format!("Opened \"{title}\" in new window"), "success");
        self.render();
    }

    /// Spawns a secondary OS window using the active event loop.
    pub fn spawn_secondary_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        config: WindowConfig,
        chat_id: Option<String>,
    ) -> Result<WindowId, String> {
        let win_attrs = WindowAttributes::default()
            .with_title(&config.title)
            .with_inner_size(LogicalSize::new(config.width, config.height));

        let window = event_loop
            .create_window(win_attrs)
            .map_err(|e| format!("failed to spawn window: {e}"))?;
        let window = Arc::new(window);
        let id = window.id();

        let context = match &self.context {
            Some(c) => c,
            None => return Err("graphics context uninitialized".into()),
        };

        let surface = softbuffer::Surface::new(context, window.clone())
            .map_err(|e| format!("failed to create surface: {e}"))?;

        let mut boxes = Vec::new();
        let msg = format!("Separate Chat: {}", config.title);
        boxes.push(LayoutBox {
            rect: Rect {
                x: 20,
                y: 50,
                w: (config.width as i32 - 40).max(100),
                h: 48,
            },
            tag: "bubble:assistant".into(),
            id: Some("sec_msg".into()),
            text: Some(msg),
            on_click: None,
            on_change: None,
            on_key: None,
            pan: (0, 0),
            clips: None,
            pad: 12,
            checked: false,
            slider: None,
            disabled: false,
            tooltip: None,
            options: Vec::new(),
            draggable: false,
            props: std::collections::HashMap::new(),
        });

        let mut sec = SecondaryWindow {
            id,
            title: config.title.clone(),
            width: config.width,
            height: config.height,
            window,
            surface,
            chat_session_id: chat_id,
            layout_boxes: boxes,
            last_frame: Vec::new(),
        };
        sec.render(&self.fonts, &self.theme);
        self.secondary_windows.insert(id, sec);
        self.dispatcher
            .dispatch_key(&format!("SpawnWindow:{}", config.title), None);
        self.dispatcher
            .dispatch_change("window_spawned", &config.title);
        self.show_toast(
            &format!("Opened \"{}\" in new window", config.title),
            "success",
        );
        Ok(id)
    }

    /// Displays a floating toast notification.
    pub fn show_toast(&mut self, message: &str, status: &str) {
        self.toast = Some(ToastNotification {
            message: message.to_string(),
            status: status.to_string(),
        });
        self.render();
    }

    /// Programmatically copies a code block to clipboard and displays toast.
    pub fn simulate_copy_code(&mut self, code_id: &str) {
        if let Some(lbox) = self
            .layout_boxes
            .iter()
            .find(|b| b.id.as_deref() == Some(code_id) && b.tag == "code_block")
        {
            let text = lbox.text.clone().unwrap_or_default();
            self.clipboard = text;
            self.show_toast("Code copied to clipboard!", "success");
            self.dispatcher
                .dispatch_key(&format!("CopyCode:{code_id}"), None);
            self.dispatcher.dispatch_change(code_id, "copied");
            self.render();
        }
    }

    /// Programmatically pastes text into the focused input or composer,
    /// normalizing rich text formatting.
    pub fn paste_text(&mut self, text: &str) {
        let normalized = normalize_rich_text_paste(text);
        self.clipboard = normalized.clone();
        if let Some(focused_id) = self.focused_id.clone() {
            let initial = self
                .layout_boxes
                .iter()
                .find(|b| b.id.as_deref() == Some(&focused_id))
                .and_then(|b| b.text.clone())
                .unwrap_or_default();
            let st = self.ensure_input_state(&focused_id, &initial);
            st.paste(&normalized);
            let new_val = st.text.clone();
            self.dispatcher.dispatch_change(&focused_id, &new_val);
            self.render();
        }
    }

    /// Collects all message bubbles and code blocks from the current layout.
    pub fn collect_chat_messages(&self) -> Vec<ChatMessageExport> {
        let mut messages = Vec::new();
        for b in &self.layout_boxes {
            if b.tag.starts_with("bubble") {
                let role = if b.tag.contains("user") {
                    "user"
                } else if b.tag.contains("assistant")
                    || b.tag.contains("ai")
                    || b.tag.contains("nudge")
                {
                    "assistant"
                } else {
                    "system"
                };
                let content = b.text.clone().unwrap_or_default();
                messages.push(ChatMessageExport {
                    role: role.into(),
                    content,
                });
            } else if b.tag == "code_block" {
                if let Some(ref text) = b.text {
                    messages.push(ChatMessageExport {
                        role: "code".into(),
                        content: format!("```\n{}\n```", text),
                    });
                }
            }
        }
        messages
    }

    /// Exports the current chat as a Markdown formatted string.
    pub fn export_chat_markdown(&self) -> String {
        let msgs = self.collect_chat_messages();
        let mut md = format!("# {}\n\n", self.config.title);
        for m in msgs {
            let role_label = match m.role.as_str() {
                "user" => "### User",
                "assistant" => "### Assistant",
                "code" => "### Code Block",
                _ => "### System",
            };
            md.push_str(&format!("{role_label}\n\n{}\n\n", m.content));
        }
        md
    }

    /// Exports the current chat as a formatted JSON string.
    pub fn export_chat_json(&self) -> Result<String, String> {
        let export = ChatExportData {
            title: self.config.title.clone(),
            messages: self.collect_chat_messages(),
        };
        serde_json::to_string_pretty(&export).map_err(|e| e.to_string())
    }

    /// Exports the window's current pixel buffer snapshot directly as a PNG file.
    pub fn export_chat_png(&mut self, path: &str) -> Result<(), String> {
        let w = self.width.max(1);
        let h = self.height.max(1);
        let stride = w as usize;
        let h_usize = h as usize;
        let total = stride * h_usize;

        let frame = if !self.last_frame.is_empty() && self.last_frame.len() >= total {
            self.last_frame.clone()
        } else {
            let mut buf = vec![self.theme.bg; total];
            for lbox in &self.layout_boxes {
                let rx = lbox.rect.x - self.scroll_x - lbox.pan.0;
                let ry = lbox.rect.y - self.scroll_y - lbox.pan.1;
                draw_rounded_rect_box(
                    &mut buf,
                    stride,
                    h_usize,
                    rx,
                    ry,
                    lbox.rect.w,
                    lbox.rect.h,
                    6,
                    self.theme.bubble_user_bg,
                    self.theme.border,
                );
                if let Some(ref text) = lbox.text {
                    let fonts = &self.fonts;
                    draw_text_ttf_clipped(
                        fonts,
                        FaceKind::Ui,
                        rx + 12,
                        ry + 8,
                        text,
                        Rect {
                            x: rx,
                            y: ry,
                            w: lbox.rect.w,
                            h: lbox.rect.h,
                        },
                        |px, py, w, h, cov| {
                            draw_rect_alpha(
                                &mut buf,
                                stride,
                                h_usize,
                                px,
                                py,
                                w,
                                h,
                                self.theme.text,
                                cov,
                            );
                        },
                    );
                }
            }
            buf
        };

        let mut rgba = Vec::with_capacity(total * 4);
        for &px in &frame[..total] {
            let r = ((px >> 16) & 0xFF) as u8;
            let g = ((px >> 8) & 0xFF) as u8;
            let b = (px & 0xFF) as u8;
            rgba.extend_from_slice(&[r, g, b, 255]);
        }

        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut w_buf = std::io::BufWriter::new(file);
        let mut encoder = png::Encoder::new(&mut w_buf, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Exports the chat to the given file path in Markdown, JSON, or PNG format.
    pub fn export_chat_to_file(&mut self, format: &str, path: &str) -> Result<String, String> {
        self.last_exported_file = Some(path.to_string());
        let res = match format.to_lowercase().as_str() {
            "md" | "markdown" => {
                let md = self.export_chat_markdown();
                std::fs::write(path, &md).map_err(|e| e.to_string())?;
                self.show_toast(&format!("Exported Markdown to {path}"), "success");
                self.dispatcher
                    .dispatch_key(&format!("ChatExport:md:{path}"), None);
                self.dispatcher.dispatch_change("chat_export", path);
                Ok(md)
            }
            "json" => {
                let js = self.export_chat_json()?;
                std::fs::write(path, &js).map_err(|e| e.to_string())?;
                self.show_toast(&format!("Exported JSON to {path}"), "success");
                self.dispatcher
                    .dispatch_key(&format!("ChatExport:json:{path}"), None);
                self.dispatcher.dispatch_change("chat_export", path);
                Ok(js)
            }
            "png" => {
                self.export_chat_png(path)?;
                self.show_toast(&format!("Exported PNG snapshot to {path}"), "success");
                self.dispatcher
                    .dispatch_key(&format!("ChatExport:png:{path}"), None);
                self.dispatcher.dispatch_change("chat_export", path);
                Ok(path.to_string())
            }
            _ => Err(format!("Unsupported export format: {format}")),
        };
        self.render();
        res
    }

    /// Sets target zoom percentage with animated convergence (Issue #262).
    pub fn set_zoom_target(&mut self, target_pct: f32) {
        self.target_zoom_pct = target_pct.clamp(50.0, 300.0);
    }

    /// Steps zoom animation towards target zoom percentage.
    pub fn step_zoom_animation(&mut self, factor: f32) {
        let diff = self.target_zoom_pct - self.current_zoom_pct;
        if diff.abs() > 0.1 {
            self.current_zoom_pct += diff * factor.clamp(0.0, 1.0);
        } else {
            self.current_zoom_pct = self.target_zoom_pct;
        }
    }

    // =========================================================================
    // SECTION 5: Error capture, Async LLM, Persistence, and Partial Redraw
    // =========================================================================

    /// Sets the active UI error banner (Section 5.1).
    pub fn set_error(
        &mut self,
        message: &str,
        error_type: &str,
        retryable: bool,
        action_id: Option<String>,
    ) {
        self.current_error = Some(UiErrorState {
            message: message.to_string(),
            error_type: error_type.to_string(),
            retryable,
            action_id,
        });
        self.dispatcher
            .dispatch_key(&format!("ErrorShown:{error_type}"), None);
        self.dispatcher.dispatch_change("error_state", error_type);
        self.render();
    }

    /// Clears and dismisses the active error banner.
    pub fn clear_error(&mut self) {
        if self.current_error.take().is_some() {
            self.dispatcher.dispatch_key("ErrorDismissed", None);
            self.dispatcher.dispatch_change("error_state", "none");
            self.render();
        }
    }

    /// Triggers retry for a retryable error.
    pub fn trigger_error_retry(&mut self) {
        if let Some(err) = self.current_error.take() {
            self.dispatcher.dispatch_key("ErrorRetry", None);
            if let Some(ref aid) = err.action_id {
                self.dispatcher.dispatch_key(&format!("Action:{aid}"), None);
            }
            self.dispatcher.dispatch_change("error_state", "retry");
            self.show_toast("Retrying operation...", "info");
            self.render();
        }
    }

    /// Saves an input draft record for crash recovery (Section 5.1).
    pub fn save_draft_recovery(&self, input_id: &str, text: &str) {
        let record = DraftRecoveryRecord {
            draft: text.to_string(),
            input_id: input_id.to_string(),
            timestamp_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };
        let dir = std::env::temp_dir().join("nudge_drafts");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("draft_{input_id}.json"));
        if let Ok(js) = serde_json::to_string_pretty(&record) {
            let _ = std::fs::write(path, js);
        }
    }

    /// Restores a saved input draft record if one exists.
    pub fn load_draft_recovery(&self, input_id: &str) -> Option<String> {
        let path = std::env::temp_dir()
            .join("nudge_drafts")
            .join(format!("draft_{input_id}.json"));
        let content = std::fs::read_to_string(path).ok()?;
        let record: DraftRecoveryRecord = serde_json::from_str(&content).ok()?;
        Some(record.draft)
    }

    /// Starts a background async LLM or tool execution task with progress & status (Section 5.2).
    pub fn start_async_task(&mut self, task_id: &str, status: &str) {
        self.async_state = AsyncExecutionState {
            in_flight: true,
            status_text: status.to_string(),
            task_id: Some(task_id.to_string()),
            progress_percent: Some(0.0),
            chunks_received: 0,
        };
        if self.spinner_origin.is_none() {
            self.spinner_origin = Some(Instant::now());
        }
        self.dispatcher.dispatch_change("async_task", task_id);
        self.dispatcher
            .dispatch_key(&format!("TaskStarted:{task_id}"), None);
        self.render();
    }

    /// Receives a live chunk during async streaming execution.
    pub fn update_async_chunk(&mut self, chunk_text: &str) {
        if self.async_state.in_flight {
            self.async_state.chunks_received += 1;
            self.dispatcher.dispatch_change("stream_chunk", chunk_text);
            self.render();
        }
    }

    /// Cancels the currently executing async task via Stop / Cancel button.
    pub fn cancel_async_task(&mut self) {
        if let Some(task_id) = self.async_state.task_id.take() {
            self.async_state.in_flight = false;
            self.show_toast("Execution cancelled.", "info");
            self.dispatcher
                .dispatch_key(&format!("TaskCancelled:{task_id}"), None);
            self.dispatcher.dispatch_change("task_cancel", &task_id);
            self.render();
        }
    }

    /// Completes the currently executing async task.
    pub fn finish_async_task(&mut self, result_text: &str) {
        if let Some(task_id) = self.async_state.task_id.take() {
            self.async_state.in_flight = false;
            self.dispatcher
                .dispatch_key(&format!("TaskFinished:{task_id}"), None);
            self.dispatcher.dispatch_change(&task_id, result_text);
            self.render();
        }
    }

    /// Saves window state (size, theme, locale, sidebar) to JSON file (Section 5.4).
    pub fn save_window_state(&self, path: &str) -> Result<(), String> {
        let state = PersistedWindowState {
            width: self.width,
            height: self.height,
            x: None,
            y: None,
            dark_theme: self.theme.is_dark(),
            locale: format!("{:?}", self.locale).to_lowercase(),
            sidebar_open: self.sidebar_open,
        };
        let js = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
        std::fs::write(path, js).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Loads window state from JSON file (Section 5.4).
    pub fn load_window_state(&mut self, path: &str) -> Result<PersistedWindowState, String> {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let state: PersistedWindowState =
            serde_json::from_str(&content).map_err(|e| e.to_string())?;
        self.apply_persisted_state(&state);
        Ok(state)
    }

    /// Applies loaded window state to the current app.
    pub fn apply_persisted_state(&mut self, state: &PersistedWindowState) {
        self.width = state.width.max(200);
        self.height = state.height.max(150);
        self.sidebar_open = state.sidebar_open;
        if state.dark_theme {
            self.theme = Theme::DARK;
        } else {
            self.theme = Theme::LIGHT;
        }
        self.locale = match state.locale.as_str() {
            "tr" => Locale::Tr,
            _ => Locale::En,
        };
        self.render();
    }

    /// Toggles sidebar visibility (Section 5.4).
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_open = !self.sidebar_open;
        let open_str = if self.sidebar_open { "true" } else { "false" };
        self.dispatcher.dispatch_change("sidebar_open", open_str);
        self.dispatcher
            .dispatch_key(&format!("SidebarToggled:{}", self.sidebar_open), None);
        self.render();
    }

    /// Marks a bounding box as dirty for partial redraw (Section 5.5).
    pub fn mark_dirty(&mut self, rect: Rect) {
        self.dirty_rect = Some(match self.dirty_rect {
            Some(cur) => cur.union(&rect),
            None => rect,
        });
    }

    /// Clears the dirty rect tracker once redraw is completed.
    pub fn clear_dirty(&mut self) {
        self.dirty_rect = None;
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
                max_w: self.width as i32,
                max_h: self.height as i32,
            };
            let (total_w, total_h) = layout_element_ctx(
                root,
                0,
                0,
                self.width as i32,
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
        // An open list swallows hover for everything but its own field. The
        // card is drawn over the form, so a box hiding beneath it must not
        // light up and offer a click that would never arrive — and the field
        // the list belongs to keeps its ramp, because the list is the field's
        // own open state rather than something covering it.
        if let Some(open) = &self.open_select {
            let (box_index, lbox) = find_select_box(&self.layout_boxes, &open.key)?;
            if lbox
                .screen_rect(self.scroll_x, self.scroll_y)
                .contains(pos.0, pos.1)
            {
                return Some(select_key(lbox, box_index));
            }
            return None;
        }
        // A bar under the pointer swallows hover: the tracks run down the very
        // edge of the window, where a control can sit, and a thumb that cannot
        // be told apart from the button behind it is a thumb nobody trusts.
        // Living here rather than in one caller is what makes it hold for
        // *every* path that asks — a cursor move and each frame's
        // re-derivation alike.
        if self.scrollbar_geometry().axis_at(pos).is_some() {
            return None;
        }
        find_hovered_button_in_boxes(&self.layout_boxes, self.scroll_x, self.scroll_y, pos)
    }

    /// Re-derive hover from the layout as it stands now.
    ///
    /// `render` calls this rather than trusting the hover the last cursor move
    /// left behind: layout has just re-run, so a control that slid out from
    /// under the pointer must stop lighting up and one that slid under it must
    /// start. It has to go through `find_hovered_button` for that — asking the
    /// boxes directly would hand the hover straight back to whatever sits
    /// behind a scrollbar, and the hover ramp would chase it to full.
    fn refresh_hover(&mut self) {
        self.hovered_id = self.find_hovered_button(self.cursor_pos);
    }

    pub fn handle_cursor_moved(&mut self, pos: (i32, i32)) -> bool {
        // The row an open list would light up before the pointer moves, so the
        // repaint can be asked for on the *change* rather than on every pixel
        // of travel across a row.
        let popup_row_before = self.popup_effective_row();

        self.cursor_pos = pos;

        // A drag owns the pointer entirely: the thumb follows it and nothing
        // else under it lights up, which is what stops a control sliding past
        // the cursor mid-drag from hijacking the gesture.
        if self.scrollbar_drag.is_some() || self.region_scrollbar_drag.is_some() {
            return self.update_scrollbar_drag(pos);
        }
        // A slider being dragged is the same claim on the pointer: the handle
        // follows the finger and nothing under it lights up, so a control the
        // hand slides over mid-drag cannot steal the gesture.
        if self.slider_drag.is_some() {
            return self.update_slider_drag(pos);
        }

        if let Some(drag) = self.drag_state.as_mut() {
            drag.current_pos = pos;
            if (drag.current_pos.0 - drag.start_pos.0).abs() > 4
                || (drag.current_pos.1 - drag.start_pos.1).abs() > 4
            {
                drag.active = true;
            }
        }
        if self.drag_state.as_ref().is_some_and(|d| d.active) {
            let source_id = self.drag_state.as_ref().unwrap().source_id.clone();
            let target = self
                .drop_target_at(pos, &source_id)
                .and_then(|b| b.id.clone());
            if let Some(drag) = self.drag_state.as_mut() {
                drag.drop_target = target;
            }
            return true;
        }

        if let Some(menu) = self.context_menu.as_mut() {
            let r = menu.menu_rect();
            if r.contains(pos.0, pos.1) {
                let row = ((pos.1 - (r.y + 6)).max(0) / 28) as usize;
                if row < menu.items.len() && row != menu.highlight {
                    menu.highlight = row;
                    return true;
                }
            }
        }

        let g = self.scrollbar_geometry();
        let bar_here = g.axis_at(pos);
        let bar_changed = bar_here != self.scrollbar_hover;
        self.scrollbar_hover = bar_here;

        // A bar under the pointer swallows hover — which `find_hovered_button`
        // decides, so the cursor move and every later frame agree.
        let new_hovered = self.find_hovered_button(pos);
        let hover_changed = new_hovered != self.hovered_id;
        self.hovered_id = new_hovered;
        self.sync_cursor_icon();

        // An open list lights the row the pointer is on, and falls back to the
        // keyboard's row when the pointer is off the card. The highlight lives
        // in the *drawing*, not in the state — the pointer brushing past a row
        // on its way somewhere else must not steal the keyboard's choice.
        let popup_row_changed = self.popup_effective_row() != popup_row_before;

        // A tooltip both appears and *follows* the pointer, so every movement
        // over a labelled box needs a repaint. Keying this off the hover change
        // alone — as the button tone does — leaves the card frozen where the
        // pointer entered and never removes it on the way out.
        let tooltip_here = self.tooltip_here();
        let needs_redraw =
            hover_changed || tooltip_here || self.tooltip_shown || bar_changed || popup_row_changed;
        self.tooltip_shown = tooltip_here;
        needs_redraw
    }

    /// The pointer left the window.
    ///
    /// Every hover decision the window makes — the button tone, the scrollbar
    /// highlight, the text caret, the tooltip card — is derived from
    /// `cursor_pos`, and the platform sends no further moves once the pointer
    /// is gone. Left alone, that position stays where the pointer last was, so
    /// the frame it left stays painted forever: a card explaining a control
    /// nobody is pointing at, a button lit under an absent hand, an I-beam
    /// over a field the mouse has nothing to do with. Dismissing it would need
    /// the pointer to come back and leave again, which is the opposite of
    /// leaving.
    ///
    /// So the position is parked far outside the window rather than merely
    /// snapped to an edge. An edge coordinate would still be *inside* whatever
    /// control sits on that edge, so the tooltip would stay up and the button
    /// would stay lit — just for the wrong box. Far outside, `visible_in`
    /// rejects every box in the tree, so `tooltip_at`, `find_hovered_button`,
    /// `input_under_cursor` and `sync_cursor_icon` all answer from the same
    /// empty position and cannot disagree with each other about what is under
    /// a pointer that is not here.
    ///
    /// A drag in progress is deliberately left holding the control. The
    /// pointer stepping out mid-drag is the hand still on it — the platform
    /// still delivers moves once it comes back — and ending the gesture here
    /// would drop it the moment someone dragged fast enough to cross the edge.
    pub fn handle_cursor_left(&mut self) -> bool {
        let was_hovered = self.hovered_id.clone();
        let was_bar = self.scrollbar_hover;
        let was_tip = self.tooltip_shown;
        let was_kind = self.cursor_kind;

        self.cursor_pos = POINTER_OUTSIDE;
        self.hovered_id = None;
        self.scrollbar_hover = None;
        self.tooltip_shown = false;
        self.sync_cursor_icon();

        // A repaint is owed for whatever the frame that is up is currently
        // showing, and nothing at all when the pointer was already over empty
        // space — leaving a window nobody was touching must not flicker it.
        was_hovered.is_some() || was_bar.is_some() || was_tip || was_kind != self.cursor_kind
    }

    /// Which row an open list lights up: the one under the pointer, or the
    /// keyboard's row when the pointer is off the card. A single source for
    /// the card's highlight, the pointer cursor and Enter, so the three can
    /// never light different rows.
    pub fn popup_effective_row(&self) -> Option<usize> {
        let popup = self.open_select_popup()?;
        let row = select_row_at(
            popup.rect,
            popup.options.len(),
            popup.first,
            self.cursor_pos,
        );
        Some(row.unwrap_or(popup.highlight))
    }

    /// Whether a tooltip should be painted at the current cursor position.
    /// The tooltip, if any, for whatever sits under the cursor.
    ///
    /// A scrollbar under the pointer swallows the control behind it whole —
    /// hover, the press, and this too. A card describing a button you cannot
    /// reach because the bar is drawn over it is a card about the wrong thing.
    fn tooltip_at(&self) -> Option<String> {
        if self.scrollbar_geometry().axis_at(self.cursor_pos).is_some() {
            return None;
        }
        find_tooltip_in_boxes(
            &self.layout_boxes,
            self.scroll_x,
            self.scroll_y,
            self.width as i32,
            self.height as i32,
            self.cursor_pos,
        )
    }

    fn tooltip_here(&self) -> bool {
        self.tooltip_at().is_some()
    }

    // -------------------------------------------------------------
    // Timer / animation (roadmap 1.7)
    // -------------------------------------------------------------

    /// Whether the element holding focus is a text field that is still laid
    /// out and still enabled — the only condition under which a caret exists.
    fn has_focused_input(&self) -> bool {
        let Some(ref id) = self.focused_id else {
            return false;
        };
        self.layout_boxes
            .iter()
            .any(|b| b.tag == "input" && !b.disabled && b.id.as_deref() == Some(id.as_str()))
    }

    /// Whether any spinner in the layout tree is currently within view.
    /// An off-screen or un-laid-out spinner costs zero frame wake-ups.
    fn has_visible_spinner(&self) -> bool {
        self.layout_boxes.iter().any(|b| {
            b.tag == "spinner"
                && b.visible_in(
                    self.scroll_x,
                    self.scroll_y,
                    self.width as i32,
                    self.height as i32,
                )
        })
    }

    /// Restart the caret's visible phase. Typing or clicking somewhere new has
    /// to *show* the caret immediately rather than land in the middle of a
    /// blink, which would make the very act of moving it look like nothing
    /// happened.
    fn touch_caret(&mut self) {
        if self.has_focused_input() {
            self.caret_blink_origin = Some(Instant::now());
            if let Some(win) = &self.window {
                win.request_redraw();
            }
        }
    }

    /// Where in its cycle the caret is: on for [`CARET_BLINK_ON`], off for
    /// [`CARET_BLINK_OFF`], repeating from the moment it last moved.
    fn caret_phase(&self, now: Instant, origin: Instant) -> Duration {
        let period = CARET_BLINK_ON + CARET_BLINK_OFF;
        let elapsed = now.saturating_duration_since(origin).as_nanos();
        Duration::from_nanos((elapsed % period.as_nanos()) as u64)
    }

    /// Whether the caret should be drawn in the frame being built — true only
    /// in the visible half of the cycle, and only while a focusable field is
    /// actually there to host it.
    fn caret_visible(&self, now: Instant) -> bool {
        match (self.caret_blink_origin, self.has_focused_input()) {
            (Some(origin), true) => self.caret_phase(now, origin) < CARET_BLINK_ON,
            _ => false,
        }
    }

    /// How long until the caret flips again.
    fn next_caret_toggle(&self, now: Instant, origin: Instant) -> Duration {
        let into = self.caret_phase(now, origin);
        let remaining = if into < CARET_BLINK_ON {
            CARET_BLINK_ON - into
        } else {
            (CARET_BLINK_ON + CARET_BLINK_OFF) - into
        };
        // Never schedule "immediately": a zero-length deadline would have the
        // loop wake, do nothing, and sleep again in a tight spin.
        remaining.max(Duration::from_millis(1))
    }

    /// Move every ramp in flight toward its target — `1.0` under the pointer,
    /// `0.0` everywhere else — and drop the ones that have fully faded out.
    /// The target is read from the *current* hover state, so moving from one
    /// control to another ramps both at once instead of the first snapping
    /// back the instant the pointer leaves it.
    fn advance_hover_ramps(&mut self, step: Duration) -> bool {
        if let Some(key) = self.hovered_id.clone() {
            self.hover_ramps.entry(key).or_insert(0.0);
        }
        if self.hover_ramps.is_empty() {
            return false;
        }
        let rate = step.as_secs_f32() / HOVER_RAMP_SECS;
        let hovered = self.hovered_id.clone();
        let mut moving = false;
        let mut faded = Vec::new();
        for (key, t) in self.hover_ramps.iter_mut() {
            let target = if hovered.as_deref() == Some(key.as_str()) {
                1.0
            } else {
                0.0
            };
            *t = if *t < target {
                (*t + rate).min(target)
            } else {
                (*t - rate).max(target)
            };
            if (*t - target).abs() > f32::EPSILON {
                moving = true;
            }
            // Faded-out entries are dropped so the map cannot grow with every
            // control the pointer has ever visited; the hovered one always
            // stays, even at zero, so its very first frame has something to
            // read.
            if *t <= 0.0 && target == 0.0 {
                faded.push(key.clone());
            }
        }
        for key in faded {
            self.hover_ramps.remove(&key);
        }
        moving
    }

    /// Whether any button is still between its resting and hovered tones.
    fn ramps_in_flight(&self) -> bool {
        self.hover_ramps.values().any(|t| *t > 0.0 && *t < 1.0)
    }

    /// Advance the animation by the time the last frame actually took, then
    /// draw nothing — the caller draws. `dt` is clamped to
    /// [`MAX_ANIM_STEP`] so a long stall cannot teleport a ramp to its end.
    fn advance_animation(&mut self) {
        let now = Instant::now();
        let dt = self
            .last_tick
            .map(|prev| now.saturating_duration_since(prev))
            .unwrap_or(Duration::ZERO);
        self.last_tick = Some(now);
        self.advance_hover_ramps(dt.min(MAX_ANIM_STEP));
    }

    /// Recompute the clock from what the UI actually needs. Called after every
    /// frame, so an interface with nothing moving ends up with a cleared
    /// clock: the loop then sleeps in `ControlFlow::Wait` and costs nothing
    /// until the next real event.
    ///
    /// This deliberately does *not* ask for a redraw. The frame is requested
    /// by [`about_to_wait`] once the deadline it just recorded has actually
    /// arrived — asking here would rebuild the very redraw that frame
    /// consumed, and pin the loop at full speed instead of at the tick.
    fn schedule_frames(&mut self, now: Instant) {
        self.clock.clear();

        if self.ramps_in_flight() {
            self.clock.wake_in(now, RAMP_TICK);
        }

        if let Some(origin) = self.caret_blink_origin {
            if self.has_focused_input() {
                self.clock.wake_in(now, self.next_caret_toggle(now, origin));
            } else {
                // The caret it belonged to is gone; stop paying for it.
                self.caret_blink_origin = None;
            }
        }

        if self.has_visible_spinner() {
            if self.spinner_origin.is_none() {
                self.spinner_origin = Some(now);
            }
            self.clock.wake_in(now, SPINNER_TICK);
        } else {
            // No spinner remains in view: drop the origin so idle cost is 0.
            self.spinner_origin = None;
        }
    }

    /// Spend a wake-up on a frame if the clock's promised moment has arrived.
    /// Returns whether a redraw was requested. This is the only place the loop
    /// re-arms itself, which is what keeps one animation frame per tick rather
    /// than one per iteration.
    fn fire_due_frame(&mut self) -> bool {
        if self.clock.is_idle() {
            return false;
        }
        match self.clock.next_deadline() {
            Some(at) if Instant::now() >= at => {
                self.clock.clear();
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
                true
            }
            _ => false,
        }
    }

    /// Put the event loop to sleep until the clock's deadline — or for good,
    /// when there is none.
    fn pump_clock(&self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(match self.clock.next_deadline() {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }

    // -------------------------------------------------------------
    // Scrollbar interaction (roadmap 1.9)
    // -------------------------------------------------------------

    /// Both window scrollbars against the current layout.
    fn scrollbar_geometry(&self) -> ScrollbarGeometry {
        if !self.scroll_regions.is_empty() {
            return ScrollbarGeometry::default();
        }
        ScrollbarGeometry::resolve(
            self.width as i32,
            self.height as i32,
            self.content_width,
            self.content_height,
            self.scroll_x,
            self.scroll_y,
        )
    }

    /// Where the pointer sits along a bar's track, in track-local pixels.
    fn scrollbar_along(g: &ScrollbarGeometry, axis: ScrollbarAxis, pos: (i32, i32)) -> i32 {
        let (px, py) = pos;
        match axis {
            ScrollbarAxis::Vertical => g
                .vertical
                .map(|(track, _)| py - track.y)
                .unwrap_or_default(),
            ScrollbarAxis::Horizontal => g
                .horizontal
                .map(|(track, _)| px - track.x)
                .unwrap_or_default(),
        }
    }

    /// Begin a scrollbar interaction at `pos`: grabbing the thumb if the press
    /// landed on it, or paging the content so the thumb's centre comes to the
    /// pointer if it landed on bare track. Returns whether a bar was hit.
    ///
    /// The grab offset is recorded rather than assuming the press was at the
    /// thumb's centre — otherwise the thumb jumps the instant it is pressed,
    /// which makes precise positioning impossible.
    fn begin_scrollbar_drag(&mut self, pos: (i32, i32)) -> bool {
        for r in &self.scroll_regions {
            let max = r.max_offset();
            if max <= 0 {
                continue;
            }
            let vx = r.rect.x - self.scroll_x - r.pan.0;
            let vy = r.rect.y - self.scroll_y - r.pan.1;
            let (vw, vh) = (r.rect.w, r.rect.h);
            if pos.0 >= vx + vw - 14 && pos.0 <= vx + vw && pos.1 >= vy && pos.1 <= vy + vh {
                let track_h = (vh - 8).max(1);
                let thumb_h = ((track_h as f64 * vh as f64) / r.content_h.max(1) as f64)
                    .max(20.0)
                    .min(track_h as f64) as i32;
                let travel = (track_h - thumb_h).max(1);
                let off = self.scroll_offsets.get(&r.key).copied().unwrap_or(0);
                let thumb_y =
                    vy + 4 + ((off as f64 / max as f64) * (track_h - thumb_h).max(0) as f64) as i32;
                let grab = if pos.1 >= thumb_y && pos.1 <= thumb_y + thumb_h {
                    pos.1 - thumb_y
                } else {
                    let target = (pos.1 - (vy + 4) - thumb_h / 2).clamp(0, travel);
                    let new_off = ((target as f64 / travel as f64) * max as f64).round() as i32;
                    self.scroll_offsets.insert(r.key.clone(), new_off);
                    thumb_h / 2
                };
                self.region_scrollbar_drag = Some((r.key.clone(), grab, vy + 4, travel, max));
                self.render();
                return true;
            }
        }

        let g = self.scrollbar_geometry();
        let Some(axis) = g.axis_at(pos) else {
            return false;
        };
        let along = Self::scrollbar_along(&g, axis, pos);
        let (track, thumb) = match axis {
            ScrollbarAxis::Vertical => g.vertical.expect("axis_at returned Vertical"),
            ScrollbarAxis::Horizontal => g.horizontal.expect("axis_at returned Horizontal"),
        };
        let thumb_along = match axis {
            ScrollbarAxis::Vertical => thumb.y - track.y,
            ScrollbarAxis::Horizontal => thumb.x - track.x,
        };

        let grab = if g.on_thumb(pos) {
            along - thumb_along
        } else {
            // Bare track: page the content to the pointer, then grab the
            // middle of the thumb that just arrived under it.
            let travel = match axis {
                ScrollbarAxis::Vertical => (track.h - thumb.h).max(0),
                ScrollbarAxis::Horizontal => (track.w - thumb.w).max(0),
            };
            let target = (along
                - match axis {
                    ScrollbarAxis::Vertical => thumb.h / 2,
                    ScrollbarAxis::Horizontal => thumb.w / 2,
                })
            .clamp(0, travel);
            let max = match axis {
                ScrollbarAxis::Vertical => max_scroll(self.content_height, self.height as i32),
                ScrollbarAxis::Horizontal => max_scroll(self.content_width, self.width as i32),
            };
            self.set_scrollbar_offset(axis, scrollbar_offset_at(travel, target, max));
            match axis {
                ScrollbarAxis::Vertical => thumb.h / 2,
                ScrollbarAxis::Horizontal => thumb.w / 2,
            }
        };

        self.scrollbar_drag = Some((axis, grab, along));
        self.scrollbar_hover = Some(axis);
        true
    }

    /// Follow the pointer with the bar being dragged. Returns whether the
    /// offset moved.
    fn update_scrollbar_drag(&mut self, pos: (i32, i32)) -> bool {
        if let Some((ref key, grab, track_y, travel, max)) = self.region_scrollbar_drag {
            let target = (pos.1 - track_y - grab).clamp(0, travel);
            let new_off = ((target as f64 / travel as f64) * max as f64).round() as i32;
            let cur = self.scroll_offsets.get(key).copied().unwrap_or(0);
            if new_off != cur {
                self.scroll_offsets.insert(key.clone(), new_off);
                self.render();
                return true;
            }
            return false;
        }

        let Some((axis, grab, _)) = self.scrollbar_drag else {
            return false;
        };
        let g = self.scrollbar_geometry();
        let along = Self::scrollbar_along(&g, axis, pos);
        let (track, thumb) = match axis {
            ScrollbarAxis::Vertical => g.vertical.expect("dragging a bar that has no track"),
            ScrollbarAxis::Horizontal => g.horizontal.expect("dragging a bar that has no track"),
        };
        let travel = match axis {
            ScrollbarAxis::Vertical => (track.h - thumb.h).max(0),
            ScrollbarAxis::Horizontal => (track.w - thumb.w).max(0),
        };
        let max = match axis {
            ScrollbarAxis::Vertical => max_scroll(self.content_height, self.height as i32),
            ScrollbarAxis::Horizontal => max_scroll(self.content_width, self.width as i32),
        };

        // Drag from the recorded grab point, so the content follows the
        // pointer exactly and the thumb never slips out from under the cursor.
        let target = (along - grab).clamp(0, travel);
        let next = scrollbar_offset_at(travel, target, max);
        self.set_scrollbar_offset(axis, next)
    }

    /// Let go of the bar being dragged. Returns whether there was one.
    ///
    /// The drag owns the pointer while it runs, so hover — including which
    /// bar, if any, the pointer sits on — was frozen at the point the grab
    /// started. Re-deriving it on release is what stops a control under the
    /// released pointer staying dark until the next pixel of movement, and a
    /// bar staying lit long after the pointer has left it.
    fn end_scrollbar_drag(&mut self) -> bool {
        let r_ended = self.region_scrollbar_drag.take().is_some();
        let b_ended = self.scrollbar_drag.take().is_some();
        if !r_ended && !b_ended {
            return false;
        }
        self.handle_cursor_moved(self.cursor_pos);
        true
    }

    /// The `slider` the press landed on, if any, and the track it draws in.
    fn slider_at(&self, pos: (i32, i32)) -> Option<(usize, LayoutBox, Rect)> {
        let (mx, my) = pos;
        for (i, lbox) in self.layout_boxes.iter().enumerate() {
            if lbox.tag != "slider" {
                continue;
            }
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
            return Some((i, lbox.clone(), slider_track_rect(sr)));
        }
        None
    }

    /// Begin a slider interaction at `pos`. Returns whether a slider took the
    /// press — which it always does when one is under the pointer, disabled or
    /// not, because a control that cannot move must still absorb the press
    /// rather than let it fall through to whatever is stacked behind it.
    ///
    /// Pressing the *handle* records how far off centre the press landed, so
    /// the handle follows the finger instead of jumping its own width to the
    /// right. Pressing bare track moves the value straight to the pointer,
    /// because that is the obvious reading of the gesture.
    ///
    /// Pressing the *label* — anywhere on the row past the groove — moves
    /// nothing at all. The row is one box so the words take the press with the
    /// control, but the words are not the dial: `x_to_value` clamps, so
    /// letting a label click reach it would slam a 500 to 1993, which is a
    /// value nobody asked for by reading a caption. Still swallowed, so a
    /// control behind the row cannot be fired through it either.
    fn begin_slider_drag(&mut self, pos: (i32, i32)) -> bool {
        let Some((_, lbox, track)) = self.slider_at(pos) else {
            return false;
        };
        let Some(spec) = lbox.slider else {
            return true;
        };
        if lbox.disabled || lbox.id.is_none() {
            return true;
        }
        // The groove spans the track, and the handle's own radius overhangs
        // its ends by design — so a little past either stop is still the dial,
        // while the label beside it is not.
        let overhang = SLIDER_KNOB / 2;
        if pos.0 < track.x - overhang || pos.0 > track.x + track.w + overhang {
            return true;
        }
        let id = lbox.id.clone().expect("checked above");
        let handle_x = track.x + spec.value_to_x(spec.value).round() as i32;
        let grab_dx = (pos.0 - handle_x) as f32;
        let on_handle = (pos.0 - handle_x).abs() <= SLIDER_KNOB / 2;
        let first = if on_handle {
            spec.value
        } else {
            spec.x_to_value((pos.0 - track.x) as f32)
        };
        // Pressing bare track moves the value first, so the grab offset is
        // re-measured from where the handle *now* is: the handle then sits
        // under the finger for the rest of the drag instead of leaping a
        // handle's width on the first pixel of movement.
        let grab_dx = if on_handle {
            grab_dx
        } else {
            (pos.0 - (track.x + spec.value_to_x(first).round() as i32)) as f32
        };
        self.slider_drag = Some(SliderDrag {
            grab_dx,
            track_x: track.x,
            origin: SliderSpec {
                value: first,
                ..spec
            },
            current: first,
            id: id.clone(),
        });
        if first != spec.value {
            let step = spec.step;
            self.apply_slider_value(&id, first, step);
            true
        } else {
            false
        }
    }

    /// Follow the pointer with the handle being dragged. Returns whether the
    /// value moved — and so whether a change was reported and a repaint owed.
    fn update_slider_drag(&mut self, pos: (i32, i32)) -> bool {
        let Some((id, track_x, grab_dx, origin, current)) = self
            .slider_drag
            .as_ref()
            .map(|d| (d.id.clone(), d.track_x, d.grab_dx, d.origin, d.current))
        else {
            return false;
        };
        // The offset the press landed at is carried through the whole drag, so
        // the handle stays under the finger rather than leaping to its own
        // centre. `track_x` is the value remembered at the press — re-reading
        // it from a form that scrolled under a held handle would throw the
        // value by the amount the form moved.
        let value = origin.x_to_value((pos.0 - track_x) as f32 - grab_dx);
        if value == current {
            return false;
        }
        let step = origin.step;
        if let Some(drag) = self.slider_drag.as_mut() {
            drag.current = value;
        }
        self.apply_slider_value(&id, value, step);
        true
    }

    /// Let go of the handle being dragged. Returns whether there was one, and
    /// re-derives hover the same way a released scrollbar does — the drag owns
    /// the pointer while it runs, so what the released pointer sits on is
    /// whatever was under the hand when it stopped.
    fn end_slider_drag(&mut self) -> bool {
        if self.slider_drag.take().is_none() {
            return false;
        }
        self.handle_cursor_moved(self.cursor_pos);
        true
    }

    /// Write a slider's new number: report it, then re-read the tree so the
    /// handle, the fill and the state all show the value that was just
    /// accepted.
    fn apply_slider_value(&mut self, id: &str, value: f64, step: f64) {
        self.dispatcher
            .dispatch_change(id, &crate::ui::slider_num_str(value, step));
        self.render();
    }

    /// Write an offset to whichever axis' scroll state it belongs to, clamped
    /// the same way the wheel clamps it.
    fn set_scrollbar_offset(&mut self, axis: ScrollbarAxis, value: i32) -> bool {
        match axis {
            ScrollbarAxis::Vertical => {
                let max = max_scroll(self.content_height, self.height as i32);
                let next = value.clamp(0, max);
                if next == self.scroll_y {
                    return false;
                }
                self.scroll_y = next;
                true
            }
            ScrollbarAxis::Horizontal => {
                let max = max_scroll(self.content_width, self.width as i32);
                let next = value.clamp(0, max);
                if next == self.scroll_x {
                    return false;
                }
                self.scroll_x = next;
                true
            }
        }
    }

    /// Push the cursor shape implied by whatever is under the pointer to the
    /// window. Called after hover changes and after every layout, so a control
    /// that moves out from under a still pointer never keeps a stale shape.
    fn sync_cursor_icon(&mut self) {
        // A scrollbar outranks everything else under the pointer: the bars run
        // along the window edge, where a control could perfectly well sit, and
        // a thumb that cannot be told apart from the button behind it is a
        // thumb nobody trusts.
        let over_thumb =
            self.scrollbar_hover.is_some() && self.scrollbar_geometry().on_thumb(self.cursor_pos);
        // Rows of an open list are choices, so they take the hand — and they
        // are drawn, not laid out, so no `LayoutBox` could ever report them.
        let over_popup_row = self.select_row_hit(self.cursor_pos).is_some();
        let kind = if self.scrollbar_drag.is_some() {
            CursorKind::Grabbing
        } else if over_thumb {
            CursorKind::Grab
        } else if self.scrollbar_hover.is_some() {
            CursorKind::Default
        } else if over_popup_row || self.hovered_id.is_some() {
            // `find_hovered_button` already excludes disabled buttons, so the
            // hand never appears over an inert control.
            CursorKind::Pointer
        } else if input_under_cursor(
            &self.layout_boxes,
            self.scroll_x,
            self.scroll_y,
            self.width as i32,
            self.height as i32,
            self.cursor_pos,
        ) {
            CursorKind::Text
        } else {
            CursorKind::Default
        };
        if kind != self.cursor_kind {
            self.cursor_kind = kind;
            if let Some(win) = &self.window {
                win.set_cursor(kind.icon());
            }
        }
    }

    /// Where the open list is, and how much of it the window can show, or
    /// `None` when nothing is open — or when the field it belongs to has
    /// scrolled or re-laid out of sight, which is how a list detaches from
    /// its control and floats over nothing. Resolved from the current layout
    /// every time rather than remembered, so a scroll or a re-render moves
    /// the card with the field instead of leaving it behind.
    pub fn open_select_popup(&self) -> Option<SelectPopupView> {
        let open = self.open_select.as_ref()?;
        let (_, lbox) = find_select_box(&self.layout_boxes, &open.key)?;
        if !lbox.visible_in(
            self.scroll_x,
            self.scroll_y,
            self.width as i32,
            self.height as i32,
        ) {
            return None;
        }
        let count = lbox.options.len();
        let field = lbox.screen_rect(self.scroll_x, self.scroll_y);
        let rect = select_popup_rect(field, count, self.width as i32, self.height as i32);
        let rows = select_visible_rows(rect.h).min(count);
        Some(SelectPopupView {
            key: open.key.clone(),
            rect,
            options: lbox.options.clone(),
            first: select_scroll_clamp(count, rows, open.scroll),
            rows,
            highlight: open.highlight.min(count.saturating_sub(1)),
            value: lbox.text.clone(),
            id: lbox.id.clone(),
        })
    }

    /// Opens a field's list, starting the keyboard on the option it is
    /// showing so Enter keeps the current choice unless the user moves. A
    /// field with no `id` has nowhere to report a choice to, so it opens
    /// nothing — it is decoration, like a button with no `on_click`.
    pub fn open_select(&mut self, key: &str) {
        let Some((_, lbox)) = find_select_box(&self.layout_boxes, key) else {
            return;
        };
        if lbox.disabled || lbox.id.is_none() || lbox.options.is_empty() {
            return;
        }
        let count = lbox.options.len();
        let highlight = lbox
            .text
            .as_deref()
            .and_then(|v| lbox.options.iter().position(|o| o == v))
            .unwrap_or(0);
        let scroll = select_scroll_for_highlight(count, count, highlight);
        self.open_select = Some(OpenSelect {
            key: key.to_string(),
            highlight,
            scroll,
        });
        self.render();
    }

    /// Shuts the open list, if any.
    pub fn close_select(&mut self) {
        if self.open_select.take().is_some() {
            self.render();
        }
    }

    /// Takes an option out of the open list: reports it through the field's
    /// `on_change`, then shuts the list. The choice is dispatched as a change
    /// rather than a click, so it reuses the plumbing a text field already
    /// shares — the handler sees the new value and the frame re-renders.
    pub fn commit_select(&mut self, index: usize) {
        let Some(popup) = self.open_select_popup() else {
            self.open_select = None;
            return;
        };
        let Some(value) = popup.options.get(index).cloned() else {
            self.open_select = None;
            return;
        };
        self.open_select = None;
        if let Some(id) = popup.id {
            self.dispatcher.dispatch_change(&id, &value);
        }
        self.render();
    }

    /// Which option a press on the open list lands on, resolved the same way
    /// the card is drawn so a click can never strike a row the eye did not see.
    pub fn select_row_hit(&self, pos: (i32, i32)) -> Option<usize> {
        let popup = self.open_select_popup()?;
        select_row_at(popup.rect, popup.options.len(), popup.first, pos)
    }

    /// Moves the keyboard highlight, pulling the list so it stays visible.
    /// Returns whether anything moved, so the caller can decide to repaint.
    pub fn move_select_highlight(&mut self, delta: isize) -> bool {
        let Some(popup) = self.open_select_popup() else {
            self.open_select = None;
            return false;
        };
        let count = popup.options.len();
        let cur = popup.highlight as isize;
        let next = (cur + delta).clamp(0, count.saturating_sub(1) as isize) as usize;
        let scroll = select_scroll_for_highlight(count, popup.rows, next);
        let open = self.open_select.as_mut().expect("open list checked above");
        let moved = open.highlight != next || open.scroll != scroll;
        open.highlight = next;
        open.scroll = scroll;
        moved
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
        let max_scroll_y = if self.scroll_regions.is_empty() {
            (total_h - self.height as i32 + 20).max(0)
        } else {
            0
        };
        let max_scroll_x = if self.scroll_regions.is_empty() {
            (total_w - self.width as i32 + 20).max(0)
        } else {
            0
        };
        self.scroll_y = self.scroll_y.clamp(0, max_scroll_y);
        self.scroll_x = self.scroll_x.clamp(0, max_scroll_x);

        // The program may drive the palette from state — a theme toggle is
        // nothing more than state plus a button — so it is asked for every
        // frame rather than fixed at startup, and lands on this very paint.
        // Done before the surface is borrowed, like everything else this frame
        // has to decide.
        if let Some(t) = self.dispatcher.theme() {
            self.theme = t;
        }

        // Hover is re-derived here as well as on every cursor move, so the
        // tone follows the layout rather than the last event. Resolved before
        // the surface is borrowed, and before the cursor shape, which reads it.
        self.refresh_hover();

        // Layout just moved every box, so re-derive the cursor shape from the
        // new geometry rather than trusting the one hover last set.
        self.sync_cursor_icon();

        // Whether the caret is in the visible half of its blink. Resolved once
        // here — before the surface is borrowed — so every field draws it for
        // the same frame, and a per-box check cannot let two carets disagree
        // if the clock happened to flip between them.
        let caret_on = self.caret_visible(Instant::now());

        // Both window scrollbars, resolved once here so drawing them cannot
        // disagree with hit-testing them later.
        let scrollbars = self.scrollbar_geometry();
        let bar_hover = self.scrollbar_hover;
        let bar_dragging = self.scrollbar_drag.is_some();

        // And the tooltip, which layers over every box rather than sitting in
        // their order, so it too is asked for while `self` can still be read.
        //
        // An open choice list owns the pointer outright: while it is up it
        // takes every press, so a label belonging to a box hiding behind the
        // card would be describing something the user cannot reach. Suppressing
        // the tooltip is how the list says it is modal.
        //
        // A list whose field has scrolled or re-laid out of view shuts here
        // rather than drawing a card attached to nothing.
        if self.open_select.is_some() && self.open_select_popup().is_none() {
            self.open_select = None;
        }
        let popup = self.open_select_popup();
        let tooltip = if popup.is_some() {
            None
        } else {
            self.tooltip_at()
        };

        // The row the card lights, resolved here so the pointer cursor and the
        // drawn row cannot disagree with what the frame already measured.
        let popup_hover = self.popup_effective_row();

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
        buffer.fill(theme.bg);

        let (mx, my) = self.cursor_pos;

        // Shadows first, as their own pass: every halo lands on the bare
        // background, then the boxes paint over the parts underneath them.
        // Drawing a halo inline would let a later bubble's shadow darken an
        // earlier bubble's edge — a grey smear down the feed.
        draw_shadows(
            &self.layout_boxes,
            self.scroll_x,
            self.scroll_y,
            &mut buffer,
            stride,
            h_usize,
            &theme,
        );

        // Render each layout box with window scroll, enclosing scroll
        // containers (pan) & their viewport clipping. The index participates
        // in the button dispatch key when a button has no id or handler.
        for (box_index, lbox) in self.layout_boxes.iter().enumerate() {
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
                        let cell_h = fonts.cell_h;
                        for (idx, line) in text.split('\n').enumerate() {
                            let line_y = ry + 2 + (idx as i32 * cell_h);
                            draw_rich_line(
                                fonts,
                                &theme,
                                rx,
                                line_y,
                                line,
                                color,
                                clip,
                                |px, py, w, h, col, cov| {
                                    draw_rect_alpha(
                                        &mut buffer,
                                        stride,
                                        h_usize,
                                        px,
                                        py,
                                        w,
                                        h,
                                        col,
                                        cov,
                                    );
                                },
                            );
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
                            let cell_h = fonts.cell_h;
                            for (idx, line) in text.split('\n').enumerate() {
                                let line_y = ty + (idx as i32 * cell_h);
                                draw_rich_line(
                                    fonts,
                                    &theme,
                                    tx,
                                    line_y,
                                    line,
                                    fg,
                                    Some(tc),
                                    |px, py, w, h, col, cov| {
                                        draw_rect_alpha(
                                            &mut buffer,
                                            stride,
                                            h_usize,
                                            px,
                                            py,
                                            w,
                                            h,
                                            col,
                                            cov,
                                        );
                                    },
                                );
                            }
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
                    // even if the cursor sits on its (invisible) rectangle, and
                    // a disabled one is never hoverable at all.
                    let hovered = !lbox.disabled
                        && screen_rect.contains(mx, my)
                        && clip.is_none_or(|c| c.contains(mx, my));
                    // The pressed tone is matched on the dispatch *key* rather
                    // than on box identity, so the box that lit up when the
                    // button went down is exactly the box that fires on release.
                    let key = button_key(lbox, box_index);
                    let is_focused =
                        lbox.id.is_some() && self.focused_id.as_deref() == lbox.id.as_deref();
                    let pressed = hovered && self.pressed_id.as_deref() == Some(key.as_str());
                    // How far this button has travelled toward its hovered
                    // tone. Read by the same key, so an unlabelled button
                    // ramps too and one already fading out after the pointer
                    // left keeps drawing the blend it has reached instead of
                    // snapping back to rest the moment it stops being hovered.
                    let hover_t = if is_focused {
                        1.0
                    } else {
                        self.hover_ramps.get(&key).copied().unwrap_or(0.0)
                    };
                    let variant = lbox
                        .props
                        .get("variant")
                        .map(|s| s.as_str())
                        .unwrap_or("default");
                    let is_selected = lbox.props.get("selected").is_some_and(|s| is_truthy(s))
                        || lbox.props.get("active").is_some_and(|s| is_truthy(s));
                    let align = lbox
                        .props
                        .get("align")
                        .map(|s| s.as_str())
                        .unwrap_or("center");

                    let mut text_color = theme.button_text;

                    if is_selected {
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
                            theme.selection_bg,
                            theme.selection_bg,
                        );
                        text_color = 0xFFFF_FFFF;
                    } else if variant == "flat"
                        || variant == "ghost"
                        || variant == "toolbar"
                        || variant == "item"
                        || variant == "folder_card"
                        || variant == "file_card"
                    {
                        text_color = theme.text;
                        if pressed {
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
                                theme.selection_bg,
                                theme.selection_bg,
                            );
                            text_color = 0xFFFF_FFFF;
                        } else if hovered || hover_t > 0.05 {
                            let (hover_bg, hover_border) =
                                if variant == "folder_card" || variant == "file_card" {
                                    if theme.is_dark() {
                                        (0xFF35_3B42, 0xFF45_4D56)
                                    } else {
                                        (0xFFEBF3FB, 0xFFB0_D6F5)
                                    }
                                } else {
                                    let c = if theme.is_dark() {
                                        0xFF35_3B42
                                    } else {
                                        0xFFD8_DCDF
                                    };
                                    (c, c)
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
                                hover_bg,
                                hover_border,
                            );
                        }
                    } else {
                        let (grad_top, grad_bottom) =
                            button_tones(&theme, hover_t, pressed, lbox.disabled);
                        let border_color = if lbox.disabled {
                            theme.border
                        } else if is_focused {
                            theme.input_border_focused
                        } else {
                            theme.button_border
                        };
                        draw_rounded_rect_gradient_box_clipped(
                            &mut buffer,
                            stride,
                            h_usize,
                            clip,
                            rx,
                            ry,
                            lbox.rect.w,
                            lbox.rect.h,
                            CORNER_RADIUS,
                            grad_top,
                            grad_bottom,
                            border_color,
                            false,
                        );
                    }
                    if is_focused && !is_selected {
                        draw_focus_ring(
                            &mut buffer,
                            stride,
                            h_usize,
                            clip,
                            rx,
                            ry,
                            lbox.rect.w,
                            lbox.rect.h,
                            CORNER_RADIUS,
                            theme.input_border_focused,
                        );
                    }
                    if variant == "folder_card" || variant == "file_card" {
                        if variant == "folder_card" {
                            let icon_w = 56;
                            let icon_h = 40;
                            let icon_x = rx + (lbox.rect.w - icon_w) / 2;
                            let icon_y = ry + 8;
                            draw_breeze_folder_icon(
                                &mut buffer,
                                stride,
                                h_usize,
                                clip,
                                icon_x,
                                icon_y,
                                icon_w,
                                icon_h,
                            );
                        } else {
                            let ftype = lbox
                                .props
                                .get("file_type")
                                .map(|s| s.as_str())
                                .unwrap_or("");
                            let ext = lbox.props.get("ext").map(|s| s.as_str()).unwrap_or("");
                            let is_img = ftype == "image"
                                || ext == "png"
                                || ext == "jpg"
                                || ext == "jpeg"
                                || ext == "svg"
                                || ext == "webp"
                                || ext == "gif";
                            let is_code = ftype == "code"
                                || ext == "rs"
                                || ext == "ndg"
                                || ext == "py"
                                || ext == "js"
                                || ext == "ts"
                                || ext == "c"
                                || ext == "cpp"
                                || ext == "h"
                                || ext == "json"
                                || ext == "toml"
                                || ext == "yaml"
                                || ext == "md"
                                || ext == "sh";
                            let is_audio = ftype == "audio"
                                || ext == "mp3"
                                || ext == "wav"
                                || ext == "flac"
                                || ext == "ogg";
                            let is_video = ftype == "video"
                                || ext == "mp4"
                                || ext == "mkv"
                                || ext == "webm"
                                || ext == "avi";
                            let is_archive = ftype == "archive"
                                || ext == "zip"
                                || ext == "tar"
                                || ext == "gz"
                                || ext == "7z"
                                || ext == "xz";

                            if is_img {
                                let icon_w = 48;
                                let icon_h = 40;
                                let icon_x = rx + (lbox.rect.w - icon_w) / 2;
                                let icon_y = ry + 8;
                                draw_image_thumbnail_card(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    clip,
                                    icon_x,
                                    icon_y,
                                    icon_w,
                                    icon_h,
                                );
                            } else if is_code {
                                let icon_w = 38;
                                let icon_h = 44;
                                let icon_x = rx + (lbox.rect.w - icon_w) / 2;
                                let icon_y = ry + 8;
                                draw_code_thumbnail_card(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    clip,
                                    icon_x,
                                    icon_y,
                                    icon_w,
                                    icon_h,
                                    ext,
                                );
                            } else if is_audio {
                                let icon_w = 40;
                                let icon_h = 42;
                                let icon_x = rx + (lbox.rect.w - icon_w) / 2;
                                let icon_y = ry + 8;
                                draw_audio_thumbnail_card(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    clip,
                                    icon_x,
                                    icon_y,
                                    icon_w,
                                    icon_h,
                                );
                            } else if is_video {
                                let icon_w = 46;
                                let icon_h = 40;
                                let icon_x = rx + (lbox.rect.w - icon_w) / 2;
                                let icon_y = ry + 8;
                                draw_video_thumbnail_card(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    clip,
                                    icon_x,
                                    icon_y,
                                    icon_w,
                                    icon_h,
                                );
                            } else if is_archive {
                                let icon_w = 36;
                                let icon_h = 44;
                                let icon_x = rx + (lbox.rect.w - icon_w) / 2;
                                let icon_y = ry + 8;
                                draw_archive_thumbnail_card(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    clip,
                                    icon_x,
                                    icon_y,
                                    icon_w,
                                    icon_h,
                                );
                            } else {
                                let icon_w = 36;
                                let icon_h = 44;
                                let icon_x = rx + (lbox.rect.w - icon_w) / 2;
                                let icon_y = ry + 8;
                                let accent = 0xFF4A_90D9;
                                draw_breeze_file_icon(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    clip,
                                    icon_x,
                                    icon_y,
                                    icon_w,
                                    icon_h,
                                    accent,
                                );
                            }
                        }
                        if let Some(ref text) = lbox.text {
                            let parts: Vec<&str> = text.split('\n').collect();
                            let line1 = parts.first().copied().unwrap_or("");
                            let line2 = parts.get(1).copied().unwrap_or("");
                            let interior = Rect {
                                x: rx + 4,
                                y: ry,
                                w: lbox.rect.w - 8,
                                h: lbox.rect.h,
                            };
                            let text_clip = match clip {
                                None => Some(interior),
                                Some(c) => intersect(interior, c),
                            };
                            if let Some(tc) = text_clip {
                                let w1 = self.fonts.measure_width(FaceKind::Ui, line1);
                                let max_text_w = (lbox.rect.w - 12).max(20);
                                let display_line1 = if w1 > max_text_w {
                                    let mut truncated = String::new();
                                    for ch in line1.chars() {
                                        let candidate = format!("{truncated}{ch}…");
                                        if self.fonts.measure_width(FaceKind::Ui, &candidate)
                                            > max_text_w
                                        {
                                            break;
                                        }
                                        truncated.push(ch);
                                    }
                                    if truncated.is_empty() {
                                        line1.chars().take(4).collect::<String>()
                                    } else {
                                        format!("{truncated}…")
                                    }
                                } else {
                                    line1.to_string()
                                };
                                let dw1 = self.fonts.measure_width(FaceKind::Ui, &display_line1);
                                let tx1 = rx + ((lbox.rect.w - dw1) / 2).max(4);
                                let ty1 = ry + 54;
                                draw_text_ttf_clipped(
                                    &self.fonts,
                                    FaceKind::Ui,
                                    tx1,
                                    ty1,
                                    &display_line1,
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
                                            text_color,
                                            cov,
                                        );
                                    },
                                );
                                if !line2.is_empty() {
                                    let w2 = self.fonts.measure_width(FaceKind::Ui, line2);
                                    let tx2 = rx + ((lbox.rect.w - w2) / 2).max(4);
                                    let ty2 = ry + 74;
                                    let sub_color = if is_selected {
                                        0xFFEE_F2F5
                                    } else {
                                        theme.text_dim
                                    };
                                    draw_text_ttf_clipped(
                                        &self.fonts,
                                        FaceKind::Ui,
                                        tx2,
                                        ty2,
                                        line2,
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
                                                sub_color,
                                                cov,
                                            );
                                        },
                                    );
                                }
                            }
                        }
                    } else if let Some(ref text) = lbox.text {
                        let text_w = self.fonts.measure_width(FaceKind::Ui, text);
                        let tx = if align == "start" || align == "left" {
                            rx + 8
                        } else {
                            rx + ((lbox.rect.w - text_w) / 2).max(8)
                        };
                        let ty = ry + ((lbox.rect.h - cell_h) / 2).max(2);
                        let color = if lbox.disabled {
                            theme.text_dim
                        } else {
                            text_color
                        };
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
                    // An inert field can never hold focus, so no caret or
                    // selection highlight can ever appear inside it.
                    let is_focused = !lbox.disabled
                        && lbox.id.is_some()
                        && self.focused_id.as_deref() == lbox.id.as_deref();
                    let (bg, border_color) = input_surface(&theme, is_focused, lbox.disabled);

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
                        bg,
                        border_color,
                    );
                    if is_focused {
                        draw_focus_ring(
                            &mut buffer,
                            stride,
                            h_usize,
                            clip,
                            rx,
                            ry,
                            lbox.rect.w,
                            lbox.rect.h,
                            CORNER_RADIUS,
                            theme.input_border_focused,
                        );
                    }

                    let (display_text, cursor_opt, selection_opt) = if let Some(ref id) = lbox.id {
                        let lbox_text = lbox.text.clone().unwrap_or_default();
                        if let Some(st) = self.text_inputs.get_mut(id) {
                            if lbox_text.is_empty() && !st.text.is_empty() {
                                st.text.clear();
                                st.cursor = 0;
                            }
                            (st.text.clone(), Some(st.cursor), st.selection_range())
                        } else {
                            (lbox_text, None, None)
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
                    let input_text_color = if lbox.disabled {
                        theme.text_dim
                    } else {
                        theme.text
                    };
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

                    // Draw the caret while focused and in the visible half of
                    // its blink; the blink's other half is what makes a still
                    // field obviously alive without any polling from the user.
                    if is_focused && caret_on {
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
                // A closed choice field: the input's chrome, so it reads as
                // something a value is entered into, plus the value itself
                // and a chevron. The list it opens is an overlay drawn after
                // every box, never a box of its own — a real layout would
                // shove the rest of the form down every time a field opened.
                "select" => {
                    let key = select_key(lbox, box_index);
                    let is_open_here = self
                        .open_select
                        .as_ref()
                        .is_some_and(|open| open.key == key);
                    // Open reads as focused — the same accent border a text
                    // field wears while it takes keys — because the list is
                    // the field's active state.
                    let (bg, mut border_color) = input_surface(&theme, is_open_here, lbox.disabled);
                    // Hover lifts the border the way a button lifts its
                    // surface, so the field that will open is visible before
                    // it does.
                    if !lbox.disabled {
                        let hover_t = self.hover_ramps.get(&key).copied().unwrap_or(0.0);
                        if hover_t > 0.0 {
                            border_color =
                                lerp_color(border_color, theme.input_border_focused, hover_t);
                        }
                    }

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
                        bg,
                        border_color,
                    );
                    let is_focused =
                        lbox.id.is_some() && self.focused_id.as_deref() == lbox.id.as_deref();
                    if is_focused {
                        draw_focus_ring(
                            &mut buffer,
                            stride,
                            h_usize,
                            clip,
                            rx,
                            ry,
                            lbox.rect.w,
                            lbox.rect.h,
                            CORNER_RADIUS,
                            theme.input_border_focused,
                        );
                    }

                    // The value, left-aligned and vertically centred: a
                    // choice field shows exactly one line, so it sits in the
                    // middle rather than hanging off the top the way a
                    // multi-line text field does.
                    let value_text = lbox.text.clone().unwrap_or_default();
                    let fonts = &self.fonts;
                    let field = Rect {
                        x: rx + 1,
                        y: ry + 1,
                        // Room is left on the right for the chevron, so a
                        // long option runs out of room before it would run
                        // into the one sign that says the field opens.
                        w: (lbox.rect.w - 28).max(0),
                        h: (lbox.rect.h - 2).max(0),
                    };
                    let text_clip = match clip {
                        None => Some(field),
                        Some(c) => intersect(field, c),
                    };
                    let text_color = if lbox.disabled {
                        theme.text_dim
                    } else {
                        theme.text
                    };
                    if let Some(tc) = text_clip {
                        let ty = ry + ((lbox.rect.h - cell_h) / 2).max(4);
                        draw_text_ttf_clipped(
                            fonts,
                            FaceKind::Ui,
                            rx + 8,
                            ty,
                            &value_text,
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
                                    text_color,
                                    cov,
                                );
                            },
                        );
                    }

                    // The chevron is drawn from rectangles, not a glyph: a
                    // font need not carry ▾, and a field that lost the one
                    // mark saying it opens would be a field nobody dares
                    // click. It stays put even when the list is open — the
                    // mark describes the field, not the list.
                    let chevron_color = if lbox.disabled {
                        theme.text_dim
                    } else if is_open_here {
                        theme.input_border_focused
                    } else {
                        theme.text_dim
                    };
                    draw_chevron_down(
                        &mut buffer,
                        stride,
                        h_usize,
                        clip,
                        rx + lbox.rect.w - 20,
                        ry + (lbox.rect.h - 4) / 2,
                        chevron_color,
                    );
                }
                // A boolean control, drawn entirely from rectangles: the
                // square, the mark inside it, the track, the knob. The
                // drawing itself lives in `draw_flag_control` so it can be
                // judged without a window; the row's own work here is to
                // find the control's place and then paint the label beside
                // it, which is the part only the layout can know.
                "checkbox" | "toggle" => {
                    let key = flag_key(lbox, box_index);
                    let hover_t = if lbox.disabled {
                        0.0
                    } else {
                        self.hover_ramps.get(&key).copied().unwrap_or(0.0)
                    };
                    let flag = FlagView::new(
                        &lbox.tag,
                        Rect {
                            x: rx,
                            y: ry,
                            w: lbox.rect.w,
                            h: lbox.rect.h,
                        },
                        lbox.checked,
                        hover_t,
                        lbox.disabled,
                    );
                    draw_flag_control(&mut buffer, stride, h_usize, clip, &theme, &flag);
                    let is_focused =
                        lbox.id.is_some() && self.focused_id.as_deref() == lbox.id.as_deref();
                    if is_focused {
                        draw_focus_ring(
                            &mut buffer,
                            stride,
                            h_usize,
                            clip,
                            flag.rect.x,
                            flag.rect.y,
                            flag.rect.w,
                            flag.rect.h,
                            4,
                            theme.input_border_focused,
                        );
                    }

                    // The label belongs to the control and is drawn beside
                    // it in the same row the press covers.
                    let label_x = flag.rect.x + flag.rect.w + FLAG_GAP;
                    let label = lbox.text.clone().unwrap_or_default();
                    let label_color = if lbox.disabled {
                        theme.text_dim
                    } else {
                        theme.text
                    };
                    let region = Rect {
                        x: label_x,
                        y: ry,
                        w: (lbox.rect.w - flag.rect.w - FLAG_GAP).max(0),
                        h: lbox.rect.h,
                    };
                    let label_clip = match clip {
                        None => Some(region),
                        Some(c) => intersect(region, c),
                    };
                    if let Some(lc) = label_clip {
                        let fonts = &self.fonts;
                        let ty = ry + ((lbox.rect.h - cell_h) / 2).max(0);
                        draw_text_ttf_clipped(
                            fonts,
                            FaceKind::Ui,
                            label_x,
                            ty,
                            &label,
                            lc,
                            |px, py, w, h, cov| {
                                draw_rect_alpha(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    px,
                                    py,
                                    w,
                                    h,
                                    label_color,
                                    cov,
                                );
                            },
                        );
                    }
                }
                // A slider, drawn entirely from rounded rectangles: the groove,
                // the part of it travelled, the handle sitting on the value.
                // As with the flag, the drawing lives in `draw_slider_control`
                // so it can be judged without a window; the row's work here is
                // to find the track's place and paint the label beside it,
                // which is the part only the layout can know.
                "slider" => {
                    let key = slider_key(lbox, box_index);
                    let hover_t = if lbox.disabled {
                        0.0
                    } else {
                        self.hover_ramps.get(&key).copied().unwrap_or(0.0)
                    };
                    // The spec is read off the box — the snapshot layout took
                    // — rather than re-parsed out of the tree here, so the
                    // handle is drawn where the press and the written-back
                    // value also believe it is.
                    let Some(spec) = lbox.slider else {
                        continue;
                    };
                    let view = SliderView::new(
                        Rect {
                            x: rx,
                            y: ry,
                            w: lbox.rect.w,
                            h: lbox.rect.h,
                        },
                        hover_t,
                        lbox.disabled,
                        spec,
                    );
                    draw_slider_control(&mut buffer, stride, h_usize, clip, &theme, &view);
                    let is_focused =
                        lbox.id.is_some() && self.focused_id.as_deref() == lbox.id.as_deref();
                    if is_focused {
                        draw_focus_ring(
                            &mut buffer,
                            stride,
                            h_usize,
                            clip,
                            view.rect.x,
                            view.rect.y,
                            view.rect.w,
                            view.rect.h,
                            4,
                            theme.input_border_focused,
                        );
                    }

                    // The label belongs to the control and is drawn beside it
                    // in the same row the press covers.
                    let label_x = view.rect.x + view.rect.w + FLAG_GAP;
                    let label = lbox.text.clone().unwrap_or_default();
                    let label_color = if lbox.disabled {
                        theme.text_dim
                    } else {
                        theme.text
                    };
                    let region = Rect {
                        x: label_x,
                        y: ry,
                        w: (lbox.rect.w - view.rect.w - FLAG_GAP).max(0),
                        h: lbox.rect.h,
                    };
                    let label_clip = match clip {
                        None => Some(region),
                        Some(c) => intersect(region, c),
                    };
                    if let Some(lc) = label_clip {
                        let fonts = &self.fonts;
                        let ty = ry + ((lbox.rect.h - cell_h) / 2).max(0);
                        draw_text_ttf_clipped(
                            fonts,
                            FaceKind::Ui,
                            label_x,
                            ty,
                            &label,
                            lc,
                            |px, py, w, h, cov| {
                                draw_rect_alpha(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    px,
                                    py,
                                    w,
                                    h,
                                    label_color,
                                    cov,
                                );
                            },
                        );
                    }
                }
                "progress" => {
                    let Some(spec) = lbox.slider else {
                        continue;
                    };
                    let track_w = lbox
                        .props
                        .get("width")
                        .or_else(|| lbox.props.get("w"))
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or_else(|| {
                            if lbox.text.is_some() {
                                (lbox.rect.w - 60).max(40)
                            } else {
                                lbox.rect.w.min(PROGRESS_TRACK_W)
                            }
                        });
                    let track_r = Rect {
                        x: rx,
                        y: ry + ((lbox.rect.h - PROGRESS_TRACK_H) / 2).max(0),
                        w: track_w,
                        h: PROGRESS_TRACK_H,
                    };
                    draw_progress_control(
                        &mut buffer,
                        stride,
                        h_usize,
                        clip,
                        &theme,
                        track_r,
                        spec.value,
                        spec.min,
                        spec.max,
                        lbox.disabled,
                    );
                    if let Some(ref label) = lbox.text {
                        let label_x = track_r.x + track_r.w + FLAG_GAP;
                        let region = Rect {
                            x: label_x,
                            y: ry,
                            w: (lbox.rect.w - track_r.w - FLAG_GAP).max(0),
                            h: lbox.rect.h,
                        };
                        let label_clip = match clip {
                            None => Some(region),
                            Some(c) => intersect(region, c),
                        };
                        if let Some(lc) = label_clip {
                            let fonts = &self.fonts;
                            let ty = ry + ((lbox.rect.h - cell_h) / 2).max(0);
                            let label_color = if lbox.disabled {
                                theme.text_dim
                            } else {
                                theme.text
                            };
                            draw_text_ttf_clipped(
                                fonts,
                                FaceKind::Ui,
                                label_x,
                                ty,
                                label,
                                lc,
                                |px, py, w, h, cov| {
                                    draw_rect_alpha(
                                        &mut buffer,
                                        stride,
                                        h_usize,
                                        px,
                                        py,
                                        w,
                                        h,
                                        label_color,
                                        cov,
                                    );
                                },
                            );
                        }
                    }
                }
                "spinner" => {
                    let spinner_r = Rect {
                        x: rx,
                        y: ry + ((lbox.rect.h - SPINNER_SIZE) / 2).max(0),
                        w: SPINNER_SIZE,
                        h: SPINNER_SIZE,
                    };
                    let phase = match self.spinner_origin {
                        Some(origin) => {
                            (Instant::now().duration_since(origin).as_secs_f32()
                                / SPINNER_PERIOD_SECS)
                                % 1.0
                        }
                        None => 0.0,
                    };
                    draw_spinner_control(
                        &mut buffer,
                        stride,
                        h_usize,
                        clip,
                        &theme,
                        spinner_r,
                        lbox.disabled,
                        phase,
                    );
                    if let Some(ref label) = lbox.text {
                        let label_x = spinner_r.x + spinner_r.w + FLAG_GAP;
                        let region = Rect {
                            x: label_x,
                            y: ry,
                            w: (lbox.rect.w - spinner_r.w - FLAG_GAP).max(0),
                            h: lbox.rect.h,
                        };
                        let label_clip = match clip {
                            None => Some(region),
                            Some(c) => intersect(region, c),
                        };
                        if let Some(lc) = label_clip {
                            let fonts = &self.fonts;
                            let ty = ry + ((lbox.rect.h - cell_h) / 2).max(0);
                            let label_color = if lbox.disabled {
                                theme.text_dim
                            } else {
                                theme.text
                            };
                            draw_text_ttf_clipped(
                                fonts,
                                FaceKind::Ui,
                                label_x,
                                ty,
                                label,
                                lc,
                                |px, py, w, h, cov| {
                                    draw_rect_alpha(
                                        &mut buffer,
                                        stride,
                                        h_usize,
                                        px,
                                        py,
                                        w,
                                        h,
                                        label_color,
                                        cov,
                                    );
                                },
                            );
                        }
                    }
                }
                "image" | "icon" => {
                    let dest = Rect {
                        x: rx,
                        y: ry,
                        w: lbox.rect.w,
                        h: lbox.rect.h,
                    };
                    if let Some(ref path) = lbox.text {
                        if !self.image_cache.contains_key(path) {
                            if let Ok(bytes) = std::fs::read(path) {
                                if let Ok(decoded) = crate::ui::decode_png(&bytes) {
                                    self.image_cache.insert(path.clone(), decoded);
                                }
                            }
                        }
                        if let Some(img) = self.image_cache.get(path) {
                            let cr = if lbox.tag == "image" { 6 } else { 0 };
                            let alpha = if lbox.disabled { 0.5 } else { 1.0 };
                            draw_image_rgba(
                                &mut buffer,
                                stride,
                                h_usize,
                                clip,
                                dest,
                                img,
                                cr,
                                alpha,
                            );
                        }
                    }
                }
                "tabs" => {
                    let active = lbox.text.as_deref().unwrap_or("");
                    let mut cur_x = rx;
                    let cell_h = self.fonts.cell_h;
                    let accent = theme.input_border_focused;

                    for opt in &lbox.options {
                        let tw = opt.chars().count() as i32 * CELL_W + 24;
                        let tab_rect = Rect {
                            x: cur_x,
                            y: ry,
                            w: tw,
                            h: lbox.rect.h,
                        };
                        let is_active = opt == active;

                        // Tab background
                        let (bg_color, border_color) = if is_active {
                            (theme.input_bg, accent)
                        } else {
                            (theme.bg, theme.border)
                        };
                        draw_rounded_rect_box_clipped(
                            &mut buffer,
                            stride,
                            h_usize,
                            clip,
                            tab_rect.x,
                            tab_rect.y,
                            tab_rect.w,
                            tab_rect.h,
                            4,
                            bg_color,
                            border_color,
                        );

                        // Active bottom indicator bar
                        if is_active {
                            let ind_h = 2;
                            draw_rect_alpha(
                                &mut buffer,
                                stride,
                                h_usize,
                                tab_rect.x + 2,
                                tab_rect.y + tab_rect.h - ind_h - 1,
                                tab_rect.w - 4,
                                ind_h,
                                accent,
                                1.0,
                            );
                        }

                        // Tab text label
                        let text_color = if is_active {
                            theme.text
                        } else {
                            theme.text_dim
                        };
                        let ty = ry + ((lbox.rect.h - cell_h) / 2).max(0);
                        let tx = cur_x + 12;
                        let region = Rect {
                            x: tx,
                            y: ry,
                            w: (tw - 16).max(0),
                            h: lbox.rect.h,
                        };
                        let text_clip = match clip {
                            None => Some(region),
                            Some(c) => intersect(region, c),
                        };
                        if let Some(tc) = text_clip {
                            let fonts = &self.fonts;
                            draw_text_ttf_clipped(
                                fonts,
                                FaceKind::Ui,
                                tx,
                                ty,
                                opt,
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
                                        text_color,
                                        cov,
                                    );
                                },
                            );
                        }

                        cur_x += tw;
                    }
                }
                "code_block" => {
                    let code = lbox.text.as_deref().unwrap_or("");
                    let lines: Vec<&str> = code.split('\n').collect();
                    let bg_color = theme.input_bg;
                    let border_color = theme.border;
                    let radius = 6;

                    // Container background & border
                    draw_rounded_rect_box_clipped(
                        &mut buffer,
                        stride,
                        h_usize,
                        clip,
                        rx,
                        ry,
                        lbox.rect.w,
                        lbox.rect.h,
                        radius,
                        bg_color,
                        border_color,
                    );

                    let gutter_w = 40;
                    // Gutter separator line
                    let sep_rect = Rect {
                        x: rx + gutter_w,
                        y: ry + 1,
                        w: 1,
                        h: (lbox.rect.h - 2).max(0),
                    };
                    if let Some(sc) = match clip {
                        None => Some(sep_rect),
                        Some(c) => intersect(sep_rect, c),
                    } {
                        draw_rect(
                            &mut buffer,
                            stride,
                            h_usize,
                            sc.x,
                            sc.y,
                            sc.w,
                            sc.h,
                            theme.border,
                        );
                    }

                    let line_h = 20;
                    let pad_top = 10;
                    let fonts = &self.fonts;

                    // Draw each line: line number in gutter + highlighted code tokens
                    for (line_idx, line_str) in lines.iter().enumerate() {
                        let line_y = ry + pad_top + (line_idx as i32 * line_h);
                        if line_y + line_h < ry || line_y > ry + lbox.rect.h {
                            continue;
                        }

                        // 1. Line number in gutter
                        let num_str = format!("{}", line_idx + 1);
                        let num_w = fonts.measure_width(FaceKind::Mono, &num_str);
                        let num_x = rx + gutter_w - 8 - num_w;
                        let num_clip = match clip {
                            None => Some(Rect {
                                x: rx + 2,
                                y: ry + 2,
                                w: gutter_w - 4,
                                h: (lbox.rect.h - 4).max(0),
                            }),
                            Some(c) => intersect(
                                Rect {
                                    x: rx + 2,
                                    y: ry + 2,
                                    w: gutter_w - 4,
                                    h: (lbox.rect.h - 4).max(0),
                                },
                                c,
                            ),
                        };
                        if let Some(nc) = num_clip {
                            draw_text_ttf_clipped(
                                fonts,
                                FaceKind::Mono,
                                num_x,
                                line_y,
                                &num_str,
                                nc,
                                |px, py, w, h, cov| {
                                    draw_rect_alpha(
                                        &mut buffer,
                                        stride,
                                        h_usize,
                                        px,
                                        py,
                                        w,
                                        h,
                                        theme.text_dim,
                                        cov,
                                    );
                                },
                            );
                        }

                        // 2. Code tokens in body
                        let code_area = Rect {
                            x: rx + gutter_w + 12,
                            y: ry + 2,
                            w: (lbox.rect.w - gutter_w - 16).max(0),
                            h: (lbox.rect.h - 4).max(0),
                        };
                        let code_clip = match clip {
                            None => Some(code_area),
                            Some(c) => intersect(code_area, c),
                        };

                        if let Some(cc) = code_clip {
                            let mut tok_x = rx + gutter_w + 12;
                            let tokens = tokenize_code_line(line_str);
                            for (tok_text, tok_kind) in tokens {
                                let tok_color = match tok_kind {
                                    SyntaxTokenKind::Keyword => theme.input_border_focused, // Blue accent
                                    SyntaxTokenKind::String => 0xFF10_B981, // Green Emerald-500
                                    SyntaxTokenKind::Number => 0xFFF5_9E0B, // Amber-500
                                    SyntaxTokenKind::Comment => theme.text_dim, // Muted grey
                                    SyntaxTokenKind::Punctuation => 0xFF94_A3B8, // Slate-400
                                    SyntaxTokenKind::Ident => theme.text,
                                    SyntaxTokenKind::Whitespace => theme.text,
                                };

                                draw_text_ttf_clipped(
                                    fonts,
                                    FaceKind::Mono,
                                    tok_x,
                                    line_y,
                                    tok_text,
                                    cc,
                                    |px, py, w, h, cov| {
                                        draw_rect_alpha(
                                            &mut buffer,
                                            stride,
                                            h_usize,
                                            px,
                                            py,
                                            w,
                                            h,
                                            tok_color,
                                            cov,
                                        );
                                    },
                                );

                                tok_x += fonts.prefix_width(
                                    FaceKind::Mono,
                                    tok_text,
                                    tok_text.chars().count(),
                                );
                            }
                        }
                    }

                    // Top-right "Copy" badge pill
                    let copy_w = 48;
                    let copy_h = 18;
                    let copy_x = rx + lbox.rect.w - copy_w - 8;
                    let copy_y = ry + 6;
                    let copy_rect = Rect {
                        x: copy_x,
                        y: copy_y,
                        w: copy_w,
                        h: copy_h,
                    };
                    if let Some(cc) = match clip {
                        None => Some(copy_rect),
                        Some(c) => intersect(copy_rect, c),
                    } {
                        draw_rounded_rect_box_clipped(
                            &mut buffer,
                            stride,
                            h_usize,
                            Some(cc),
                            copy_x,
                            copy_y,
                            copy_w,
                            copy_h,
                            3,
                            theme.selection_bg,
                            theme.input_border_focused,
                        );
                        let copy_tw = fonts.measure_width(FaceKind::Ui, "Copy");
                        let copy_tx = copy_x + (copy_w - copy_tw) / 2;
                        let copy_ty = copy_y + (copy_h - fonts.cell_h) / 2;
                        draw_text_ttf_clipped(
                            fonts,
                            FaceKind::Ui,
                            copy_tx,
                            copy_ty,
                            "Copy",
                            cc,
                            |px, py, w, h, cov| {
                                draw_rect_alpha(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    px,
                                    py,
                                    w,
                                    h,
                                    theme.button_text,
                                    cov,
                                );
                            },
                        );
                    }
                }
                "divider" => {
                    let is_vert =
                        lbox.props.get("orientation").map(|s| s.as_str()) == Some("vertical");
                    let line_rect = if is_vert {
                        Rect {
                            x: rx + lbox.rect.w / 2,
                            y: ry,
                            w: 1,
                            h: lbox.rect.h,
                        }
                    } else {
                        Rect {
                            x: rx,
                            y: ry + lbox.rect.h / 2,
                            w: lbox.rect.w,
                            h: 1,
                        }
                    };
                    if let Some(lc) = match clip {
                        None => Some(line_rect),
                        Some(c) => intersect(line_rect, c),
                    } {
                        draw_rect(
                            &mut buffer,
                            stride,
                            h_usize,
                            lc.x,
                            lc.y,
                            lc.w,
                            lc.h,
                            theme.border,
                        );
                    }
                }
                "panel" => {
                    let bg_color = match lbox.props.get("bg").map(|s| s.as_str()) {
                        Some("sidebar") => {
                            if theme.is_dark() {
                                0xFF1E_2022
                            } else {
                                0xFFE3_E5E8
                            }
                        }
                        Some("toolbar") => {
                            if theme.is_dark() {
                                0xFF24_272B
                            } else {
                                0xFFE9_EBEC
                            }
                        }
                        Some("table") | Some("view") => {
                            if theme.is_dark() {
                                0xFF2A_2E32
                            } else {
                                0xFFFF_FFFF
                            }
                        }
                        Some("card") => {
                            if theme.is_dark() {
                                0xFF31_363B
                            } else {
                                0xFFF5_F6F7
                            }
                        }
                        Some(hex) if hex.starts_with('#') => {
                            parse_hex_color(hex).unwrap_or(theme.bg)
                        }
                        _ => theme.bg,
                    };
                    let rect = Rect {
                        x: rx,
                        y: ry,
                        w: lbox.rect.w,
                        h: lbox.rect.h,
                    };
                    if let Some(rc) = match clip {
                        None => Some(rect),
                        Some(c) => intersect(rect, c),
                    } {
                        draw_rect(
                            &mut buffer,
                            stride,
                            h_usize,
                            rc.x,
                            rc.y,
                            rc.w,
                            rc.h,
                            bg_color,
                        );
                        if let Some(bc_str) = lbox.props.get("border") {
                            let bc = if bc_str == "true" {
                                theme.border
                            } else {
                                parse_hex_color(bc_str).unwrap_or(theme.border)
                            };
                            draw_rect_outline(
                                &mut buffer,
                                stride,
                                h_usize,
                                rc.x,
                                rc.y,
                                rc.w,
                                rc.h,
                                bc,
                            );
                        }
                    }
                }
                "spacer" => {}
                "badge" | "tag" => {
                    let text = lbox.text.as_deref().unwrap_or("");
                    let (bg, border, text_color) = match text.to_ascii_uppercase().as_str() {
                        "YOU" | "USER" => (theme.button_bg, theme.button_border, theme.button_text),
                        "NUDGE" | "AI" | "ASSISTANT" => (
                            0xFF10_B981, // Emerald-500
                            0xFF34_D399, // Emerald-400
                            0xFFFF_FFFF,
                        ),
                        "SYSTEM" | "WARN" | "WARNING" => (
                            0xFFF5_9E0B, // Amber-500
                            0xFFFBBF24,  // Amber-400
                            0xFF00_0000,
                        ),
                        _ => (theme.input_bg, theme.border, theme.text),
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
                        4,
                        bg,
                        border,
                    );

                    let fonts = &self.fonts;
                    let cell_h = fonts.cell_h;
                    let text_w = fonts.measure_width(FaceKind::Ui, text);
                    let tx = rx + ((lbox.rect.w - text_w) / 2).max(4);
                    let ty = ry + ((lbox.rect.h - cell_h) / 2).max(1);

                    let text_region = Rect {
                        x: rx + 2,
                        y: ry + 1,
                        w: (lbox.rect.w - 4).max(0),
                        h: (lbox.rect.h - 2).max(0),
                    };
                    let text_clip = match clip {
                        None => Some(text_region),
                        Some(c) => intersect(text_region, c),
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
                                    text_color,
                                    cov,
                                );
                            },
                        );
                    }
                }
                "status_line" => {
                    let text = lbox.text.as_deref().unwrap_or("");
                    let status = lbox.options.first().map(|s| s.as_str()).unwrap_or("info");
                    let indicator_color = match status {
                        "error" => 0xFFEF_4444,            // Red-500
                        "warning" | "warn" => 0xFFF5_9E0B, // Amber-500
                        "success" => 0xFF10_B981,          // Emerald-500
                        _ => 0xFF3B_82F6,                  // Blue-500 (info)
                    };

                    // Background container
                    draw_rounded_rect_box_clipped(
                        &mut buffer,
                        stride,
                        h_usize,
                        clip,
                        rx,
                        ry,
                        lbox.rect.w,
                        lbox.rect.h,
                        4,
                        theme.input_bg,
                        theme.border,
                    );

                    // Left indicator pill / bar
                    let bar_w = 4;
                    let bar_rect = Rect {
                        x: rx + 2,
                        y: ry + 4,
                        w: bar_w,
                        h: (lbox.rect.h - 8).max(0),
                    };
                    if let Some(bc) = match clip {
                        None => Some(bar_rect),
                        Some(c) => intersect(bar_rect, c),
                    } {
                        draw_rounded_rect_box_clipped(
                            &mut buffer,
                            stride,
                            h_usize,
                            Some(bc),
                            bc.x,
                            bc.y,
                            bc.w,
                            bc.h,
                            2,
                            indicator_color,
                            indicator_color,
                        );
                    }

                    // Status text label
                    let fonts = &self.fonts;
                    let cell_h = fonts.cell_h;
                    let tx = rx + 16;
                    let ty = ry + ((lbox.rect.h - cell_h) / 2).max(1);
                    let text_region = Rect {
                        x: tx,
                        y: ry + 1,
                        w: (lbox.rect.w - 20).max(0),
                        h: (lbox.rect.h - 2).max(0),
                    };
                    if let Some(tc) = match clip {
                        None => Some(text_region),
                        Some(c) => intersect(text_region, c),
                    } {
                        draw_rich_line(
                            fonts,
                            &theme,
                            tx,
                            ty,
                            text,
                            theme.text,
                            Some(tc),
                            |px, py, w, h, col, cov| {
                                draw_rect_alpha(
                                    &mut buffer,
                                    stride,
                                    h_usize,
                                    px,
                                    py,
                                    w,
                                    h,
                                    col,
                                    cov,
                                );
                            },
                        );
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
            let is_dragging = self
                .region_scrollbar_drag
                .as_ref()
                .is_some_and(|(k, ..)| k == &r.key);
            let is_hovered = self.cursor_pos.0 >= vx + vw - 14
                && self.cursor_pos.0 <= vx + vw
                && self.cursor_pos.1 >= vy
                && self.cursor_pos.1 <= vy + vh;
            // Draw subtle track
            draw_rounded_rect_box(
                &mut buffer,
                stride,
                h_usize,
                vx + vw - 8,
                vy + 4,
                6,
                track_h,
                3,
                theme.scrollbar_track,
                theme.scrollbar_track,
            );
            // Draw rounded thumb
            let thumb_col = if is_dragging {
                theme.selection_bg
            } else if is_hovered {
                lerp_color(theme.scrollbar_thumb, theme.text, 0.4)
            } else {
                theme.scrollbar_thumb
            };
            draw_rounded_rect_box(
                &mut buffer,
                stride,
                h_usize,
                vx + vw - 8,
                thumb_y,
                6,
                thumb_h,
                3,
                thumb_col,
                thumb_col,
            );
        }

        // Window scrollbars. Both axes go through the same geometry the
        // pointer hit-tests against, and the tone lifts when a bar is hovered
        // or held so the thing that will move is visible before it does.
        if let Some((track, thumb)) = scrollbars.vertical {
            let (thumb_c, track_c) = scrollbar_tones(
                &theme,
                bar_hover == Some(ScrollbarAxis::Vertical),
                bar_dragging && bar_hover == Some(ScrollbarAxis::Vertical),
            );
            draw_rounded_rect_box(
                &mut buffer,
                stride,
                h_usize,
                track.x,
                track.y,
                track.w,
                track.h,
                4,
                track_c,
                track_c,
            );
            draw_rounded_rect_box(
                &mut buffer,
                stride,
                h_usize,
                thumb.x,
                thumb.y,
                thumb.w,
                thumb.h,
                3,
                thumb_c,
                thumb_c,
            );
        }
        if let Some((track, thumb)) = scrollbars.horizontal {
            let (thumb_c, track_c) = scrollbar_tones(
                &theme,
                bar_hover == Some(ScrollbarAxis::Horizontal),
                bar_dragging && bar_hover == Some(ScrollbarAxis::Horizontal),
            );
            draw_rounded_rect_box(
                &mut buffer,
                stride,
                h_usize,
                track.x,
                track.y,
                track.w,
                track.h,
                4,
                track_c,
                track_c,
            );
            draw_rounded_rect_box(
                &mut buffer,
                stride,
                h_usize,
                thumb.x,
                thumb.y,
                thumb.w,
                thumb.h,
                3,
                thumb_c,
                thumb_c,
            );
        }

        // The open choice list, painted after every box and after both
        // scrollbars so it layers over whatever it drops onto, but before the
        // tooltip — which is suppressed while a list is up, so nothing can
        // cover the card the user is picking from.
        if let Some(popup) = &popup {
            draw_select_popup(
                &mut buffer,
                stride,
                h_usize,
                &self.fonts,
                &theme,
                popup,
                popup_hover,
            );
        }

        // Modal Command Palette, painted over standard content
        if let Some(ref pal) = self.command_palette {
            draw_command_palette(
                &mut buffer,
                stride,
                h_usize,
                &self.fonts,
                &theme,
                self.width as i32,
                self.height as i32,
                pal,
            );
        }

        // Draw drop target insertion line if dragging
        if let Some(ref drag) = self.drag_state {
            if drag.active {
                if let Some(ref target_id) = drag.drop_target {
                    if let Some(target_box) = self
                        .layout_boxes
                        .iter()
                        .find(|b| b.id.as_deref() == Some(target_id))
                    {
                        let sr = target_box.screen_rect(self.scroll_x, self.scroll_y);
                        draw_rect(
                            &mut buffer,
                            stride,
                            h_usize,
                            sr.x,
                            (sr.y - 2).max(0),
                            sr.w,
                            3,
                            theme.input_border_focused,
                        );
                    }
                }
                // Floating ghost pill
                let label = drag.source_text.as_deref().unwrap_or(&drag.source_id);
                draw_drag_ghost(
                    &mut buffer,
                    stride,
                    h_usize,
                    &self.fonts,
                    &theme,
                    drag.current_pos,
                    label,
                );
            }
        }

        // Draw file drop overlay if file is hovered
        if let Some(ref file) = self.hovered_file {
            draw_file_drop_overlay(
                &mut buffer,
                stride,
                h_usize,
                &self.fonts,
                &theme,
                self.width as i32,
                self.height as i32,
                file,
            );
        }

        // Floating Context Menu overlay
        if let Some(ref menu) = self.context_menu {
            draw_context_menu(&mut buffer, stride, h_usize, &self.fonts, &theme, menu);
        }

        // Modal Settings Dialog window overlay
        if self.settings_dialog.open {
            draw_settings_dialog(
                &mut buffer,
                stride,
                h_usize,
                &self.fonts,
                &theme,
                self.width as i32,
                self.height as i32,
                &self.settings_dialog,
            );
        }

        // Floating Toast Notification overlay
        if let Some(ref toast) = self.toast {
            draw_toast_notification(
                &mut buffer,
                stride,
                h_usize,
                &self.fonts,
                &theme,
                self.width as i32,
                toast,
            );
        }

        // Active Error Banner overlay (Section 5.1)
        if let Some(ref err) = self.current_error {
            draw_error_banner(
                &mut buffer,
                stride,
                h_usize,
                &self.fonts,
                self.width as i32,
                err,
            );
        }

        // Active Async LLM / Streaming Status Bar (Section 5.2)
        if self.async_state.in_flight {
            let phase = self.spinner_origin.map_or(0.0, |t| {
                (Instant::now().duration_since(t).as_secs_f32() % SPINNER_PERIOD_SECS)
                    / SPINNER_PERIOD_SECS
            });
            draw_async_status_bar(
                &mut buffer,
                stride,
                h_usize,
                &self.fonts,
                &theme,
                self.width as i32,
                self.height as i32,
                &self.async_state,
                phase,
            );
        }

        // Tooltip overlay, painted last so it layers over every box, including
        // those inside a scroll viewport that would otherwise clip it. Asked
        // for above, while `self` could still be read — and already made to
        // yield to a scrollbar, like hover and the press.
        if let Some(tip) = tooltip {
            draw_tooltip(
                &mut buffer,
                stride,
                h_usize,
                &self.fonts,
                &theme,
                self.width as i32,
                self.height as i32,
                mx,
                my,
                &tip,
            );
        }

        if self.hud_mode || std::env::var("NUDGE_HUD").as_deref() == Ok("1") {
            let _ = crate::jarvis_hud::with_hud(|h| {
                h.advance_tick();
                h.render_to_u32_buffer(&mut buffer, stride, h_usize);
                Ok(())
            });
        }

        self.last_frame = buffer.to_vec();
        self.render_count += 1;
        let _ = buffer.present();
        self.clear_dirty();
    }

    pub fn handle_click(&mut self) {
        let (mx, my) = self.cursor_pos;

        // Error banner owns click if active (Section 5.1)
        if let Some(ref err) = self.current_error {
            let banner_h = 38;
            let banner_w = (self.width as i32 - 40).max(200);
            let banner_x = 20;
            let banner_y = 10;
            if mx >= banner_x
                && mx < banner_x + banner_w
                && my >= banner_y
                && my < banner_y + banner_h
            {
                // Check Close '✕' on far right
                let close_x = banner_x + banner_w - 28;
                if mx >= close_x {
                    self.clear_error();
                    return;
                }
                // Check Retry button if retryable
                if err.retryable {
                    let retry_w = 80;
                    let retry_x = banner_x + banner_w - retry_w - 32;
                    if mx >= retry_x && mx < retry_x + retry_w {
                        self.trigger_error_retry();
                        return;
                    }
                }
                return;
            }
        }

        // Async status bar Cancel button click (Section 5.2)
        if self.async_state.in_flight {
            let bar_h = 34;
            let bar_w = (self.width as i32 - 40).max(200);
            let bar_x = 20;
            let bar_y = self.height as i32 - bar_h - 14;
            let cancel_w = 70;
            let cancel_h = 24;
            let cancel_x = bar_x + bar_w - cancel_w - 8;
            let cancel_y = bar_y + (bar_h - cancel_h) / 2;
            if mx >= cancel_x
                && mx < cancel_x + cancel_w
                && my >= cancel_y
                && my < cancel_y + cancel_h
            {
                self.cancel_async_task();
                return;
            }
        }

        // Context menu owns click before anything else
        if let Some(menu) = self.context_menu.take() {
            let r = menu.menu_rect();
            if r.contains(mx, my) {
                let row = ((my - (r.y + 6)).max(0) / 28) as usize;
                if let Some(item) = menu.items.get(row) {
                    if item.id == "copy" {
                        if let Some(ref text) = menu.target_text {
                            self.clipboard = text.clone();
                            self.show_toast("Message copied to clipboard!", "info");
                        }
                    } else if item.id == "export_md" {
                        let _ = self.export_chat_to_file("md", "/tmp/opencode/chat_export.md");
                    } else if item.id == "export_json" {
                        let _ = self.export_chat_to_file("json", "/tmp/opencode/chat_export.json");
                    } else if item.id == "export_png" {
                        let _ = self.export_chat_to_file("png", "/tmp/opencode/chat_export.png");
                    } else if item.id == "settings" {
                        self.open_settings_dialog();
                    } else if item.id == "popout" {
                        let title = menu
                            .target_text
                            .clone()
                            .unwrap_or_else(|| "Separate Chat Window".into());
                        self.request_spawn_window(&title, 480, 360, menu.target_id.clone());
                    } else if item.id == "new_window" {
                        self.request_spawn_window("Nudge Studio — Chat", 500, 400, None);
                    }
                    let target_str = menu.target_id.as_deref().unwrap_or("window");
                    self.dispatcher
                        .dispatch_change(target_str, &format!("context:{}", item.id));
                    self.dispatcher
                        .dispatch_key(&format!("ContextMenu:{}:{target_str}", item.id), None);
                }
            }
            self.render();
            return;
        }

        // Command palette owns click before anything else
        if let Some(pal) = self.command_palette.take() {
            let card_w = 400.min(self.width as i32 - 40).max(200);
            let card_x = (self.width as i32 - card_w) / 2;
            let card_y = 40;
            let filtered = pal.filtered_commands();
            let row_h = 32;
            let search_h = 40;
            let shown_rows = filtered.len().min(6);
            let card_h = search_h + (shown_rows as i32 * row_h) + 12;

            if mx >= card_x && mx < card_x + card_w && my >= card_y && my < card_y + card_h {
                let list_y = card_y + search_h + 6;
                if my >= list_y {
                    let clicked_row = ((my - list_y) / row_h) as usize;
                    if let Some(&cmd) = filtered.get(clicked_row) {
                        if cmd == "New Chat Window" {
                            self.request_spawn_window("Nudge Studio — Chat", 500, 400, None);
                        } else if cmd == "Open Settings" {
                            self.open_settings_dialog();
                        } else if cmd == "Export Chat as Markdown" {
                            let _ = self.export_chat_to_file("md", "/tmp/opencode/chat_export.md");
                        } else if cmd == "Export Chat as JSON" {
                            let _ =
                                self.export_chat_to_file("json", "/tmp/opencode/chat_export.json");
                        } else if cmd == "Export Chat as PNG" {
                            let _ =
                                self.export_chat_to_file("png", "/tmp/opencode/chat_export.png");
                        }
                        self.dispatcher
                            .dispatch_key(&format!("Command:{cmd}"), None);
                    }
                }
            }
            // A click anywhere dismisses the palette
            self.render();
            return;
        }

        // Modal Settings Dialog owns clicks when active
        if self.settings_dialog.open {
            let win_w = self.width as i32;
            let win_h = self.height as i32;
            let card_w = 420.min(win_w - 40).max(280);
            let card_h = 300.min(win_h - 40).max(200);
            let card_x = (win_w - card_w) / 2;
            let card_y = (win_h - card_h) / 2;

            // Click outside card closes dialog
            if mx < card_x || mx >= card_x + card_w || my < card_y || my >= card_y + card_h {
                self.close_settings_dialog();
                return;
            }

            // Close 'x' button (top-right of card)
            if mx >= card_x + card_w - 36
                && mx < card_x + card_w - 8
                && my >= card_y + 4
                && my < card_y + 36
            {
                self.close_settings_dialog();
                return;
            }

            // Tabs bar: tab_y = card_y + 44..card_y + 70
            let tab_y = card_y + 44;
            if my >= tab_y && my < tab_y + 28 {
                let tab_w = 90;
                for i in 0..3 {
                    let tx = card_x + 16 + (i as i32 * (tab_w + 8));
                    if mx >= tx && mx < tx + tab_w {
                        self.settings_dialog.active_tab = i;
                        self.dispatcher
                            .dispatch_key(&format!("SettingsTab:{i}"), None);
                        self.render();
                        return;
                    }
                }
            }

            // Content area
            let content_y = tab_y + 36;
            match self.settings_dialog.active_tab {
                0 => {
                    // Dark theme toggle
                    let row1_y = content_y + 8;
                    if my >= row1_y && my < row1_y + 26 {
                        self.settings_dialog.dark_theme = !self.settings_dialog.dark_theme;
                        let val = self.settings_dialog.dark_theme;
                        self.dispatcher
                            .dispatch_change("dark_theme", if val { "true" } else { "false" });
                        self.dispatcher
                            .dispatch_key(&format!("SettingsChange:dark_theme:{val}"), None);
                        self.render();
                        return;
                    }
                    // Stream output toggle
                    let row2_y = content_y + 42;
                    if my >= row2_y && my < row2_y + 26 {
                        self.settings_dialog.stream_response =
                            !self.settings_dialog.stream_response;
                        let val = self.settings_dialog.stream_response;
                        self.dispatcher
                            .dispatch_change("stream_response", if val { "true" } else { "false" });
                        self.dispatcher
                            .dispatch_key(&format!("SettingsChange:stream:{val}"), None);
                        self.render();
                        return;
                    }
                    // Language toggle / cycle
                    let row3_y = content_y + 76;
                    if my >= row3_y && my < row3_y + 26 {
                        self.toggle_locale();
                        return;
                    }
                }
                1 => {
                    // Model cycling
                    let row1_y = content_y + 8;
                    if my >= row1_y && my < row1_y + 26 {
                        let next_model = match self.settings_dialog.model.as_str() {
                            "gemini-3.8-flash" => "claude-3-5-sonnet",
                            "claude-3-5-sonnet" => "gpt-4o",
                            _ => "gemini-3.8-flash",
                        };
                        self.settings_dialog.model = next_model.to_string();
                        self.dispatcher.dispatch_change("model", next_model);
                        self.dispatcher
                            .dispatch_key(&format!("SettingsChange:model:{next_model}"), None);
                        self.render();
                        return;
                    }
                    // Temperature toggle / step
                    let row2_y = content_y + 42;
                    if my >= row2_y && my < row2_y + 26 {
                        let next_temp = if self.settings_dialog.temperature >= 0.9 {
                            0.2
                        } else {
                            self.settings_dialog.temperature + 0.2
                        };
                        self.settings_dialog.temperature = next_temp;
                        let s = format!("{next_temp:.1}");
                        self.dispatcher.dispatch_change("temperature", &s);
                        self.dispatcher
                            .dispatch_key(&format!("SettingsChange:temp:{s}"), None);
                        self.render();
                        return;
                    }
                }
                _ => {}
            }

            // Save & Close button (footer)
            let footer_y = card_y + card_h - 44;
            let btn_w = 100;
            let btn_x = card_x + card_w - btn_w - 16;
            if mx >= btn_x && mx < btn_x + btn_w && my >= footer_y && my < footer_y + 32 {
                self.close_settings_dialog();
                self.show_toast("Settings saved!", "success");
                return;
            }

            self.render();
            return;
        }

        // An open list owns the press before any box does: it is drawn over
        // everything, so letting the box loop run first would fire whatever
        // control happens to sit behind the card. A row commits its option; a
        // press anywhere else — on the field again, or outside it — only shuts
        // the list, and is spent either way. That is what stops a second
        // press from reaching a button hiding under the card.
        if self.open_select.is_some() {
            match self.select_row_hit((mx, my)) {
                Some(index) => {
                    self.commit_select(index);
                }
                None => {
                    self.close_select();
                }
            }
            return;
        }

        let mut clicked_button = None;
        let mut clicked_input = None;
        let mut clicked_select = None;
        // The id of a boolean control to flip, and the value it lands on.
        let mut clicked_flag: Option<(String, bool)> = None;
        let mut clicked_tab: Option<(String, String)> = None;
        let mut clicked_code: Option<(String, String)> = None;

        for (box_index, lbox) in self.layout_boxes.iter().enumerate() {
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
                if !lbox.disabled {
                    // Resolve the key exactly the way hover and the pressed
                    // tone do (id first, then `on_click`), so the box that lit
                    // up is the one that fires. A button with neither never
                    // dispatches — it is decoration.
                    clicked_button = lbox.id.clone().or_else(|| lbox.on_click.clone());
                    if let Some(ref id) = lbox.id {
                        self.focused_id = Some(id.clone());
                    }
                }
                // Either way the press stops here: a disabled control absorbs
                // the click instead of letting it fall through to a control
                // stacked behind it.
                break;
            } else if lbox.tag == "input" {
                if !lbox.disabled {
                    clicked_input = Some(lbox.clone());
                }
                break;
            } else if lbox.tag == "select" {
                if !lbox.disabled {
                    // Same absorption rule as a button: a disabled field takes
                    // the press and opens nothing, rather than letting it fall
                    // through to whatever is stacked behind it.
                    clicked_select = Some(select_key(lbox, box_index));
                }
                break;
            } else if matches!(lbox.tag.as_str(), "checkbox" | "toggle") {
                // The whole row — square or track *and* the label beside it —
                // is the control, so the words toggle it too. An id-less one
                // still absorbs the press: it has nowhere to report, so the
                // press must not reach whatever is stacked behind it.
                if !lbox.disabled {
                    if let Some(id) = lbox.id.clone() {
                        clicked_flag = Some((id, !lbox.checked));
                    }
                }
                break;
            } else if lbox.tag == "tabs" {
                if !lbox.disabled {
                    if let Some(id) = lbox.id.clone() {
                        let rel_x = mx - sr.x;
                        let mut cur_x = 0;
                        for opt in &lbox.options {
                            let tw = opt.chars().count() as i32 * CELL_W + 24;
                            if rel_x >= cur_x && rel_x < cur_x + tw {
                                clicked_tab = Some((id.clone(), opt.clone()));
                                break;
                            }
                            cur_x += tw;
                        }
                    }
                }
                break;
            } else if lbox.tag == "code_block" {
                if !lbox.disabled {
                    let code_id = lbox.id.clone().unwrap_or_else(|| "code".into());
                    let code_text = lbox.text.clone().unwrap_or_default();
                    clicked_code = Some((code_id, code_text));
                }
                break;
            }
        }

        if let Some(button_id) = clicked_button {
            self.dispatcher.dispatch_click(&button_id);
            self.render();
        } else if let Some(key) = clicked_select {
            // Opening a choice field takes focus away from any text field:
            // the caret must not keep blinking under a list that now owns
            // the keyboard.
            self.focused_id = None;
            self.open_select(&key);
        } else if let Some((id, next)) = clicked_flag {
            // The flip is reported as a change, not a click: the same route a
            // text field takes, so one handler name covers both kinds of
            // control and there is no second event to write.
            self.dispatcher
                .dispatch_change(&id, if next { "true" } else { "false" });
            self.render();
        } else if let Some((id, chosen)) = clicked_tab {
            self.dispatcher.dispatch_change(&id, &chosen);
            self.render();
        } else if let Some((code_id, code_text)) = clicked_code {
            self.clipboard = code_text;
            self.show_toast("Code copied to clipboard!", "success");
            self.dispatcher
                .dispatch_key(&format!("CopyCode:{code_id}"), None);
            self.dispatcher.dispatch_change(&code_id, "copied");
            self.render();
        } else if let Some(lbox) = clicked_input {
            // Map the click back into content space for caret placement.
            let cx = mx + self.scroll_x + lbox.pan.0;
            let cy = my + self.scroll_y + lbox.pan.1;
            if let Some(ref id) = lbox.id {
                self.focused_id = Some(id.clone());
                // Placing the caret restarts its blink: the first thing a
                // click in a field has to do is show where it landed.
                self.touch_caret();
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
        // Any key at all restarts the caret's visible phase: a caret that
        // happens to be mid-blink when the user types would look like the
        // keystroke did nothing.
        self.touch_caret();

        // Context menu owns keystrokes when active
        if self.context_menu.is_some() {
            match logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.context_menu = None;
                    self.render();
                    return;
                }
                Key::Named(NamedKey::ArrowUp) => {
                    if let Some(menu) = self.context_menu.as_mut() {
                        menu.highlight = menu.highlight.saturating_sub(1);
                    }
                    self.render();
                    return;
                }
                Key::Named(NamedKey::ArrowDown) => {
                    if let Some(menu) = self.context_menu.as_mut() {
                        let count = menu.items.len();
                        menu.highlight = (menu.highlight + 1).min(count.saturating_sub(1));
                    }
                    self.render();
                    return;
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(menu) = self.context_menu.take() {
                        if let Some(item) = menu.items.get(menu.highlight) {
                            if item.id == "copy" {
                                if let Some(ref text) = menu.target_text {
                                    self.clipboard = text.clone();
                                    self.show_toast("Message copied to clipboard!", "info");
                                }
                            } else if item.id == "export_md" {
                                let _ =
                                    self.export_chat_to_file("md", "/tmp/opencode/chat_export.md");
                            } else if item.id == "export_json" {
                                let _ = self
                                    .export_chat_to_file("json", "/tmp/opencode/chat_export.json");
                            } else if item.id == "export_png" {
                                let _ = self
                                    .export_chat_to_file("png", "/tmp/opencode/chat_export.png");
                            }
                            let target_str = menu.target_id.as_deref().unwrap_or("window");
                            self.dispatcher
                                .dispatch_change(target_str, &format!("context:{}", item.id));
                            self.dispatcher.dispatch_key(
                                &format!("ContextMenu:{}:{target_str}", item.id),
                                None,
                            );
                        }
                    }
                    self.render();
                    return;
                }
                _ => {}
            }
        }

        // Modal Settings Dialog owns keystrokes when active
        if self.settings_dialog.open {
            match logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.close_settings_dialog();
                    return;
                }
                Key::Named(NamedKey::Enter) => {
                    self.close_settings_dialog();
                    self.show_toast("Settings saved!", "success");
                    return;
                }
                Key::Named(NamedKey::Tab) => {
                    self.settings_dialog.active_tab = (self.settings_dialog.active_tab + 1) % 3;
                    self.dispatcher.dispatch_key(
                        &format!("SettingsTab:{}", self.settings_dialog.active_tab),
                        None,
                    );
                    self.render();
                    return;
                }
                _ => {
                    return;
                }
            }
        }

        // Modal Command Palette owns keystrokes when active
        if self.command_palette.is_some() {
            match logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.command_palette = None;
                    self.render();
                    return;
                }
                Key::Named(NamedKey::ArrowUp) => {
                    if let Some(pal) = self.command_palette.as_mut() {
                        let count = pal.filtered_commands().len();
                        if count > 0 {
                            pal.highlight = pal.highlight.saturating_sub(1);
                        }
                    }
                    self.render();
                    return;
                }
                Key::Named(NamedKey::ArrowDown) => {
                    if let Some(pal) = self.command_palette.as_mut() {
                        let count = pal.filtered_commands().len();
                        if count > 0 {
                            pal.highlight = (pal.highlight + 1).min(count.saturating_sub(1));
                        }
                    }
                    self.render();
                    return;
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(pal) = self.command_palette.take() {
                        let filtered = pal.filtered_commands();
                        if let Some(&cmd) = filtered.get(pal.highlight) {
                            if cmd == "New Chat Window" {
                                self.request_spawn_window("Nudge Studio — Chat", 500, 400, None);
                            } else if cmd == "Open Settings" {
                                self.open_settings_dialog();
                            } else if cmd == "Export Chat as Markdown" {
                                let _ =
                                    self.export_chat_to_file("md", "/tmp/opencode/chat_export.md");
                            } else if cmd == "Export Chat as JSON" {
                                let _ = self
                                    .export_chat_to_file("json", "/tmp/opencode/chat_export.json");
                            } else if cmd == "Export Chat as PNG" {
                                let _ = self
                                    .export_chat_to_file("png", "/tmp/opencode/chat_export.png");
                            }
                            self.dispatcher
                                .dispatch_key(&format!("Command:{cmd}"), None);
                        }
                    }
                    self.render();
                    return;
                }
                Key::Named(NamedKey::Backspace) => {
                    if let Some(pal) = self.command_palette.as_mut() {
                        pal.query.pop();
                        pal.highlight = 0;
                    }
                    self.render();
                    return;
                }
                Key::Character(s) if !self.ctrl_pressed => {
                    if let Some(pal) = self.command_palette.as_mut() {
                        pal.query.push_str(s);
                        pal.highlight = 0;
                    }
                    self.render();
                    return;
                }
                _ => {}
            }
        }

        // A list that is up owns the whole keyboard before any field does. It
        // sits over the form, so a keystroke meant for the list must not reach
        // a text field hiding behind the card — which is why every branch here
        // returns, including the ones that change nothing.
        if self.open_select.is_some() {
            let (closed, moved) = match logical_key {
                Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Tab) => (true, false),
                Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                    // Enter takes whatever the card is lighting: the pointer's
                    // row when the pointer is on the card, the keyboard's row
                    // when it is not. Otherwise Enter would commit a row the
                    // user can see un-highlighted.
                    let index = self
                        .popup_effective_row()
                        .or_else(|| self.open_select_popup().map(|p| p.highlight))
                        .unwrap_or(0);
                    self.commit_select(index);
                    (true, false)
                }
                Key::Named(NamedKey::ArrowUp) => (false, self.move_select_highlight(-1)),
                Key::Named(NamedKey::ArrowDown) => (false, self.move_select_highlight(1)),
                Key::Named(NamedKey::PageUp) => {
                    let step = self.open_select_popup().map(|p| p.rows.max(1)).unwrap_or(1);
                    (false, self.move_select_highlight(-(step as isize)))
                }
                Key::Named(NamedKey::PageDown) => {
                    let step = self.open_select_popup().map(|p| p.rows.max(1)).unwrap_or(1);
                    (false, self.move_select_highlight(step as isize))
                }
                Key::Named(NamedKey::Home) => {
                    let moved = self.open_select.as_ref().is_some_and(|o| o.highlight != 0);
                    if let Some(open) = self.open_select.as_mut() {
                        open.highlight = 0;
                        open.scroll = 0;
                    }
                    (false, moved)
                }
                Key::Named(NamedKey::End) => {
                    let popup = self.open_select_popup();
                    let last = popup.as_ref().map(|p| p.options.len().saturating_sub(1));
                    let (count, visible) = match &popup {
                        Some(p) => (p.options.len(), p.rows),
                        None => (0, 0),
                    };
                    let moved = popup
                        .as_ref()
                        .is_some_and(|p| p.highlight + 1 != p.options.len());
                    if let (Some(open), Some(last)) = (self.open_select.as_mut(), last) {
                        open.highlight = last;
                        open.scroll = select_scroll_for_highlight(count, visible, last);
                    }
                    (false, moved)
                }
                _ => (false, false),
            };
            if closed {
                self.close_select();
            } else if moved {
                self.render();
            }
            return;
        }

        if let Some(ref focused_id) = self.focused_id.clone() {
            let lbox = self
                .layout_boxes
                .iter()
                .find(|b| b.id.as_deref() == Some(focused_id))
                .cloned();

            let tag = lbox.as_ref().map(|b| b.tag.as_str()).unwrap_or("");
            if tag != "input" {
                let focusable_ids: Vec<String> = self
                    .layout_boxes
                    .iter()
                    .filter(|b| {
                        !b.disabled
                            && (b.tag == "button"
                                || b.tag == "input"
                                || b.tag == "select"
                                || b.tag == "slider"
                                || matches!(b.tag.as_str(), "checkbox" | "toggle"))
                            && b.id.is_some()
                    })
                    .filter_map(|b| b.id.clone())
                    .collect();

                match logical_key {
                    Key::Named(NamedKey::Tab) => {
                        if !focusable_ids.is_empty() {
                            let curr_pos = focusable_ids.iter().position(|id| id == focused_id);
                            let next_idx = if self.shift_pressed {
                                match curr_pos {
                                    Some(pos) => {
                                        if pos == 0 {
                                            focusable_ids.len().saturating_sub(1)
                                        } else {
                                            pos - 1
                                        }
                                    }
                                    None => 0,
                                }
                            } else {
                                match curr_pos {
                                    Some(pos) => (pos + 1) % focusable_ids.len(),
                                    None => 0,
                                }
                            };
                            self.focused_id = Some(focusable_ids[next_idx].clone());
                            self.render();
                            return;
                        }
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        if self.scroll_regions.is_empty() && !focusable_ids.is_empty() {
                            let curr_pos = focusable_ids.iter().position(|id| id == focused_id);
                            let next_idx = match curr_pos {
                                Some(pos) => (pos + 1) % focusable_ids.len(),
                                None => 0,
                            };
                            self.focused_id = Some(focusable_ids[next_idx].clone());
                            self.render();
                            return;
                        }
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        if self.scroll_regions.is_empty() && !focusable_ids.is_empty() {
                            let curr_pos = focusable_ids.iter().position(|id| id == focused_id);
                            let prev_idx = match curr_pos {
                                Some(pos) => {
                                    if pos == 0 {
                                        focusable_ids.len().saturating_sub(1)
                                    } else {
                                        pos - 1
                                    }
                                }
                                None => 0,
                            };
                            self.focused_id = Some(focusable_ids[prev_idx].clone());
                            self.render();
                            return;
                        }
                    }
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        if tag == "button" {
                            if let Some(ref target_b) = lbox {
                                let click_target =
                                    target_b.id.clone().or_else(|| target_b.on_click.clone());
                                if let Some(target) = click_target {
                                    self.dispatcher.dispatch_click(&target);
                                }
                            }
                        } else if matches!(tag, "checkbox" | "toggle") {
                            if let Some(ref target_b) = lbox {
                                let next_val = !target_b.checked;
                                self.dispatcher.dispatch_change(
                                    focused_id,
                                    if next_val { "true" } else { "false" },
                                );
                            }
                        } else if tag == "select" {
                            if let Some(ref target_b) = lbox {
                                let key = select_key(target_b, 0);
                                self.open_select(&key);
                            }
                        }
                        self.render();
                        return;
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        if tag == "slider" {
                            if let Some(ref target_b) = lbox {
                                if let Some(ref sl) = target_b.slider {
                                    let new_val = (sl.value - sl.step).max(sl.min).to_string();
                                    self.dispatcher.dispatch_change(focused_id, &new_val);
                                    self.render();
                                    return;
                                }
                            }
                        }
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        if tag == "slider" {
                            if let Some(ref target_b) = lbox {
                                if let Some(ref sl) = target_b.slider {
                                    let new_val = (sl.value + sl.step).min(sl.max).to_string();
                                    self.dispatcher.dispatch_change(focused_id, &new_val);
                                    self.render();
                                    return;
                                }
                            }
                        }
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.focused_id = None;
                        self.render();
                        return;
                    }
                    _ => {}
                }
            }

            if tag == "input" {
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
                                let clip = normalize_rich_text_paste(&self.clipboard);
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
                        Key::Named(NamedKey::F1)
                        | Key::Named(NamedKey::F2)
                        | Key::Named(NamedKey::F3)
                        | Key::Named(NamedKey::F4)
                        | Key::Named(NamedKey::F5)
                        | Key::Named(NamedKey::F6)
                        | Key::Named(NamedKey::F7)
                        | Key::Named(NamedKey::F8)
                        | Key::Named(NamedKey::F9)
                        | Key::Named(NamedKey::F10)
                        | Key::Named(NamedKey::F11)
                        | Key::Named(NamedKey::F12) => {
                            let f_name = match logical_key {
                                Key::Named(NamedKey::F1) => "F1",
                                Key::Named(NamedKey::F2) => "F2",
                                Key::Named(NamedKey::F3) => "F3",
                                Key::Named(NamedKey::F4) => "F4",
                                Key::Named(NamedKey::F5) => "F5",
                                Key::Named(NamedKey::F6) => "F6",
                                Key::Named(NamedKey::F7) => "F7",
                                Key::Named(NamedKey::F8) => "F8",
                                Key::Named(NamedKey::F9) => "F9",
                                Key::Named(NamedKey::F10) => "F10",
                                Key::Named(NamedKey::F11) => "F11",
                                Key::Named(NamedKey::F12) => "F12",
                                _ => "",
                            };
                            self.dispatcher.dispatch_key(f_name, None);
                            self.render();
                            return;
                        }
                        Key::Character(s)
                            if s.eq_ignore_ascii_case("k")
                                || s.eq_ignore_ascii_case("b")
                                || s.eq_ignore_ascii_case("h")
                                || s.eq_ignore_ascii_case("t")
                                || s.eq_ignore_ascii_case("w")
                                || s.eq_ignore_ascii_case("l")
                                || s.eq_ignore_ascii_case("f")
                                || s == "/" =>
                        {
                            // Global shortcuts take precedence even when focused in input
                            if s.eq_ignore_ascii_case("k") {
                                if self.command_palette.is_some() {
                                    self.command_palette = None;
                                } else {
                                    self.command_palette = Some(CommandPaletteState::default());
                                }
                            }
                            let chord = format!("Ctrl+{s}");
                            self.dispatcher.dispatch_key(&chord, None);
                            self.render();
                            return;
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
                            let focusable_ids: Vec<String> = self
                                .layout_boxes
                                .iter()
                                .filter(|b| {
                                    !b.disabled
                                        && (b.tag == "input"
                                            || b.tag == "button"
                                            || b.tag == "select"
                                            || b.tag == "slider"
                                            || matches!(b.tag.as_str(), "checkbox" | "toggle"))
                                        && b.id.is_some()
                                })
                                .filter_map(|b| b.id.clone())
                                .collect();
                            if !focusable_ids.is_empty() {
                                let curr_pos = focusable_ids.iter().position(|id| id == focused_id);
                                let next_idx = if self.shift_pressed {
                                    match curr_pos {
                                        Some(pos) => {
                                            if pos == 0 {
                                                focusable_ids.len().saturating_sub(1)
                                            } else {
                                                pos - 1
                                            }
                                        }
                                        None => 0,
                                    }
                                } else {
                                    match curr_pos {
                                        Some(pos) => (pos + 1) % focusable_ids.len(),
                                        None => 0,
                                    }
                                };
                                self.focused_id = Some(focusable_ids[next_idx].clone());
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
                    let is_enter_or_esc = key_name == "Enter" || key_name == "Escape";
                    let chord = if self.ctrl_pressed {
                        format!("Ctrl+{key_name}")
                    } else {
                        key_name
                    };
                    if on_key.is_some() || is_enter_or_esc {
                        self.dispatcher.dispatch_key(&chord, Some(focused_id));
                    }
                }

                self.render();
                return;
            }
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
                None => {
                    let focusable_ids: Vec<String> = self
                        .layout_boxes
                        .iter()
                        .filter(|b| {
                            !b.disabled
                                && (b.tag == "button"
                                    || b.tag == "input"
                                    || b.tag == "select"
                                    || b.tag == "slider"
                                    || matches!(b.tag.as_str(), "checkbox" | "toggle"))
                                && b.id.is_some()
                        })
                        .filter_map(|b| b.id.clone())
                        .collect();
                    if let Some(last) = focusable_ids.last() {
                        self.focused_id = Some(last.clone());
                        self.render();
                        return;
                    } else {
                        self.scroll_by(0, -20);
                    }
                }
            },
            Key::Named(NamedKey::ArrowDown) => match &region_target {
                Some((key, _)) => {
                    self.scroll_region_by(key, 20);
                }
                None => {
                    let focusable_ids: Vec<String> = self
                        .layout_boxes
                        .iter()
                        .filter(|b| {
                            !b.disabled
                                && (b.tag == "button"
                                    || b.tag == "input"
                                    || b.tag == "select"
                                    || b.tag == "slider"
                                    || matches!(b.tag.as_str(), "checkbox" | "toggle"))
                                && b.id.is_some()
                        })
                        .filter_map(|b| b.id.clone())
                        .collect();
                    if let Some(first) = focusable_ids.first() {
                        self.focused_id = Some(first.clone());
                        self.render();
                        return;
                    } else {
                        self.scroll_by(0, 20);
                    }
                }
            },
            Key::Named(NamedKey::Tab) => {
                let focusable_ids: Vec<String> = self
                    .layout_boxes
                    .iter()
                    .filter(|b| {
                        !b.disabled
                            && (b.tag == "button"
                                || b.tag == "input"
                                || b.tag == "select"
                                || b.tag == "slider"
                                || matches!(b.tag.as_str(), "checkbox" | "toggle"))
                            && b.id.is_some()
                    })
                    .filter_map(|b| b.id.clone())
                    .collect();
                if !focusable_ids.is_empty() {
                    let target_id = if self.shift_pressed {
                        focusable_ids.last()
                    } else {
                        focusable_ids.first()
                    };
                    if let Some(id) = target_id {
                        self.focused_id = Some(id.clone());
                        self.render();
                        return;
                    }
                }
            }
            Key::Named(NamedKey::Enter) => key_name = "Enter".to_string(),
            Key::Named(NamedKey::Escape) => key_name = "Escape".to_string(),
            Key::Named(NamedKey::Delete) => key_name = "Delete".to_string(),
            Key::Named(NamedKey::F1) => key_name = "F1".to_string(),
            Key::Named(NamedKey::F2) => key_name = "F2".to_string(),
            Key::Named(NamedKey::F3) => key_name = "F3".to_string(),
            Key::Named(NamedKey::F4) => key_name = "F4".to_string(),
            Key::Named(NamedKey::F5) => key_name = "F5".to_string(),
            Key::Named(NamedKey::F6) => key_name = "F6".to_string(),
            Key::Named(NamedKey::F7) => key_name = "F7".to_string(),
            Key::Named(NamedKey::F8) => key_name = "F8".to_string(),
            Key::Named(NamedKey::F9) => key_name = "F9".to_string(),
            Key::Named(NamedKey::F10) => key_name = "F10".to_string(),
            Key::Named(NamedKey::F11) => key_name = "F11".to_string(),
            Key::Named(NamedKey::F12) => key_name = "F12".to_string(),
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
            if chord == "Ctrl+k" || chord == "Ctrl+K" {
                if self.command_palette.is_some() {
                    self.command_palette = None;
                } else {
                    self.command_palette = Some(CommandPaletteState::default());
                }
            } else if chord == "Ctrl+l" || chord == "Ctrl+L" {
                self.toggle_locale();
            } else if chord == "Ctrl+f" || chord == "Ctrl+F" {
                // Focus search/filter input if present (Issue #276)
                for id in &["input_filter", "filter", "search", "search_input", "find"] {
                    if self.focus_input(id) {
                        break;
                    }
                }
            }
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
            // The first frame is drawn here rather than through
            // `RedrawRequested`, so it has to arm the clock itself — otherwise
            // a field that is focused from the start would never blink.
            self.schedule_frames(Instant::now());
            self.pump_clock(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Drain any pending window spawn requests
        while let Some(cfg) = self.spawned_window_requests.pop() {
            let _ = self.spawn_secondary_window(event_loop, cfg, None);
        }

        // Handle events for secondary windows
        if let Some(sec) = self.secondary_windows.get_mut(&window_id) {
            match event {
                WindowEvent::CloseRequested => {
                    let title = sec.title.clone();
                    self.secondary_windows.remove(&window_id);
                    self.dispatcher
                        .dispatch_key(&format!("CloseWindow:{title}"), None);
                    self.dispatcher.dispatch_change("window_closed", &title);
                    return;
                }
                WindowEvent::Resized(size) => {
                    sec.width = size.width.max(1);
                    sec.height = size.height.max(1);
                    sec.render(&self.fonts, &self.theme);
                    return;
                }
                WindowEvent::RedrawRequested => {
                    sec.render(&self.fonts, &self.theme);
                    return;
                }
                _ => return,
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                self.width = size.width.max(1);
                self.height = size.height.max(1);
                self.layout_boxes.clear();
                self.scroll_regions.clear();
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
                // Every frame starts the same way: advance whatever is moving,
                // draw the state it just reached, then recompute the clock
                // from what is *still* moving. With nothing left to animate,
                // that leaves the clock idle and the loop back in
                // `ControlFlow::Wait`, so an idle window costs no CPU.
                self.advance_animation();
                self.render();
                self.schedule_frames(Instant::now());
                self.pump_clock(event_loop);
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.handle_cursor_moved((position.x as i32, position.y as i32)) {
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                }
            }
            // The pointer stepped out. Hover, the scrollbar highlight, the
            // caret and the tooltip are all derived from a position the
            // pointer is no longer at, and no further moves arrive to correct
            // them — so without this the last frame before it left stays
            // painted, with a card explaining a control nobody is pointing at.
            WindowEvent::CursorLeft { .. } => {
                if self.handle_cursor_left() {
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
                // A scrollbar under the pointer takes the press outright: the
                // tracks run down the window edge, where a control can sit, and
                // the bar is the thing the pointer was deliberately aimed at.
                if self.begin_scrollbar_drag(self.cursor_pos) {
                    self.hovered_id = None;
                    self.sync_cursor_icon();
                    self.render();
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                    return;
                }
                // A slider takes the next press, for the same reason the bar
                // does: the control absorbs it whether or not it can move, so
                // a disabled slider stops the press reaching whatever is
                // stacked behind it instead of silently falling through.
                if self.begin_slider_drag(self.cursor_pos) {
                    self.pressed_id = None;
                    self.sync_cursor_icon();
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                    return;
                }
                // Check if pressed on a draggable box
                if let Some(lbox) = self.draggable_box_at(self.cursor_pos) {
                    self.drag_state = Some(DragState {
                        source_id: lbox.id.clone().unwrap_or_default(),
                        source_tag: lbox.tag.clone(),
                        source_text: lbox.text.clone(),
                        start_pos: self.cursor_pos,
                        current_pos: self.cursor_pos,
                        active: false,
                        drop_target: None,
                    });
                }
                // Resolve the button under the cursor *before* dispatching, so
                // the pressed tone survives the press itself: dispatch may
                // already have swapped the view underneath us.
                self.pressed_id = find_hovered_button_in_boxes(
                    &self.layout_boxes,
                    self.scroll_x,
                    self.scroll_y,
                    self.cursor_pos,
                );
                self.handle_click();
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                // Ending a drag outranks a button release: nothing can be both
                // pressed and dragged, and the hand has to come back.
                if self.end_scrollbar_drag() {
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                    return;
                }
                // A slider drag ends the same way, and for the same reason:
                // the handle has been under the finger this whole time, so
                // nothing was pressed and the release belongs to the drag.
                if self.end_slider_drag() {
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                    return;
                }
                // End element drag-and-drop
                if let Some(drag) = self.drag_state.take() {
                    if drag.active {
                        if let Some(target_id) = drag.drop_target {
                            let reorder_key = format!("Reorder:{}->{}", drag.source_id, target_id);
                            self.dispatcher
                                .dispatch_change(&drag.source_id, &format!("reorder:{target_id}"));
                            self.dispatcher.dispatch_key(&reorder_key, None);
                        }
                        self.render();
                        if let Some(win) = &self.window {
                            win.request_redraw();
                        }
                        return;
                    }
                }
                // The tone is held for exactly as long as the button is down.
                // Released outside the button still clears it, so nothing can
                // get stuck looking pressed.
                if self.pressed_id.take().is_some() {
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Right,
                ..
            } => {
                let (mx, my) = self.cursor_pos;
                let target = self.layout_boxes.iter().rev().find(|b| {
                    let r = b.screen_rect(self.scroll_x, self.scroll_y);
                    let clip_ok = b
                        .screen_clip(self.scroll_x, self.scroll_y)
                        .is_none_or(|c| c.contains(mx, my));
                    r.contains(mx, my) && clip_ok && !b.disabled
                });
                self.context_menu = Some(ContextMenuState::for_target(
                    target,
                    self.cursor_pos,
                    self.width as i32,
                    self.height as i32,
                ));
                self.render();
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Other(btn),
                ..
            } if btn == 4 || btn == 5 => {
                let dy = if btn == 4 { -40 } else { 40 };
                let region_key = self.region_at(self.cursor_pos).map(|r| r.key.clone());
                match region_key {
                    Some(key) => {
                        self.scroll_region_by(&key, dy);
                    }
                    None => {
                        self.scroll_by(0, dy);
                    }
                }
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::DroppedFile(path) => {
                let p_str = path.display().to_string();
                self.last_dropped_file = Some(p_str.clone());
                self.hovered_file = None;
                self.dispatcher.dispatch_change("file_drop", &p_str);
                self.dispatcher
                    .dispatch_key(&format!("FileDrop:{p_str}"), None);
                self.render();
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::HoveredFile(path) => {
                self.hovered_file = Some(path.display().to_string());
                self.render();
                if let Some(win) = &self.window {
                    win.request_redraw();
                }
            }
            WindowEvent::HoveredFileCancelled => {
                self.hovered_file = None;
                self.render();
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
                // An open list takes the wheel for itself while the pointer is
                // over it: a choice list is its own scroll region, and a list
                // taller than its card has to be reachable. The page behind
                // the card stays where it is — the card is on top of it, so
                // scrolling the page would move a menu the user is aiming at.
                if let Some(popup) = self.open_select_popup() {
                    if popup.rect.contains(self.cursor_pos.0, self.cursor_pos.1) {
                        let step = dy / SELECT_ROW_H;
                        if step != 0 {
                            let count = popup.options.len();
                            let want = if step >= 0 {
                                popup.first.saturating_add(step as usize)
                            } else {
                                popup.first.saturating_sub(step.unsigned_abs() as usize)
                            };
                            let next = select_scroll_clamp(count, popup.rows, want);
                            if let Some(open) = self.open_select.as_mut() {
                                open.scroll = next;
                            }
                            self.render();
                        }
                        if let Some(win) = &self.window {
                            win.request_redraw();
                        }
                        return;
                    }
                    // Outside the card the wheel does nothing at all: a list
                    // is modal, and scrolling the page out from under it would
                    // leave the card stranded over different content.
                    if let Some(win) = &self.window {
                        win.request_redraw();
                    }
                    return;
                }
                // A scroll viewport under the cursor swallows the wheel so the
                // page behind it stays put until the cursor leaves the region.
                let region_key = self.region_at(self.cursor_pos).map(|r| r.key.clone());
                match region_key {
                    Some(key) => {
                        self.scroll_region_by(&key, dy);
                    }
                    None => {
                        self.scroll_by(dx, dy);
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
        // A wake-up is only ever worth something if the clock's moment has
        // arrived; otherwise this is a stray event and the loop goes straight
        // back to sleep.
        self.fire_due_frame();
        self.pump_clock(event_loop);
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
            disabled: false,
            tooltip: None,
            options: Vec::new(),
            checked: false,
            slider: None,
            draggable: false,
            props: std::collections::HashMap::new(),
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
            disabled: false,
            tooltip: None,
            options: Vec::new(),
            checked: false,
            slider: None,
            draggable: false,
            props: std::collections::HashMap::new(),
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
    fn cursor_leaving_the_window_clears_hover_and_asks_for_a_repaint() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );
        app.layout_boxes = vec![test_box("button", "go", 50, false)];
        let key_of_btn = button_key(&app.layout_boxes[0], 0);

        // On the button: lit, and the hand.
        app.handle_cursor_moved((60, 60));
        assert_eq!(app.hovered_id.as_deref(), Some("go"));
        assert_eq!(app.cursor_kind, CursorKind::Pointer);

        // Stepping out must take both with it. The platform sends no further
        // moves once the pointer is gone, so the position it left behind is
        // the only thing still saying a button is under there.
        let redraw = app.handle_cursor_left();
        assert!(
            redraw,
            "a lit control that goes dark owes a repaint — otherwise the tone stays"
        );
        assert_eq!(app.hovered_id, None, "hover must not outlive the pointer");
        assert_eq!(
            app.cursor_kind,
            CursorKind::Default,
            "the hand must not stay over a control nobody is pointing at"
        );
        assert_eq!(
            app.cursor_pos, POINTER_OUTSIDE,
            "and the position must move, or the next frame re-derives the same hover"
        );

        // Coming back and entering again still works — nothing was left
        // latched off that a move cannot restore.
        app.handle_cursor_moved((60, 60));
        assert_eq!(app.hovered_id, Some(key_of_btn));
        assert_eq!(app.cursor_kind, CursorKind::Pointer);
    }

    #[test]
    fn cursor_leaving_the_window_takes_the_tooltip_card_with_it() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );
        let mut btn = test_box("button", "go", 50, false);
        btn.tooltip = Some("Run it".into());
        app.layout_boxes = vec![btn];

        // Hovering shows the card.
        app.handle_cursor_moved((60, 60));
        assert_eq!(app.tooltip_at().as_deref(), Some("Run it"));

        // And leaving takes it. This is the case that used to stick: the
        // card is not painted from a flag but re-derived from the cursor
        // position every frame, so a position left parked on the control
        // keeps the card up forever — with nothing to explain it and no way
        // to dismiss it but to come back and leave again.
        let redraw = app.handle_cursor_left();
        assert!(redraw, "a card that was on and is now off owes a repaint");
        assert_eq!(
            app.tooltip_at(),
            None,
            "a tooltip must not outlive the pointer that summoned it"
        );

        // A second leave with nothing showing is free — no flicker on a
        // window nobody was touching.
        assert!(
            !app.handle_cursor_left(),
            "leaving again over empty space must not repaint"
        );
    }

    #[test]
    fn leaving_the_window_over_empty_space_costs_no_repaint() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );
        app.layout_boxes = vec![test_box("button", "go", 50, false)];

        // Pointer out in the middle of nowhere — nothing lit, no card, the
        // arrow already showing. Nothing on screen changes, so nothing is
        // owed: an event that fires on every window exit must not force a
        // redraw when there is nothing to redraw.
        app.handle_cursor_moved((200, 200));
        assert_eq!(app.hovered_id, None);
        assert!(
            !app.handle_cursor_left(),
            "leaving over empty space must not repaint"
        );
    }

    #[test]
    fn a_drag_keeps_holding_the_control_when_the_pointer_leaves() {
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
                title: "T".into(),
                width: 400,
                height: 300,
            },
            dummy,
        );
        let mut slider = test_box("slider", "dial", 50, false);
        slider.slider = Some(SliderSpec {
            value: 50.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
        });
        app.layout_boxes = vec![slider];

        // Press on the groove itself — the track is centred in its row, so
        // the row's top edge is above the control and would press nothing.
        let track = slider_track_rect(app.layout_boxes[0].rect);
        app.cursor_pos = (track.x + 40, track.y + track.h / 2);
        assert!(
            app.begin_slider_drag(app.cursor_pos),
            "a press on a slider's track has to start a drag"
        );
        let origin = app.slider_drag.as_ref().unwrap().current;

        // The pointer steps out mid-drag. The hand is still on the control —
        // the platform keeps delivering moves once it comes back — so the
        // gesture must survive. Ending it here would drop it the moment
        // somebody dragged fast enough to cross the edge.
        app.handle_cursor_left();
        assert!(
            app.slider_drag.is_some(),
            "a drag must not be dropped by the pointer crossing the window edge"
        );
        assert_eq!(app.slider_drag.as_ref().unwrap().current, origin);

        // And coming back resumes it rather than stranding it.
        let moved = app.handle_cursor_moved((track.x + 80, track.y + track.h / 2));
        assert!(
            moved,
            "resuming a drag that changed the value owes a repaint"
        );
        assert!(app.slider_drag.is_some());
    }

    #[test]
    fn test_spinner_animates_and_schedules_frames_only_while_visible() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );

        // Still app with no spinner: idle cost must be 0%.
        let now = Instant::now();
        app.schedule_frames(now);
        assert!(
            app.clock.is_idle(),
            "an interface without a spinner must schedule zero frames"
        );
        assert!(app.spinner_origin.is_none());

        // Add a spinner in view.
        app.layout_boxes = vec![test_box("spinner", "spin", 50, false)];
        app.schedule_frames(now);
        assert!(
            !app.clock.is_idle(),
            "a visible spinner MUST wake the clock for continuous animation"
        );
        assert!(app.spinner_origin.is_some());

        // When the spinner is removed/disappears, clock returns to idle and origin is dropped.
        app.layout_boxes.clear();
        app.schedule_frames(now);
        assert!(
            app.clock.is_idle(),
            "removing the spinner must return the frame clock to idle"
        );
        assert!(
            app.spinner_origin.is_none(),
            "dropping the spinner must drop its orphaned origin"
        );
    }

    #[test]
    fn test_progress_bar_clamping_and_layout() {
        let mut elem = UiElement::new("progress");
        elem.props.insert("value".into(), "150".into());
        elem.props.insert("min".into(), "0".into());
        elem.props.insert("max".into(), "100".into());
        elem.text = Some("Downloading".into());

        let mut boxes = Vec::new();
        let (w, h) = layout_element(&elem, 10, 10, 400, &mut boxes);
        assert_eq!(boxes.len(), 1);
        let b = &boxes[0];
        assert_eq!(b.tag, "progress");
        assert_eq!(b.text.as_deref(), Some("Downloading"));
        let spec = b.slider.expect("progress carries its numeric spec");
        assert_eq!(spec.value, 100.0, "value must be clamped to max");
        assert_eq!(spec.min, 0.0);
        assert_eq!(spec.max, 100.0);
        assert_eq!(h, PROGRESS_H);
        assert!(w >= PROGRESS_TRACK_W);
    }

    #[test]
    fn test_image_and_icon_layout_and_decode() {
        // 1. Layout element check
        let mut img_elem = UiElement::new("image");
        img_elem.props.insert("width".into(), "64".into());
        img_elem.props.insert("height".into(), "64".into());
        img_elem.text = Some("/tmp/avatar.png".into());

        let mut boxes = Vec::new();
        let (w, h) = layout_element(&img_elem, 20, 30, 400, &mut boxes);
        assert_eq!(boxes.len(), 1);
        assert_eq!(boxes[0].tag, "image");
        assert_eq!(boxes[0].rect.w, 64);
        assert_eq!(boxes[0].rect.h, 64);
        assert_eq!(boxes[0].text.as_deref(), Some("/tmp/avatar.png"));
        assert_eq!(w, 64);
        assert_eq!(h, 64);

        // 2. Decode test on valid PNG bytes (4x4 red image)
        const TINY_4X4_PNG: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 4,
            8, 6, 0, 0, 0, 169, 241, 158, 126, 0, 0, 0, 18, 73, 68, 65, 84, 120, 156, 99, 248, 207,
            192, 240, 31, 25, 51, 144, 46, 0, 0, 60, 64, 31, 225, 224, 129, 119, 180, 0, 0, 0, 0,
            73, 69, 78, 68, 174, 66, 96, 130,
        ];
        let decoded = crate::ui::decode_png(TINY_4X4_PNG).expect("decode PNG");
        assert_eq!(decoded.width, 4);
        assert_eq!(decoded.height, 4);
        assert_eq!(decoded.data.len(), 4 * 4 * 4);
        // First pixel is red (255, 0, 0, 255)
        assert_eq!(&decoded.data[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_tabs_layout_and_interaction() {
        let mut elem = UiElement::new("tabs");
        elem.text = Some("Chat".into());
        elem.props
            .insert("options".into(), "Chat\nSettings\nLogs".into());
        elem.id = Some("main_tabs".into());

        let mut boxes = Vec::new();
        let (w, h) = layout_element(&elem, 10, 10, 400, &mut boxes);
        assert_eq!(boxes.len(), 1);
        let b = &boxes[0];
        assert_eq!(b.tag, "tabs");
        assert_eq!(b.text.as_deref(), Some("Chat"));
        assert_eq!(b.options, vec!["Chat", "Settings", "Logs"]);
        assert_eq!(h, 34);
        assert!(w > 100);

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
                title: "T".into(),
                width: 400,
                height: 300,
            },
            dummy,
        );
        app.layout_boxes = boxes;

        // Click on the second tab "Settings"
        // Tab 1 "Chat" takes: 4 * CELL_W + 24
        let tab1_w = 4 * CELL_W + 24;
        let click_x = 10 + tab1_w + 10;
        app.cursor_pos = (click_x, 20);
        app.handle_click();
        assert_eq!(*app.dispatcher.notes, "Settings");
    }

    #[test]
    fn test_code_block_layout_and_tokenization() {
        let code = "fn main() {\n    let x = 42;\n    // comment\n    return x;\n}";
        let mut elem = UiElement::new("code_block");
        elem.text = Some(code.into());
        elem.id = Some("snippet".into());

        let mut boxes = Vec::new();
        let (w, h) = layout_element(&elem, 10, 10, 400, &mut boxes);
        assert_eq!(boxes.len(), 1);
        let b = &boxes[0];
        assert_eq!(b.tag, "code_block");
        assert_eq!(b.text.as_deref(), Some(code));
        assert!(h >= 5 * 20 + 24);
        assert!(w >= 160);

        // Test tokenizer on line 2: "    let x = 42;"
        let toks = tokenize_code_line("    let x = 42;");
        assert_eq!(toks[0], ("    ", SyntaxTokenKind::Whitespace));
        assert_eq!(toks[1], ("let", SyntaxTokenKind::Keyword));
        assert_eq!(toks[2], (" ", SyntaxTokenKind::Whitespace));
        assert_eq!(toks[3], ("x", SyntaxTokenKind::Ident));
        assert_eq!(toks[5], ("=", SyntaxTokenKind::Punctuation));
        assert_eq!(toks[7], ("42", SyntaxTokenKind::Number));
        assert_eq!(toks[8], (";", SyntaxTokenKind::Punctuation));

        // Test tokenizer on comment
        let toks_comment = tokenize_code_line("    // comment");
        assert_eq!(toks_comment[1], ("// comment", SyntaxTokenKind::Comment));
    }

    #[test]
    fn test_divider_and_spacer_layout() {
        let div_elem = UiElement::new("divider");
        let mut div_boxes = Vec::new();
        let (dw, dh) = layout_element(&div_elem, 10, 10, 300, &mut div_boxes);
        assert_eq!(div_boxes.len(), 1);
        assert_eq!(div_boxes[0].tag, "divider");
        assert_eq!(dw, 300);
        assert_eq!(dh, 16);

        let mut sp_elem = UiElement::new("spacer");
        sp_elem.props.insert("size".into(), "32".into());
        let mut sp_boxes = Vec::new();
        let (sw, sh) = layout_element(&sp_elem, 10, 10, 300, &mut sp_boxes);
        assert_eq!(sp_boxes.len(), 1);
        assert_eq!(sp_boxes[0].tag, "spacer");
        assert_eq!(sw, 32);
        assert_eq!(sh, 32);
    }

    #[test]
    fn test_badge_and_tag_layout() {
        let mut b_elem = UiElement::new("badge");
        b_elem.text = Some("YOU".into());
        let mut b_boxes = Vec::new();
        let (bw, bh) = layout_element(&b_elem, 10, 10, 300, &mut b_boxes);
        assert_eq!(b_boxes.len(), 1);
        assert_eq!(b_boxes[0].tag, "badge");
        assert_eq!(b_boxes[0].text.as_deref(), Some("YOU"));
        assert_eq!(bh, 22);
        assert!(bw >= 32);
    }

    #[test]
    fn test_status_line_layout() {
        let mut elem = UiElement::new("status_line");
        elem.text = Some("System ready".into());
        elem.props.insert("status".into(), "success".into());
        let mut boxes = Vec::new();
        let (w, h) = layout_element(&elem, 10, 10, 300, &mut boxes);
        assert_eq!(boxes.len(), 1);
        assert_eq!(boxes[0].tag, "status_line");
        assert_eq!(boxes[0].text.as_deref(), Some("System ready"));
        assert_eq!(boxes[0].options, vec!["success"]);
        assert_eq!(h, 28);
        assert!(w >= 120);
    }

    #[test]
    fn test_row_and_column_smart_alignment() {
        // Test row with justify="between" and expand="true"
        let mut row_elem = UiElement::new("row");
        row_elem.props.insert("justify".into(), "between".into());
        row_elem.props.insert("padding".into(), "0".into());
        row_elem.props.insert("gap".into(), "0".into());

        let mut b1 = UiElement::new("button");
        b1.text = Some("A".into());
        let mut b2 = UiElement::new("button");
        b2.text = Some("B".into());
        row_elem.children = vec![b1, b2];

        let mut boxes = Vec::new();
        let (rw, _rh) = layout_element(&row_elem, 0, 0, 400, &mut boxes);
        assert_eq!(rw, 400);
        assert_eq!(boxes.len(), 2);
        assert_eq!(boxes[0].rect.x, 0);
        // Second button should be shifted all the way to the right
        assert!(boxes[1].rect.x > 300);

        // Test column with align="end"
        let mut col_elem = UiElement::new("column");
        col_elem.props.insert("align".into(), "end".into());
        col_elem.props.insert("expand".into(), "true".into());
        col_elem.props.insert("padding".into(), "0".into());

        let mut btn = UiElement::new("button");
        btn.text = Some("OK".into());
        col_elem.children = vec![btn];

        let mut col_boxes = Vec::new();
        let (cw, _ch) = layout_element(&col_elem, 0, 0, 300, &mut col_boxes);
        assert_eq!(cw, 300);
        assert_eq!(col_boxes.len(), 1);
        assert!(col_boxes[0].rect.x > 200);
    }

    #[test]
    fn test_global_keyboard_shortcuts() {
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
                title: "Shortcuts".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );

        // 1. Global shortcut when unfocused
        app.ctrl_pressed = true;
        app.handle_key_input(&Key::Character("k".into()), Some("k"));
        assert_eq!(*app.dispatcher.last_key, "Ctrl+k");

        app.handle_key_input(&Key::Character("b".into()), Some("b"));
        assert_eq!(*app.dispatcher.last_key, "Ctrl+b");

        app.handle_key_input(&Key::Character("/".into()), Some("/"));
        assert_eq!(*app.dispatcher.last_key, "Ctrl+/");

        // Ctrl+F focusing search input (Issue #276)
        let mut filter_box = test_box("input", "input_filter", 50, false);
        filter_box.text = Some("".into());
        app.layout_boxes = vec![filter_box];
        app.handle_key_input(&Key::Character("f".into()), Some("f"));
        assert_eq!(app.focused_id, Some("input_filter".into()));

        // 2. Global shortcut when focused inside input takes precedence
        let mut inp = test_box("input", "prompt_box", 50, false);
        inp.text = Some("hello".into());
        app.layout_boxes = vec![inp];
        app.focused_id = Some("prompt_box".into());

        app.ctrl_pressed = true;
        app.handle_key_input(&Key::Character("k".into()), Some("k"));
        assert_eq!(*app.dispatcher.last_key, "Ctrl+k");
    }

    #[test]
    fn test_command_palette_interaction_and_filtering() {
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
                title: "Palette".into(),
                width: 400,
                height: 400,
            },
            dummy,
        );

        // 1. Open with Ctrl+K
        app.ctrl_pressed = true;
        app.handle_key_input(&Key::Character("k".into()), Some("k"));
        assert!(app.command_palette.is_some());
        app.ctrl_pressed = false;

        // 2. Type query "sett"
        app.handle_key_input(&Key::Character("s".into()), Some("s"));
        app.handle_key_input(&Key::Character("e".into()), Some("e"));
        app.handle_key_input(&Key::Character("t".into()), Some("t"));
        app.handle_key_input(&Key::Character("t".into()), Some("t"));

        let filtered = app.command_palette.as_ref().unwrap().filtered_commands();
        assert_eq!(filtered, vec!["Open Settings"]);

        // 3. Press Enter to execute command
        app.handle_key_input(&Key::Named(NamedKey::Enter), None);
        assert_eq!(*app.dispatcher.last_key, "Command:Open Settings");
        assert!(
            app.command_palette.is_none(),
            "Enter commits and closes palette"
        );
    }

    #[test]
    fn test_keyboard_list_navigation_up_down_enter() {
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
                title: "List Nav".into(),
                width: 400,
                height: 400,
            },
            dummy,
        );

        let chat1 = test_box("button", "chat_item_1", 40, false);
        let chat2 = test_box("button", "chat_item_2", 90, false);
        let chat3 = test_box("button", "chat_item_3", 140, false);
        app.layout_boxes = vec![chat1, chat2, chat3];

        // 1. Initial ArrowDown focuses the first list item
        assert!(app.focused_id.is_none());
        app.handle_key_input(&Key::Named(NamedKey::ArrowDown), None);
        assert_eq!(app.focused_id.as_deref(), Some("chat_item_1"));

        // 2. Next ArrowDown moves focus to second item
        app.handle_key_input(&Key::Named(NamedKey::ArrowDown), None);
        assert_eq!(app.focused_id.as_deref(), Some("chat_item_2"));

        // 3. ArrowUp moves back to first item
        app.handle_key_input(&Key::Named(NamedKey::ArrowUp), None);
        assert_eq!(app.focused_id.as_deref(), Some("chat_item_1"));

        // 4. ArrowDown to second item and press Enter to activate
        app.handle_key_input(&Key::Named(NamedKey::ArrowDown), None);
        assert_eq!(app.focused_id.as_deref(), Some("chat_item_2"));

        app.handle_key_input(&Key::Named(NamedKey::Enter), None);
        assert_eq!(
            *app.dispatcher.val, 1,
            "Enter on focused button dispatches click"
        );
    }

    #[test]
    fn test_drag_and_drop_reorder_and_file_drop() {
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
                title: "DragDrop".into(),
                width: 400,
                height: 400,
            },
            dummy,
        );

        let mut msg1 = test_box("bubble:user", "msg_1", 40, false);
        msg1.draggable = true;
        let mut msg2 = test_box("bubble:assistant", "msg_2", 100, false);
        msg2.draggable = true;
        app.layout_boxes = vec![msg1, msg2];

        // 1. Interactive drag from msg_1 to msg_2
        app.cursor_pos = (60, 50);
        let lbox = app.draggable_box_at(app.cursor_pos).unwrap();
        app.drag_state = Some(DragState {
            source_id: lbox.id.clone().unwrap(),
            source_tag: lbox.tag.clone(),
            source_text: lbox.text.clone(),
            start_pos: app.cursor_pos,
            current_pos: app.cursor_pos,
            active: false,
            drop_target: None,
        });

        // Move mouse to msg_2 at (60, 110)
        app.handle_cursor_moved((60, 110));
        assert!(app.drag_state.as_ref().unwrap().active);
        assert_eq!(
            app.drag_state.as_ref().unwrap().drop_target.as_deref(),
            Some("msg_2")
        );

        // 2. Simulate reorder helper
        app.simulate_drag_and_drop("msg_1", "msg_2");
        assert_eq!(*app.dispatcher.notes, "reorder:msg_2");
        assert_eq!(*app.dispatcher.last_key, "Reorder:msg_1->msg_2");

        // 3. Simulate file drop
        app.simulate_file_drop("/home/user/document.pdf");
        assert_eq!(*app.dispatcher.notes, "/home/user/document.pdf");
        assert_eq!(*app.dispatcher.last_key, "FileDrop:/home/user/document.pdf");
        assert_eq!(
            app.last_dropped_file.as_deref(),
            Some("/home/user/document.pdf")
        );
    }

    #[test]
    fn test_context_menu_interaction_and_actions() {
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
                title: "ContextMenu".into(),
                width: 400,
                height: 400,
            },
            dummy,
        );

        let msg = test_box("bubble:user", "user_prompt", 40, false);
        app.layout_boxes = vec![msg];

        // 1. Right click on message to open context menu
        app.cursor_pos = (60, 50);
        let target = app.layout_boxes.first();
        app.context_menu = Some(ContextMenuState::for_target(
            target,
            app.cursor_pos,
            400,
            400,
        ));
        assert!(app.context_menu.is_some());
        let menu = app.context_menu.as_ref().unwrap();
        assert_eq!(menu.items.len(), 3);
        assert_eq!(menu.items[0].label, "Copy Message");
        assert_eq!(menu.items[1].label, "Regenerate");

        // 2. Keyboard navigate down to "Regenerate" and press Enter
        app.handle_key_input(&Key::Named(NamedKey::ArrowDown), None);
        assert_eq!(app.context_menu.as_ref().unwrap().highlight, 1);

        app.handle_key_input(&Key::Named(NamedKey::Enter), None);
        assert_eq!(*app.dispatcher.notes, "context:regenerate");
        assert_eq!(
            *app.dispatcher.last_key,
            "ContextMenu:regenerate:user_prompt"
        );
        assert!(
            app.context_menu.is_none(),
            "Enter executes and closes context menu"
        );

        // 3. Programmatic context action trigger helper
        app.trigger_context_menu_action("delete", "user_prompt");
        assert_eq!(*app.dispatcher.notes, "context:delete");
        assert_eq!(*app.dispatcher.last_key, "ContextMenu:delete:user_prompt");
    }

    #[test]
    fn test_rich_text_paste_and_copy_code_toast() {
        // 1. Rich text paste normalization
        let raw_html = "<b>Bold text</b> and <code>let x = 1;</code><br><p>Second paragraph</p>";
        let normalized = normalize_rich_text_paste(raw_html);
        assert_eq!(
            normalized,
            "**Bold text** and `let x = 1;`\nSecond paragraph\n"
        );

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
                title: "CopyPaste".into(),
                width: 400,
                height: 400,
            },
            dummy,
        );

        let mut inp = test_box("input", "composer", 30, false);
        inp.text = Some("".into());
        let mut code = test_box("code_block", "snippet_1", 80, false);
        code.text = Some("fn main() {\n    println!(\"hi\");\n}".into());
        app.layout_boxes = vec![inp, code];

        // 2. Focused input accepts normalized rich text paste
        app.focused_id = Some("composer".into());
        app.paste_text("<strong>Nudge Studio</strong> <i>fast</i>");
        assert_eq!(
            app.text_inputs.get("composer").unwrap().text,
            "**Nudge Studio** *fast*"
        );
        assert_eq!(*app.dispatcher.notes, "**Nudge Studio** *fast*");

        // 3. Copy code block triggers clipboard, toast, and dispatch event
        app.simulate_copy_code("snippet_1");
        assert_eq!(app.clipboard, "fn main() {\n    println!(\"hi\");\n}");
        assert_eq!(
            app.toast.as_ref().unwrap().message,
            "Code copied to clipboard!"
        );
        assert_eq!(app.toast.as_ref().unwrap().status, "success");
        assert_eq!(*app.dispatcher.last_key, "CopyCode:snippet_1");
        assert_eq!(*app.dispatcher.notes, "copied");
    }

    #[test]
    fn test_chat_export_markdown_json_and_png() {
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
                title: "ChatSession".into(),
                width: 400,
                height: 400,
            },
            dummy,
        );

        let mut b1 = test_box("bubble:user", "u1", 40, false);
        b1.text = Some("How does Nudge native rendering work?".into());
        let mut b2 = test_box("bubble:assistant", "a1", 100, false);
        b2.text = Some("It draws directly to Linux framebuffers using softbuffer.".into());
        let mut code = test_box("code_block", "c1", 160, false);
        code.text = Some("let buf = vec![0u32; 100];".into());
        app.layout_boxes = vec![b1, b2, code];

        // 1. Markdown export
        let md = app.export_chat_markdown();
        assert!(md.contains("# ChatSession"));
        assert!(md.contains("### User\n\nHow does Nudge native rendering work?"));
        assert!(md.contains(
            "### Assistant\n\nIt draws directly to Linux framebuffers using softbuffer."
        ));
        assert!(md.contains("```\nlet buf = vec![0u32; 100];\n```"));

        // 2. JSON export
        let js = app.export_chat_json().unwrap();
        assert!(js.contains("\"title\": \"ChatSession\""));
        assert!(js.contains("\"role\": \"user\""));
        assert!(js.contains("\"role\": \"assistant\""));

        // 3. Export to files with toast feedback
        let out_dir = std::env::temp_dir().join("nudge_test_export");
        let _ = std::fs::create_dir_all(&out_dir);
        let out_md_path = out_dir.join("test_export.md");
        let out_json_path = out_dir.join("test_export.json");
        let out_png_path = out_dir.join("test_export.png");
        let out_md = out_md_path.to_str().unwrap();
        let out_json = out_json_path.to_str().unwrap();
        let out_png = out_png_path.to_str().unwrap();

        let _ = app.export_chat_to_file("md", out_md).unwrap();
        assert_eq!(*app.dispatcher.last_key, format!("ChatExport:md:{out_md}"));
        assert_eq!(*app.dispatcher.notes, out_md);
        assert_eq!(app.toast.as_ref().unwrap().status, "success");

        let _ = app.export_chat_to_file("json", out_json).unwrap();
        assert_eq!(
            *app.dispatcher.last_key,
            format!("ChatExport:json:{out_json}")
        );

        let _ = app.export_chat_to_file("png", out_png).unwrap();
        assert_eq!(
            *app.dispatcher.last_key,
            format!("ChatExport:png:{out_png}")
        );
        assert!(std::path::Path::new(out_png).exists());
    }

    #[test]
    fn test_settings_dialog_modal_interaction_and_shortcuts() {
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
                title: "SettingsDemo".into(),
                width: 500,
                height: 400,
            },
            dummy,
        );

        // 1. Initially dialog is closed
        assert!(!app.settings_dialog.open);

        // 2. Open settings dialog
        app.open_settings_dialog();
        assert!(app.settings_dialog.open);
        assert_eq!(*app.dispatcher.last_key, "OpenSettings");

        // 3. Tab cycling with Tab key
        app.handle_key_input(&Key::Named(NamedKey::Tab), None);
        assert_eq!(app.settings_dialog.active_tab, 1);
        assert_eq!(*app.dispatcher.last_key, "SettingsTab:1");

        app.handle_key_input(&Key::Named(NamedKey::Tab), None);
        assert_eq!(app.settings_dialog.active_tab, 2);

        app.handle_key_input(&Key::Named(NamedKey::Tab), None);
        assert_eq!(app.settings_dialog.active_tab, 0);

        // 4. Click to toggle dark theme in Tab 0
        // Window 500x400 -> Card x: (500-420)/2 = 40, y: (400-300)/2 = 50
        // tab_y = 50 + 44 = 94, content_y = 94 + 36 = 130
        // row1_y = 130 + 8 = 138 (Dark Theme row)
        app.cursor_pos = (150, 142);
        app.handle_click();
        assert!(!app.settings_dialog.dark_theme);
        assert_eq!(*app.dispatcher.notes, "false");
        assert_eq!(*app.dispatcher.last_key, "SettingsChange:dark_theme:false");

        // 5. Enter commits and closes dialog
        app.handle_key_input(&Key::Named(NamedKey::Enter), None);
        assert!(!app.settings_dialog.open);
        assert_eq!(app.toast.as_ref().unwrap().message, "Settings saved!");
    }

    #[test]
    fn test_multi_window_spawning_and_routing() {
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
                title: "PrimaryChat".into(),
                width: 500,
                height: 400,
            },
            dummy,
        );

        let chat_btn = test_box("button", "chat_sess_2", 40, false);
        app.layout_boxes = vec![chat_btn];

        // 1. Context menu on chat element provides "popout" (Open in New Window)
        app.cursor_pos = (60, 50);
        let target = app.layout_boxes.first();
        let menu = ContextMenuState::for_target(target, app.cursor_pos, 500, 400);
        assert!(menu.items.iter().any(|i| i.id == "popout"));

        // 2. Request secondary window spawn
        app.request_spawn_window("SecondaryChat", 450, 350, Some("chat_sess_2".into()));
        assert_eq!(app.spawned_window_requests.len(), 1);
        assert_eq!(app.spawned_window_requests[0].title, "SecondaryChat");
        assert_eq!(app.spawned_window_requests[0].width, 450);
        assert_eq!(*app.dispatcher.last_key, "SpawnWindow:SecondaryChat");
        assert_eq!(*app.dispatcher.notes, "SecondaryChat");
        assert_eq!(
            app.toast.as_ref().unwrap().message,
            "Opened \"SecondaryChat\" in new window"
        );
    }

    #[test]
    fn test_localization_tr_en_and_shortcuts() {
        // 1. Zero-allocation fast translation lookup across supported languages
        assert_eq!(Locale::Tr.get("New Chat"), "Yeni Sohbet");
        assert_eq!(Locale::Zh.get("New Chat"), "新建对话");
        assert_eq!(Locale::Ja.get("New Chat"), "新しいチャット");
        assert_eq!(Locale::Ko.get("New Chat"), "새 대화");
        assert_eq!(Locale::Ru.get("New Chat"), "Новый чат");
        assert_eq!(Locale::Es.get("New Chat"), "Nuevo chat");
        assert_eq!(Locale::De.get("New Chat"), "Neuer Chat");
        assert_eq!(Locale::Fr.get("New Chat"), "Nouvelle discussion");
        assert_eq!(Locale::It.get("New Chat"), "Nuova chat");
        assert_eq!(Locale::Pt.get("New Chat"), "Nova conversa");
        assert_eq!(Locale::Nl.get("New Chat"), "Nieuwe chat");
        assert_eq!(Locale::En.get("New Chat"), "New Chat");

        assert_eq!(Locale::from_code("zh"), Some(Locale::Zh));
        assert_eq!(Locale::from_code("ja"), Some(Locale::Ja));
        assert_eq!(Locale::from_code("ko"), Some(Locale::Ko));
        assert_eq!(Locale::from_code("de"), Some(Locale::De));
        assert_eq!(Locale::from_code("fr"), Some(Locale::Fr));
        assert_eq!(Locale::from_code("es"), Some(Locale::Es));
        assert_eq!(Locale::from_code("it"), Some(Locale::It));
        assert_eq!(Locale::from_code("pt"), Some(Locale::Pt));
        assert_eq!(Locale::from_code("ru"), Some(Locale::Ru));
        assert_eq!(Locale::from_code("nl"), Some(Locale::Nl));

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
                title: "LocaleDemo".into(),
                width: 500,
                height: 400,
            },
            dummy,
        );

        // 2. Default locale is English
        assert_eq!(app.locale, Locale::En);

        // 3. Switch to Turkish
        app.set_locale(Locale::Tr);
        assert_eq!(app.locale, Locale::Tr);
        assert_eq!(*app.dispatcher.notes, "tr");
        assert_eq!(*app.dispatcher.last_key, "LocaleChange:Tr");
        assert_eq!(app.t("New Chat"), "Yeni Sohbet");

        // 4. Toggle cycles to Chinese (Zh)
        app.toggle_locale();
        assert_eq!(app.locale, Locale::Zh);
        assert_eq!(*app.dispatcher.notes, "zh");
        assert_eq!(*app.dispatcher.last_key, "LocaleChange:Zh");
        assert_eq!(app.t("New Chat"), "新建对话");

        // 5. Ctrl+L keyboard shortcut cycles locale (Zh -> Ja)
        app.ctrl_pressed = true;
        app.handle_key_input(&Key::Character("l".into()), Some("l"));
        assert_eq!(app.locale, Locale::Ja);
        assert_eq!(app.t("New Chat"), "新しいチャット");
    }

    #[test]
    fn test_parse_rich_spans() {
        let input = "Hello **world**, check `cargo test` and [docs](https://nudge.dev) today!";
        let spans = parse_rich_spans(input);
        assert_eq!(spans[0], ("Hello ", RichSpanKind::Plain));
        assert_eq!(spans[1], ("world", RichSpanKind::Bold));
        assert_eq!(spans[2], (", check ", RichSpanKind::Plain));
        assert_eq!(spans[3], ("cargo test", RichSpanKind::Code));
        assert_eq!(spans[4], (" and ", RichSpanKind::Plain));
        assert_eq!(spans[5], ("docs", RichSpanKind::Link));
        assert_eq!(spans[6], (" today!", RichSpanKind::Plain));
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
            disabled: false,
            tooltip: None,
            options: Vec::new(),
            checked: false,
            slider: None,
            draggable: false,
            props: std::collections::HashMap::new(),
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
            disabled: false,
            tooltip: None,
            options: Vec::new(),
            checked: false,
            slider: None,
            draggable: false,
            props: std::collections::HashMap::new(),
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

    /// The halo darkens immediately outside the shape, ramps off smoothly,
    /// and is *exactly* background once `blur` pixels away — no hard ring, no
    /// residue past the declared reach.
    #[test]
    fn test_shadow_fades_out_over_its_blur_and_stops() {
        const BG: u32 = 0xFF18_181B; // Theme::DARK.bg
        const SHADOW: u32 = 0xFF00_0000;
        let stride = 120usize;
        let height = 80usize;
        let mut buf = vec![BG; stride * height];

        // Shape at x 20..80, nudged down by oy=2, so its right edge sits x=80.
        let (blur, alpha) = (6, 0.5);
        draw_rounded_rect_shadow_clipped(
            &mut buf, stride, height, None, 20, 20, 60, 30, 10, SHADOW, alpha, blur, 0, 2,
        );

        let at = |x: usize| buf[35 * stride + x] & 0xFF;
        let bg_l = BG & 0xFF;

        // Right at the edge the halo is near full strength: markedly darker.
        assert!(
            at(80) < bg_l - 5,
            "just outside the edge must be visibly darkened, got {} vs bg {bg_l}",
            at(80)
        );

        // Past blur there must be nothing at all — the declared reach is real.
        for x in (80 + blur + 1)..80 + blur + 6 {
            assert_eq!(
                at(x as usize),
                bg_l,
                "x={x} is past blur={blur} and must be untouched background"
            );
        }

        // Monotonic ramp: darkening weakens steadily with distance. Start
        // from the darkest sample (right at the edge) and require it to rise.
        let mut prev = at(80);
        for x in 81..80 + blur {
            let v = at(x as usize);
            assert!(
                v >= prev,
                "halo must fade out monotonically: x={x} got {v}, previous {prev}"
            );
            prev = v;
        }
        assert!(
            prev >= bg_l - 1,
            "halo must reach background exactly at blur, got {prev} vs {bg_l}"
        );
    }

    /// A halo inside a scroll region is sliced at the viewport edge, exactly
    /// like every other piece of chrome — it must not leak past it.
    #[test]
    fn test_shadow_is_clipped_to_the_viewport() {
        const BG: u32 = 0xFF18_181B;
        const SHADOW: u32 = 0xFF00_0000;
        let stride = 80usize;
        let height = 60usize;
        let mut buf = vec![BG; stride * height];

        let clip = Rect {
            x: 0,
            y: 0,
            w: 40,
            h: height as i32,
        };
        // Shape spans x 20..70, so its right half (and that halo) is outside.
        draw_rounded_rect_shadow_clipped(
            &mut buf,
            stride,
            height,
            Some(clip),
            20,
            20,
            50,
            24,
            8,
            SHADOW,
            0.5,
            6,
            0,
            2,
        );

        assert_ne!(
            buf[32 * stride + 39],
            BG,
            "the part inside the viewport must carry halo"
        );
        for x in 40..80 {
            assert_eq!(
                buf[32 * stride + x],
                BG,
                "x={x} is outside the viewport and must stay background"
            );
        }
    }

    /// Why shadows are their own pass: with two bubbles almost touching, a
    /// halo drawn inline would land on the *earlier* bubble's fill and leave a
    /// grey smear. After the pass, the earlier box's interior must still be
    /// pure fill.
    #[test]
    fn test_shadow_pass_never_darkens_an_earlier_box() {
        const BG: u32 = 0xFF18_181B;
        const FILL: u32 = 0xFF3B_82F6; // bubble_user_bg
        let stride = 120usize;
        let height = 160usize;
        let mut buf = vec![BG; stride * height];

        let boxes = vec![
            LayoutBox {
                rect: Rect {
                    x: 20,
                    y: 20,
                    w: 60,
                    h: 40,
                },
                tag: "bubble:user".into(),
                id: None,
                text: None,
                on_click: None,
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 10,
                disabled: false,
                tooltip: None,
                options: Vec::new(),
                checked: false,
                slider: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            },
            // Only 2px below the first: its halo *would* reach back over the
            // first box's bottom edge if drawn inline.
            LayoutBox {
                rect: Rect {
                    x: 20,
                    y: 62,
                    w: 60,
                    h: 40,
                },
                tag: "bubble:user".into(),
                id: None,
                text: None,
                on_click: None,
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 10,
                disabled: false,
                tooltip: None,
                options: Vec::new(),
                checked: false,
                slider: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            },
        ];

        draw_shadows(&boxes, 0, 0, &mut buf, stride, height, &Theme::DARK);
        // Now the boxes paint over their own halos, exactly as render does.
        for b in &boxes {
            draw_rounded_rect_box(
                &mut buf,
                stride,
                height,
                b.rect.x,
                b.rect.y,
                b.rect.w,
                b.rect.h,
                10,
                FILL,
                Theme::DARK.border,
            );
        }

        // Deep inside box 1: pure fill, no darkening from box 2's halo.
        assert_eq!(
            buf[38 * stride + 50],
            FILL,
            "box 1's fill must not be tinted by a later box's halo"
        );
        // Its very last row is the 1px border — also must be exactly the
        // border colour, since that is where an inline halo would land.
        assert_eq!(
            buf[59 * stride + 50],
            Theme::DARK.border,
            "box 1's bottom border must stay exactly the border colour"
        );
        // And box 2 itself is intact.
        assert_eq!(buf[80 * stride + 50], FILL, "box 2's fill must be intact");
        // Between them the halo *is* allowed on the bare background.
        assert_ne!(
            buf[61 * stride + 50],
            BG,
            "the gap between bubbles should show the halo on background"
        );
    }

    /// Shadows are opt-in per tag: only the card-like bubbles float, and their
    /// blur is smaller than a feed's default 8px gap so halos never stack into
    /// a dark seam down the column.
    #[test]
    fn test_only_bubbles_carry_a_shadow_and_it_stays_inside_the_gap() {
        for tag in ["button", "text", "input", "bubble", "scroll"] {
            assert_eq!(
                shadow_for(tag),
                None,
                "flat chrome '{tag}' must not cast a halo"
            );
        }
        for tag in ["bubble:user", "bubble:assistant"] {
            let (blur, alpha, _ox, oy) = shadow_for(tag)
                .unwrap_or_else(|| panic!("'{tag}' is card-like and must cast a halo"));
            assert!(blur < 8, "blur {blur} must stay under the 8px gap");
            assert!(alpha > 0.0 && alpha <= 1.0, "alpha out of range");
            assert!(
                blur + oy <= 8,
                "reach below the box ({blur}+{oy}) must not cross an 8px gap"
            );
        }
    }

    /// A vertical ramp must start and end on the *exact* end colours (no
    /// rounding drift) and move in one direction only — a reversal would read
    /// as a band across the surface.
    #[test]
    fn test_gradient_vertical_lands_on_both_end_colours_and_ramps_once() {
        const BG: u32 = 0xFF18_181B;
        const TOP: u32 = 0xFFFF_0000; // pure red
        const BOTTOM: u32 = 0xFF00_00FF; // pure blue
        let stride = 40usize;
        let height = 30usize;
        let mut buf = vec![BG; stride * height];
        let (x, y, w, h) = (5i32, 3i32, 20i32, 20i32);

        draw_rect_gradient_clipped(
            &mut buf, stride, height, None, x, y, w, h, TOP, BOTTOM, false,
        );

        let at = |col: usize, row: usize| buf[row * stride + col];
        let cx = x as usize + 5;
        assert_eq!(
            at(cx, y as usize),
            TOP,
            "the first row must be exactly `top`"
        );
        assert_eq!(
            at(cx, (y + h - 1) as usize),
            BOTTOM,
            "the last row must be exactly `bottom`"
        );

        let mut prev_r = 256u32;
        let mut prev_b = 0u32;
        for row in (y as usize)..((y + h) as usize) {
            let c = at(cx, row);
            let (r, b) = ((c >> 16) & 0xFF, c & 0xFF);
            assert!(r <= prev_r, "red must never rise going down (row {row})");
            assert!(b >= prev_b, "blue must never fall going down (row {row})");
            prev_r = r;
            prev_b = b;
        }

        // Nothing outside the rect may be touched.
        assert_eq!(at(0, 0), BG, "pixels left of the rect stay background");
        assert_eq!(at(cx, 0), BG, "pixels above the rect stay background");
    }

    /// The horizontal orientation (required for headers and side rails) ramps
    /// across columns with the same exact endpoints.
    #[test]
    fn test_gradient_horizontal_ramps_left_to_right() {
        const BG: u32 = 0xFF18_181B;
        const LEFT: u32 = 0xFFFF_0000;
        const RIGHT: u32 = 0xFF00_00FF;
        let stride = 40usize;
        let height = 30usize;
        let mut buf = vec![BG; stride * height];
        let (x, y, w, h) = (5i32, 3i32, 20i32, 20i32);

        draw_rect_gradient_clipped(
            &mut buf, stride, height, None, x, y, w, h, LEFT, RIGHT, true,
        );

        let at = |col: usize, row: usize| buf[row * stride + col];
        let my = y as usize + 5;
        assert_eq!(
            at(x as usize, my),
            LEFT,
            "the first column must be exactly the left end colour"
        );
        assert_eq!(
            at((x + w - 1) as usize, my),
            RIGHT,
            "the last column must be exactly the right end colour"
        );

        let mut prev_b = 0u32;
        for col in (x as usize)..((x + w) as usize) {
            let b = at(col, my) & 0xFF;
            assert!(b >= prev_b, "blue must never fall going right (col {col})");
            prev_b = b;
        }
        // Rows above/below the rect are untouched — this is horizontal, so the
        // ramp must not leak vertically.
        assert_eq!(at(x as usize + 1, 0), BG);
    }

    /// A gradient inside a scroll region is sliced at the viewport edge like
    /// every other piece of chrome.
    #[test]
    fn test_gradient_respects_the_viewport_clip() {
        const BG: u32 = 0xFF18_181B;
        const TOP: u32 = 0xFFFF_0000;
        const BOTTOM: u32 = 0xFF00_00FF;
        let stride = 40usize;
        let height = 30usize;
        let mut buf = vec![BG; stride * height];
        let clip = Rect {
            x: 0,
            y: 0,
            w: 15,
            h: height as i32,
        };

        // Rect spans x 5..25, so everything from x=15 on is outside the clip.
        draw_rect_gradient_clipped(
            &mut buf,
            stride,
            height,
            Some(clip),
            5,
            3,
            20,
            20,
            TOP,
            BOTTOM,
            false,
        );

        assert_ne!(buf[10 * stride + 14], BG, "inside the clip is painted");
        for x in 15..25 {
            assert_eq!(
                buf[10 * stride + x],
                BG,
                "x={x} is past the viewport and must stay background"
            );
        }
    }

    /// The rounded variant keeps its ring a constant border colour while the
    /// interior ramps, and the ramp's endpoints land on *visible* interior
    /// rows (the border would otherwise hide them).
    #[test]
    fn test_rounded_gradient_box_rings_it_flat_and_ramps_the_interior() {
        const BG: u32 = 0xFF18_181B;
        const TOP: u32 = 0xFFFF_0000;
        const BOTTOM: u32 = 0xFF00_00FF;
        const BORDER: u32 = 0xFF00_FF00; // green, so a leak is obvious
        let stride = 40usize;
        let height = 30usize;
        let mut buf = vec![BG; stride * height];
        let (x, y, w, h, r) = (5i32, 3i32, 30i32, 20i32, 6i32);

        draw_rounded_rect_gradient_box_clipped(
            &mut buf, stride, height, None, x, y, w, h, r, TOP, BOTTOM, BORDER, false,
        );

        let at = |col: i32, row: i32| buf[row as usize * stride + col as usize];
        let cx = x + w / 2;

        assert_eq!(
            at(cx, y),
            BORDER,
            "the top ring must be the flat border colour, not a ramp sample"
        );
        assert_eq!(
            at(cx, y + 1),
            TOP,
            "the topmost filled row must be exactly `top`"
        );
        assert_eq!(
            at(cx, y + h - 2),
            BOTTOM,
            "the bottommost filled row must be exactly `bottom`"
        );

        // The interior must actually vary — a single flat tone means the ramp
        // never got applied.
        let mut distinct = std::collections::HashSet::new();
        let mut prev_r = 256u32;
        for row in (y + 1)..=(y + h - 2) {
            let c = at(cx, row);
            distinct.insert(c);
            let rr = (c >> 16) & 0xFF;
            assert!(
                rr <= prev_r,
                "interior must ramp monotonically (row {row} rose from {prev_r} to {rr})"
            );
            prev_r = rr;
        }
        assert!(
            distinct.len() >= 8,
            "interior must show a real ramp, only {} distinct colours",
            distinct.len()
        );

        // The extreme corner pixel sits sqrt(r^2+r^2) ≈ 8.5px from the arc
        // centre, i.e. ~2.5px *outside* a 6px radius — so it must be cut away
        // entirely. What must not be flat is the arc itself: partial-coverage
        // pixels are what stop the corner from stair-stepping.
        assert_eq!(at(x, y), BG, "the radius must cut the extreme corner pixel");
        assert_eq!(
            at(x + w - 1, y + h - 1),
            BG,
            "the far corner must be cut as well"
        );
        let mut blended = 0;
        for col in x..(x + w) {
            let c = at(col, y);
            if c != BG && c != BORDER {
                blended += 1;
            }
        }
        assert!(
            blended >= 2,
            "the arc needs anti-aliased pixels along the top edge, found {blended}"
        );
    }

    /// The square (`radius = 0`) fallback clips at the viewport and still pins
    /// the ramp to the visible interior.
    #[test]
    fn test_gradient_square_fallback_clips_without_losing_the_ramp() {
        const BG: u32 = 0xFF18_181B;
        const TOP: u32 = 0xFFFF_0000;
        const BOTTOM: u32 = 0xFF00_00FF;
        const BORDER: u32 = 0xFF00_FF00;
        let stride = 40usize;
        let height = 30usize;
        let mut buf = vec![BG; stride * height];
        let clip = Rect {
            x: 0,
            y: 0,
            w: 15,
            h: height as i32,
        };

        // Rect spans x 5..35, y 3..23; the clip cuts it at x=15.
        draw_rounded_rect_gradient_box_clipped(
            &mut buf,
            stride,
            height,
            Some(clip),
            5,
            3,
            30,
            20,
            0, // square
            TOP,
            BOTTOM,
            BORDER,
            false,
        );

        let at = |col: usize, row: usize| buf[row * stride + col];
        assert_eq!(at(10, 4), TOP, "top interior row inside the clip is `top`");
        assert_eq!(
            at(10, 21),
            BOTTOM,
            "bottom interior row inside the clip is `bottom`"
        );
        assert_eq!(at(10, 3), BORDER, "the visible ring is still flat border");
        for x in 15..35 {
            assert_eq!(
                at(x, 10),
                BG,
                "x={x} is past the viewport and must stay background"
            );
        }
    }

    // -------------------------------------------------------------
    // 1.6 — Cursor & mouse state
    // -------------------------------------------------------------

    /// `disabled` and `tooltip` are ordinary props, so they have to survive
    /// the whole path from the language down to the box the hit-test reads.
    /// If this ever breaks the renderer silently shows every control as live.
    #[test]
    fn test_disabled_and_tooltip_props_reach_layout() {
        let mut col = UiElement::new("column");

        let mut btn = UiElement::new("button");
        btn.text = Some("Send".into());
        btn.on_click = Some("on_send".into());
        btn.props.insert("disabled".into(), "true".into());
        btn.props.insert("tooltip".into(), "Send the draft".into());

        let mut inp = UiElement::new("input");
        inp.id = Some("draft".into());
        inp.props.insert("disabled".into(), "1".into());
        inp.props.insert("tooltip".into(), "Type here".into());

        col.children.push(btn);
        col.children.push(inp);

        let mut boxes = Vec::new();
        layout_element(&col, 0, 0, 300, &mut boxes);

        let btn_box = boxes
            .iter()
            .find(|b| b.tag == "button")
            .expect("button must lay out");
        assert!(btn_box.disabled, "`disabled = true` must reach the box");
        assert_eq!(
            btn_box.tooltip.as_deref(),
            Some("Send the draft"),
            "`tooltip` must reach the box verbatim"
        );

        let inp_box = boxes
            .iter()
            .find(|b| b.tag == "input")
            .expect("input must lay out");
        assert!(inp_box.disabled, "the `1` spelling counts as disabled too");
        assert_eq!(inp_box.tooltip.as_deref(), Some("Type here"));

        // A control that says nothing about either prop stays fully live.
        let mut plain = UiElement::new("button");
        plain.text = Some("Go".into());
        let mut solo = Vec::new();
        layout_element(&plain, 0, 0, 300, &mut solo);
        assert!(!solo[0].disabled, "no prop means enabled");
        assert!(solo[0].tooltip.is_none(), "no prop means no tooltip");
    }

    fn test_box(tag: &str, id: &str, y: i32, disabled: bool) -> LayoutBox {
        LayoutBox {
            rect: Rect {
                x: 50,
                y,
                w: 100,
                h: 40,
            },
            tag: tag.into(),
            id: Some(id.into()),
            text: Some(id.into()),
            on_click: if tag == "button" {
                Some("click".into())
            } else {
                None
            },
            on_change: if tag == "input" {
                Some("changed".into())
            } else {
                None
            },
            on_key: None,
            pan: (0, 0),
            clips: None,
            pad: 0,
            disabled,
            tooltip: None,
            options: Vec::new(),
            checked: false,
            slider: None,
            draggable: false,
            props: std::collections::HashMap::new(),
        }
    }

    /// A choice field laid out the way the renderer lays one out: the widest
    /// option decides the width, so opening the list can never resize the
    /// field underneath it.
    fn select_box(id: &str, y: i32, value: &str, options: &[&str], disabled: bool) -> LayoutBox {
        let widest = options.iter().map(|o| o.chars().count()).max().unwrap_or(0);
        LayoutBox {
            rect: Rect {
                x: 50,
                y,
                w: (widest as i32 * 10 + 56).max(140),
                h: 36,
            },
            tag: "select".into(),
            id: Some(id.into()),
            text: Some(value.into()),
            on_click: None,
            on_change: Some("changed".into()),
            on_key: None,
            pan: (0, 0),
            clips: None,
            pad: 0,
            disabled,
            tooltip: None,
            options: options.iter().map(|o| o.to_string()).collect(),
            checked: false,
            slider: None,
            draggable: false,
            props: std::collections::HashMap::new(),
        }
    }

    /// The app the interaction tests drive. No window, so `render()` is a
    /// no-op and what is under test is the state — which list is open, what
    /// the keyboard is on, and what a press reports — rather than pixels. The
    /// dispatcher borrows the caller's `val` and `notes`, so a test can read
    /// exactly what a choice dispatched.
    fn select_app<'a>(
        val: &'a mut i32,
        notes: &'a mut String,
        key: &'a mut String,
        boxes: Vec<LayoutBox>,
    ) -> NativeApp<DummyDisp<'a>> {
        let mut app = NativeApp::new(
            WindowConfig {
                title: "T".into(),
                width: 400,
                height: 400,
            },
            DummyDisp {
                val,
                notes,
                last_key: key,
            },
        );
        app.layout_boxes = boxes;
        app
    }

    /// A disabled control is not merely un-clickable: the press has to *stop*
    /// with it, so nothing stacked behind it is clicked or focused by mistake.
    /// Its live neighbours must be entirely unaffected.
    #[test]
    fn test_disabled_button_is_inert_and_absorbs_the_click() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );

        app.layout_boxes = vec![
            test_box("button", "live", 50, false),
            test_box("button", "off", 110, true),
            // Stacked exactly behind the disabled one: it must never be hit.
            test_box("input", "behind", 110, false),
        ];

        assert_eq!(app.find_hovered_button((60, 120)), None, "not hoverable");
        app.cursor_pos = (60, 120);
        app.handle_click();
        assert_eq!(
            *app.dispatcher.val, 0,
            "a disabled button must never dispatch"
        );
        assert!(
            app.focused_id.is_none(),
            "…and must not let the click fall through to the input behind it"
        );

        app.cursor_pos = (60, 60);
        app.handle_click();
        assert_eq!(
            *app.dispatcher.val, 1,
            "the enabled button beside it must still work"
        );
    }

    /// Rest → hover → pressed is one continuous ladder of blues, each rung
    /// darker than the last, and the hover ramp is literally the blend between
    /// the first two rungs. Disabled is not a fourth colour but the resting
    /// pair dimmed toward the background — and no amount of hovering can talk
    /// an inert control back into a live tone.
    #[test]
    fn test_button_tones_form_one_ladder_and_disable_washes_out() {
        let t = Theme::DARK;
        let lum = |c: u32| ((c >> 16) & 0xFF) as i64 + ((c >> 8) & 0xFF) as i64 + (c & 0xFF) as i64;

        assert_eq!(
            button_tones(&t, 0.0, false, false),
            (t.button_bg_top, t.button_bg),
            "ramp at 0.0 is exactly the resting pair"
        );
        assert_eq!(
            button_tones(&t, 1.0, false, false),
            (t.button_bg, t.button_bg_hover),
            "ramp at 1.0 is exactly the hovered pair"
        );
        assert_eq!(
            button_tones(&t, 1.0, true, false),
            (t.button_bg_hover, t.button_bg_pressed),
            "pressing drops another rung"
        );

        let (rest_top, rest_bot) = button_tones(&t, 0.0, false, false);
        let (hov_top, _) = button_tones(&t, 1.0, false, false);
        let (prs_top, _) = button_tones(&t, 1.0, true, false);
        assert!(
            lum(rest_top) > lum(hov_top) && lum(hov_top) > lum(prs_top),
            "each rung must be darker than the one above: rest {} > hover {} > pressed {}",
            lum(rest_top),
            lum(hov_top),
            lum(prs_top)
        );

        // A ramp part-way must lie strictly between the two rungs on *both*
        // channels, so an in-between frame is a real blend and never a jump.
        let (mid_top, mid_bot) = button_tones(&t, 0.5, false, false);
        assert!(
            lum(mid_top) < lum(rest_top)
                && lum(mid_top) > lum(hov_top)
                && lum(mid_bot) < lum(rest_bot)
                && lum(mid_bot) > lum(t.button_bg_hover),
            "half-ramp must sit between rest and hover on both channels: top {} rest {} hover {}",
            lum(mid_top),
            lum(rest_top),
            lum(hov_top)
        );
        // Out-of-range values clamp rather than invent a new tone.
        assert_eq!(button_tones(&t, -1.0, false, false), (rest_top, rest_bot));
        assert_eq!(
            button_tones(&t, 2.0, false, false),
            (t.button_bg, t.button_bg_hover)
        );

        let disabled = button_tones(&t, 0.0, false, true);
        assert!(
            lum(disabled.0) < lum(rest_top) && lum(disabled.1) < lum(rest_bot),
            "disabled must be washed toward the background, not re-tinted"
        );
        assert_eq!(
            button_tones(&t, 1.0, true, true),
            disabled,
            "hover and press cannot re-light an inert control"
        );
    }

    /// The clock is one deadline, and it only ever moves in one direction:
    /// forward. A later ask must not postpone an earlier one, because the
    /// caret's next toggle arriving mid-ramp has no business delaying the
    /// ramp's own last frame.
    #[test]
    fn test_frame_clock_keeps_the_earliest_deadline_and_goes_idle() {
        let mut clock = FrameClock::new();
        assert!(clock.is_idle(), "a fresh clock has nothing outstanding");
        assert_eq!(clock.next_deadline(), None);

        let now = Instant::now();
        clock.wake_in(now, Duration::from_millis(200));
        assert_eq!(
            clock.next_deadline(),
            Some(now + Duration::from_millis(200))
        );

        clock.wake_in(now, Duration::from_millis(500));
        assert_eq!(
            clock.next_deadline(),
            Some(now + Duration::from_millis(200)),
            "a later ask must never push the deadline back"
        );

        clock.wake_in(now, Duration::from_millis(50));
        assert_eq!(
            clock.next_deadline(),
            Some(now + Duration::from_millis(50)),
            "an earlier one may pull it forward"
        );

        clock.clear();
        assert!(
            clock.is_idle(),
            "having drawn the frame, the clock is idle again"
        );
    }

    /// A hover is a ramp, not a swap: the button travels from its resting tone
    /// to the hovered one over [`HOVER_RAMP_SECS`], and the control the pointer
    /// just left fades back down at the same time instead of snapping. Entries
    /// that fade all the way out are dropped, so the map cannot grow with every
    /// button the pointer has ever visited.
    #[test]
    fn test_hover_ramps_blend_in_and_fade_back_out() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );
        app.layout_boxes = vec![
            test_box("button", "a", 50, false),
            test_box("button", "b", 110, false),
        ];
        let ka = button_key(&app.layout_boxes[0], 0);
        let kb = button_key(&app.layout_boxes[1], 1);

        assert!(
            !app.advance_hover_ramps(RAMP_TICK),
            "an untouched control is not animation"
        );
        assert!(
            app.hover_ramps.is_empty(),
            "and no entry is invented for a control nobody is on"
        );

        app.hovered_id = Some(ka.clone());
        assert!(
            app.advance_hover_ramps(RAMP_TICK),
            "a fresh ramp is in flight"
        );
        let t = app.hover_ramps[&ka];
        assert!(
            t > 0.0 && t < 1.0,
            "the first tick must land strictly between the two rungs, got {t}"
        );

        for _ in 0..20 {
            app.advance_hover_ramps(RAMP_TICK);
        }
        assert_eq!(
            app.hover_ramps[&ka], 1.0,
            "a finished ramp rests exactly on the hovered tone"
        );
        assert!(
            !app.ramps_in_flight(),
            "and once there it stops asking for frames"
        );

        app.hovered_id = Some(kb.clone());
        assert!(
            app.advance_hover_ramps(RAMP_TICK),
            "both halves of the move are animation"
        );
        assert!(
            app.hover_ramps[&ka] < 1.0,
            "the control just left must start fading rather than snap"
        );
        assert!(
            app.hover_ramps[&kb] > 0.0,
            "the one just entered must start climbing"
        );

        for _ in 0..20 {
            app.advance_hover_ramps(RAMP_TICK);
        }
        assert_eq!(app.hover_ramps[&kb], 1.0, "B finishes its own ramp");
        assert!(
            !app.hover_ramps.contains_key(&ka),
            "a control faded back to rest is dropped from the map"
        );
        assert!(!app.ramps_in_flight());

        app.hovered_id = None;
        for _ in 0..20 {
            app.advance_hover_ramps(RAMP_TICK);
        }
        assert!(
            app.hover_ramps.is_empty(),
            "a still interface holds no ramp state at all"
        );
    }

    /// The caret's blink is a phase, not a coin flip: on for
    /// [`CARET_BLINK_ON`], off for [`CARET_BLINK_OFF`], repeating from whenever
    /// the caret last moved — so typing can never land in the invisible half
    /// and make the keystroke look like it did nothing.
    #[test]
    fn test_caret_blink_follows_a_phase_that_a_move_restarts() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );
        app.layout_boxes = vec![test_box("input", "draft", 50, false)];
        let origin = Instant::now();

        assert_eq!(app.caret_phase(origin, origin), Duration::ZERO);
        assert_eq!(
            app.caret_phase(origin + CARET_BLINK_ON, origin),
            CARET_BLINK_ON
        );
        assert_eq!(
            app.caret_phase(origin + CARET_BLINK_ON + CARET_BLINK_OFF, origin),
            Duration::ZERO,
            "the cycle repeats"
        );

        // The deadline points at the *next* flip — never past the cycle, and
        // never at "now", which would have the loop spin.
        assert_eq!(app.next_caret_toggle(origin, origin), CARET_BLINK_ON);
        assert_eq!(
            app.next_caret_toggle(origin + CARET_BLINK_ON, origin),
            CARET_BLINK_OFF
        );
        let into_off = CARET_BLINK_ON + Duration::from_millis(10);
        assert_eq!(
            app.next_caret_toggle(origin + into_off, origin),
            (CARET_BLINK_ON + CARET_BLINK_OFF) - into_off
        );

        // Without a focusable field there is no caret to blink at all.
        assert!(!app.caret_visible(origin), "no focus, no caret");
        app.focused_id = Some("draft".into());
        app.caret_blink_origin = Some(origin);
        assert!(
            app.caret_visible(origin),
            "freshly placed, it is in the visible half"
        );
        assert!(app.caret_visible(origin + CARET_BLINK_ON - Duration::from_millis(1)));
        assert!(
            !app.caret_visible(origin + CARET_BLINK_ON),
            "half a cycle later it is not"
        );
        assert!(app.caret_visible(origin + CARET_BLINK_ON + CARET_BLINK_OFF));

        // Moving the caret restarts the visible phase, so the next frame after
        // a keystroke always shows it.
        app.touch_caret();
        assert!(
            app.caret_visible(Instant::now()),
            "moving the caret restarts the visible phase"
        );

        // An inert field can neither hold focus nor borrow a caret.
        app.layout_boxes = vec![test_box("input", "draft", 50, true)];
        assert!(
            !app.has_focused_input(),
            "a disabled field is not a caret host"
        );
        assert!(!app.caret_visible(Instant::now()));
    }

    /// The point of the whole clock: a still interface schedules *nothing*, so
    /// the loop can sleep in `ControlFlow::Wait` and cost no CPU. The moment a
    /// caret is blinking or a button is mid-ramp it asks for a frame and
    /// carries a deadline to sleep until.
    #[test]
    fn test_a_still_interface_schedules_no_frames() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );
        app.layout_boxes = vec![
            test_box("button", "a", 50, false),
            test_box("input", "draft", 110, false),
        ];

        app.schedule_frames(Instant::now());
        assert!(
            app.clock.is_idle(),
            "an idle interface must not keep the loop awake"
        );

        let before = Instant::now();
        app.focused_id = Some("draft".into());
        app.caret_blink_origin = Some(before);
        app.schedule_frames(before);
        let deadline = app.clock.next_deadline();
        assert!(deadline.is_some(), "a blinking caret needs a frame");
        assert!(
            deadline.unwrap() > before,
            "…due after the moment it was scheduled, not at it"
        );

        app.focused_id = None;
        app.schedule_frames(Instant::now());
        assert!(
            app.clock.is_idle(),
            "with nothing focused there is nothing left to wake for"
        );
        assert!(
            app.caret_blink_origin.is_none(),
            "and the caret's orphaned origin is dropped rather than paid for forever"
        );

        app.hovered_id = Some(button_key(&app.layout_boxes[0], 0));
        app.advance_hover_ramps(Duration::from_millis(8));
        assert!(
            app.ramps_in_flight(),
            "a half-ramped button is still moving"
        );
        app.schedule_frames(Instant::now());
        assert!(
            app.clock.next_deadline().is_some(),
            "so it needs its next tick"
        );
    }

    /// The mechanism that keeps an animating window off the CPU: the loop only
    /// spends a wake-up on a frame once the clock's moment has actually
    /// arrived. Asking for the frame from `schedule_frames` instead would
    /// rebuild the redraw that frame just consumed and pin the process at full
    /// speed, which is exactly what a frame clock exists to prevent.
    #[test]
    fn test_a_frame_is_spent_only_once_its_deadline_has_arrived() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );
        app.layout_boxes = vec![test_box("input", "draft", 50, false)];
        app.focused_id = Some("draft".into());

        // Arming the clock spends nothing on its own.
        let now = Instant::now();
        app.caret_blink_origin = Some(now);
        app.schedule_frames(now);
        assert!(app.clock.next_deadline().is_some());
        assert!(
            !app.fire_due_frame(),
            "the moment has not arrived, so no frame is spent"
        );
        assert!(
            app.clock.next_deadline().is_some(),
            "…and the promise is still outstanding for the wake-up that will keep it"
        );

        // Once the caret's toggle is genuinely due, the wake-up buys one frame
        // and only one. The deadline is always in the future when it is armed,
        // so this arms the last millisecond of the cycle and lets it lapse.
        let period = CARET_BLINK_ON + CARET_BLINK_OFF;
        app.caret_blink_origin = Some(Instant::now() - period + Duration::from_millis(1));
        app.schedule_frames(Instant::now());
        std::thread::sleep(Duration::from_millis(10));
        assert!(
            app.fire_due_frame(),
            "the deadline has passed, so the frame is drawn"
        );
        assert!(
            app.clock.is_idle(),
            "and it is spent, not left outstanding to fire again next iteration"
        );
        assert!(
            !app.fire_due_frame(),
            "a spent frame cannot be collected twice — this is what stops the loop spinning"
        );
    }

    /// A bar only appears when its axis actually has somewhere to go, and the
    /// two never fight over the corner: whichever is live takes it, and the
    /// other stops short. An axis with nothing to scroll is not drawn at all.
    #[test]
    fn test_scrollbar_tracks_appear_only_when_needed_and_yield_the_corner() {
        // Fits both ways: no bars, and so no way to grab either axis.
        let (v, h) = scrollbar_tracks(400, 300, 400, 300);
        assert!(
            v.is_none() && h.is_none(),
            "nothing to scroll, nothing drawn"
        );

        // Vertical only: it gets the whole right edge, corner included.
        let (v, h) = scrollbar_tracks(400, 300, 400, 900);
        let v = v.expect("tall content needs a vertical bar");
        assert!(h.is_none(), "narrow content needs no horizontal bar");
        assert_eq!(
            v,
            Rect {
                x: 400 - SCROLLBAR_SIZE,
                y: 0,
                w: SCROLLBAR_SIZE,
                h: 300
            },
            "a lone bar runs the full edge"
        );

        // Both: the vertical keeps the whole right edge including the corner,
        // and the horizontal runs along the bottom only as far as the vertical
        // bar's left edge — so the two abut without ever overlapping, and no
        // square of the window is left where neither bar answers the pointer.
        let (v, h) = scrollbar_tracks(400, 300, 900, 900);
        let v = v.expect("vertical");
        let h = h.expect("horizontal");
        assert_eq!(
            (v.x, v.y, v.w, v.h),
            (392, 0, SCROLLBAR_SIZE, 300),
            "the vertical bar keeps the whole edge, corner included"
        );
        assert_eq!(
            (h.x, h.y, h.w, h.h),
            (0, 292, 392, SCROLLBAR_SIZE),
            "the horizontal bar stops exactly at the vertical bar's left edge"
        );
        assert!(
            v.x >= h.x + h.w,
            "the two rectangles cannot overlap — otherwise the corner would be drawn twice"
        );

        // Horizontal only: it takes the full bottom edge.
        let (v, h) = scrollbar_tracks(400, 300, 900, 300);
        assert!(v.is_none());
        let h = h.expect("wide content needs a horizontal bar");
        assert_eq!(h.w, 400, "a lone bar runs the full edge");
    }

    /// The thumb is proportional to the visible share so it reads as "how much
    /// of this am I seeing", but never disappears: on very long content the
    /// minimum length takes over, which is the whole point of a minimum.
    #[test]
    fn test_thumb_length_is_proportional_floored_and_capped() {
        // Half the content visible -> half the track.
        assert_eq!(scrollbar_thumb_len(300, 500, 1000, 500), 150);
        // A quarter visible.
        assert_eq!(scrollbar_thumb_len(300, 250, 1000, 750), 75);
        // Absurdly long content: floored, not shrunk to nothing.
        assert_eq!(
            scrollbar_thumb_len(300, 100, 100_000, 99_900),
            SCROLLBAR_MIN_THUMB,
            "a thumb nobody can grab is no better than no thumb"
        );
        // More of the content visible than the track can hold: capped at the
        // track, since a thumb longer than its own channel is meaningless.
        assert_eq!(scrollbar_thumb_len(100, 600, 500, 100), 100);
        // Just over the track's worth of visible content, as the window's 20px
        // of slack can produce: still under the cap, not at it.
        assert_eq!(scrollbar_thumb_len(100, 500, 520, 20), 96);

        // Nothing to scroll -> no thumb at all.
        assert_eq!(scrollbar_thumb_len(300, 300, 300, 0), 0);
        assert_eq!(scrollbar_thumb_len(0, 300, 1000, 500), 0);
    }

    /// The thumb's position pins to both ends exactly when the axis is at its
    /// limit, and never runs off the track however the offset was reached.
    #[test]
    fn test_thumb_position_pins_at_both_ends_of_the_track() {
        let (track, thumb) = (300, 150);
        let travel = track - thumb;

        assert_eq!(scrollbar_thumb_pos(track, thumb, 0, 500), 0, "top");
        assert_eq!(
            scrollbar_thumb_pos(track, thumb, 500, 500),
            travel,
            "bottom — the thumb touches the end exactly when scrolling stops"
        );
        assert_eq!(
            scrollbar_thumb_pos(track, thumb, 250, 500),
            travel / 2,
            "halfway through is halfway down"
        );
        // Out-of-range offsets are clamped, not believed.
        assert_eq!(scrollbar_thumb_pos(track, thumb, -999, 500), 0);
        assert_eq!(scrollbar_thumb_pos(track, thumb, 99_999, 500), travel);
    }

    /// Dragging and drawing are inverse operations up to the track's own
    /// resolution: 240 pixels of track standing in for 940 offsets is ~4
    /// offsets per pixel, so a round trip can lose at most one pixel and never
    /// gains one. The ends are exact, which is what lets a user park the view.
    #[test]
    fn test_dragging_back_to_a_drawn_position_recovers_that_position() {
        let (track, thumb, max) = (300, 60, 940);
        let travel = track - thumb;
        for offset in [0, 1, 17, 300, 620, 940] {
            let pos = scrollbar_thumb_pos(track, thumb, offset, max);
            let back = scrollbar_offset_at(travel, pos, max);
            let pos_again = scrollbar_thumb_pos(track, thumb, back, max);
            assert!(
                pos_again <= pos && pos - pos_again <= 1,
                "offset {offset}: dragging back to pixel {pos} lands on {pos_again}, \
                 which is within the track's one-pixel resolution"
            );
        }
        // The ends are exact, not merely close.
        assert_eq!(scrollbar_thumb_pos(track, thumb, 0, max), 0);
        assert_eq!(scrollbar_offset_at(travel, 0, max), 0);
        assert_eq!(scrollbar_offset_at(travel, travel, max), max);
        assert_eq!(
            scrollbar_thumb_pos(track, thumb, max, max),
            travel,
            "…so the thumb touches the end exactly when the content stops"
        );
        // A degenerate track asks for nothing rather than dividing by zero.
        assert_eq!(scrollbar_offset_at(0, 40, max), 0);
        assert_eq!(scrollbar_offset_at(travel, 40, 0), 0);
    }

    /// A thumb beats the track it sits in — otherwise every grab would be read
    /// as a page jump — and the two axes are told apart by where they run.
    #[test]
    fn test_hit_testing_prefers_thumb_over_track() {
        let g = ScrollbarGeometry::resolve(400, 300, 400, 2000, 0, 0);
        let (v_track, v_thumb) = g.vertical.expect("vertical bar");

        assert_eq!(
            g.axis_at((v_thumb.x + 1, v_thumb.y + 1)),
            Some(ScrollbarAxis::Vertical),
            "on the thumb"
        );
        assert_eq!(
            g.axis_at((v_track.x + 1, v_track.h - 5)),
            Some(ScrollbarAxis::Vertical),
            "on bare track well below the thumb"
        );
        assert!(
            g.on_thumb((v_thumb.x + 1, v_thumb.y + 1)),
            "so the press is read as a grab"
        );
        assert!(
            !g.on_thumb((v_track.x + 1, v_track.h - 5)),
            "and a track press is read as a jump"
        );
        assert_eq!(
            g.axis_at((100, 150)),
            None,
            "the middle of the window belongs to the content"
        );

        // With both axes live, the corner goes to the vertical bar.
        let g = ScrollbarGeometry::resolve(400, 300, 2000, 2000, 0, 0);
        assert_eq!(
            g.axis_at((399, 299)),
            Some(ScrollbarAxis::Vertical),
            "the corner is the vertical bar's, not a dead zone"
        );
    }

    /// Pressing a thumb must not move it. The grab point is recorded, so the
    /// content follows the pointer exactly from wherever the press landed —
    /// which is what makes positioning precise instead of approximate.
    #[test]
    fn test_pressing_a_thumb_does_not_jump_it() {
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
                title: "T".into(),
                width: 200,
                height: 300,
            },
            dummy,
        );
        app.content_height = 3000;
        app.scroll_y = 400;

        let g = app.scrollbar_geometry();
        let (_, thumb) = g.vertical.expect("vertical bar");
        // Press near the thumb's bottom edge, not its middle.
        let press = (thumb.x + 1, thumb.y + thumb.h - 2);
        assert!(app.begin_scrollbar_drag(press), "the thumb was hit");
        assert_eq!(
            app.scroll_y, 400,
            "and the offset did not move — the press was a grab, not a jump"
        );

        // Now move the pointer down by exactly 10px: the offset moves by the
        // same distance, and the thumb lands back under the pointer — to
        // within the one pixel the integer scroll offset can resolve, since
        // several offsets share a track pixel.
        let moved = app.update_scrollbar_drag((press.0, press.1 + 10));
        assert!(moved, "the drag moved the content");
        let g2 = app.scrollbar_geometry();
        let (_, thumb2) = g2.vertical.expect("still there");
        let drift = (thumb2.y - (thumb.y + 10)).abs();
        assert!(
            drift <= 1,
            "the thumb followed the pointer by the pointer's own distance \
             (off by {drift}px, which is the track's resolution)"
        );
        assert!(
            thumb2.y > thumb.y,
            "and it moved the way the pointer did, not the other way"
        );
    }

    /// Pressing bare track pages the content to the pointer and then keeps
    /// dragging from there, rather than only jumping once.
    #[test]
    fn test_pressing_the_track_pages_and_then_drags() {
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
                title: "T".into(),
                width: 200,
                height: 300,
            },
            dummy,
        );
        app.content_height = 3000;
        app.scroll_y = 0;

        let g = app.scrollbar_geometry();
        let (track, _) = g.vertical.expect("vertical bar");
        // Click near the bottom of the track.
        assert!(app.begin_scrollbar_drag((track.x + 1, track.h - 2)), "hit");
        assert!(
            app.scroll_y > 0,
            "a track press pages the content toward the pointer"
        );
        let after_jump = app.scroll_y;

        // Dragging continues from there rather than snapping back to zero.
        assert!(app.update_scrollbar_drag((track.x + 1, track.h - 20)));
        assert!(
            app.scroll_y != after_jump,
            "and the gesture keeps dragging after the jump"
        );
        assert!(
            app.scroll_y < after_jump,
            "moving back up the track scrolls back up"
        );
    }

    /// A drag is clamped at both ends and reaches the very end exactly, so the
    /// thumb touches the track's end precisely when the content stops.
    #[test]
    fn test_a_drag_is_clamped_and_reaches_the_end_exactly() {
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
                title: "T".into(),
                width: 200,
                height: 300,
            },
            dummy,
        );
        app.content_height = 3000;
        app.scroll_y = 0;

        let g = app.scrollbar_geometry();
        let (track, thumb) = g.vertical.expect("vertical bar");
        assert!(app.begin_scrollbar_drag((thumb.x + 1, thumb.y + 1)), "grab");

        // Far past the bottom of the track.
        app.update_scrollbar_drag((thumb.x + 1, track.y + track.h + 5000));
        let max = max_scroll(app.content_height, app.height as i32);
        assert_eq!(
            app.scroll_y, max,
            "dragging past the end stops exactly at the end"
        );
        let g = app.scrollbar_geometry();
        let (track, thumb) = g.vertical.expect("vertical bar");
        assert_eq!(
            thumb.y + thumb.h,
            track.y + track.h,
            "…and the thumb is flush with the track's end there"
        );

        // Far above the top.
        app.update_scrollbar_drag((thumb.x + 1, track.y - 5000));
        assert_eq!(app.scroll_y, 0, "dragging above the top stops at zero");
        let g = app.scrollbar_geometry();
        let (_, thumb) = g.vertical.expect("vertical bar");
        assert_eq!(thumb.y, track.y, "…and the thumb is flush with the top");
    }

    /// The horizontal bar drives `scroll_x` and leaves `scroll_y` alone, and
    /// the two are told apart purely by where they run.
    #[test]
    fn test_horizontal_drag_drives_scroll_x_only() {
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
                title: "T".into(),
                width: 300,
                height: 200,
            },
            dummy,
        );
        app.content_width = 3000;
        app.content_height = 3000;
        app.scroll_x = 0;
        app.scroll_y = 700;

        let g = app.scrollbar_geometry();
        let (h_track, _) = g.horizontal.expect("horizontal bar");
        assert!(
            app.begin_scrollbar_drag((h_track.x + h_track.w - 2, h_track.y + 1)),
            "hit"
        );
        assert_eq!(app.scroll_y, 700, "the vertical offset is untouched");
        assert!(app.scroll_x > 0, "…and the horizontal one moved");

        app.update_scrollbar_drag((h_track.x + h_track.w - 2, h_track.y + 1));
        let g = app.scrollbar_geometry();
        let (_, thumb) = g.horizontal.expect("still there");
        assert_eq!(
            thumb.x + thumb.w,
            h_track.x + h_track.w,
            "dragged to the far end, the thumb is flush with it"
        );
    }

    /// A bar under the pointer swallows hover entirely: the tracks run down the
    /// window edge, where a control can sit, and a thumb that lights up the
    /// button behind it is a thumb nobody trusts.
    #[test]
    fn test_a_scrollbar_under_the_pointer_swallows_button_hover() {
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
                title: "T".into(),
                width: 200,
                height: 300,
            },
            dummy,
        );
        app.content_height = 3000;

        // A button whose box reaches under the vertical bar, and a card
        // describing it.
        let mut btn = test_box("button", "go", 50, false);
        btn.rect = Rect {
            x: 150,
            y: 50,
            w: 50,
            h: 40,
        };
        btn.tooltip = Some("Go".into());
        app.layout_boxes = vec![btn];

        let g = app.scrollbar_geometry();
        let (track, _) = g.vertical.expect("vertical bar");

        // Away from the bar, the button lights up as usual.
        app.handle_cursor_moved((160, 60));
        assert_eq!(
            app.hovered_id.as_deref(),
            Some("go"),
            "a button away from the bar still hovers"
        );
        assert_eq!(
            app.tooltip_at().as_deref(),
            Some("Go"),
            "and so does its card"
        );

        // On the bar, nothing does.
        app.handle_cursor_moved((track.x + 1, 60));
        assert_eq!(
            app.hovered_id, None,
            "the bar takes the hover, not the control behind it"
        );
        assert_eq!(
            app.tooltip_at(), None,
            "and the card goes with it — a tooltip about a control the bar is drawn over is a tooltip about the wrong thing"
        );
        assert_eq!(app.scrollbar_hover, Some(ScrollbarAxis::Vertical));
    }

    /// `render` re-derives hover on its own so the tone follows the layout
    /// rather than the last cursor event. That re-derivation has to ask the
    /// same question a cursor move does — otherwise every frame quietly hands
    /// the hover back to whatever sits behind a scrollbar, and the ramp the
    /// frame just started fading is given a target of 1.0 again. The live pass
    /// is what caught this: the cursor move swallowed the hover, the very next
    /// render gave it straight back, and three frames later the button sat at
    /// the tone it had started from.
    #[test]
    fn test_the_frames_own_hover_rederivation_also_yields_to_a_bar() {
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
                title: "T".into(),
                width: 200,
                height: 300,
            },
            dummy,
        );
        app.content_height = 3000;

        // A button whose box reaches under the vertical bar.
        let mut btn = test_box("button", "go", 50, false);
        btn.rect = Rect {
            x: 150,
            y: 50,
            w: 50,
            h: 40,
        };
        app.layout_boxes = vec![btn];

        let g = app.scrollbar_geometry();
        let (track, _) = g.vertical.expect("vertical bar");

        // Hover the button, then leave it for the bar.
        app.cursor_pos = (160, 60);
        app.refresh_hover();
        assert_eq!(app.hovered_id.as_deref(), Some("go"));

        app.handle_cursor_moved((track.x + 1, 60));
        assert_eq!(app.hovered_id, None, "the cursor move swallows it");

        // The next frame asks again, from the layout rather than the event. It
        // must still be swallowed.
        app.refresh_hover();
        assert_eq!(
            app.hovered_id, None,
            "a frame must not hand the hover back to the control behind the bar"
        );

        // And a pointer that leaves the bar lights the button up again: the
        // swallow follows the bar, not the pointer's history.
        app.handle_cursor_moved((160, 60));
        app.refresh_hover();
        assert_eq!(app.hovered_id.as_deref(), Some("go"));
    }

    /// The hover highlight is a real tone change, not a no-op, and holding the
    /// bar lifts it further still — so the thing that will move is visible
    /// before it does.
    #[test]
    fn test_scrollbar_tones_lift_on_hover_and_further_when_held() {
        let t = Theme::DARK;
        let lum = |c: u32| ((c >> 16) & 0xFF) as i64 + ((c >> 8) & 0xFF) as i64 + (c & 0xFF) as i64;

        let (rest_thumb, rest_track) = scrollbar_tones(&t, false, false);
        assert_eq!(
            (rest_thumb, rest_track),
            (t.scrollbar_thumb, t.scrollbar_track),
            "unhovered is exactly the theme's own tones"
        );

        let (hov_thumb, hov_track) = scrollbar_tones(&t, true, false);
        assert_ne!(hov_thumb, rest_thumb, "hovering the thumb changes it");
        assert_ne!(hov_track, rest_track, "and the track it runs in");

        let (held_thumb, _) = scrollbar_tones(&t, true, true);
        assert!(
            (lum(held_thumb) - lum(rest_thumb)).abs() > (lum(hov_thumb) - lum(rest_thumb)).abs(),
            "holding lifts the thumb further than hovering alone"
        );
        // Whatever the theme's text colour is, the point of the highlight is
        // contrast against the track — so the gap must widen, not shrink.
        assert!(
            (lum(hov_thumb) - lum(hov_track)).abs() >= (lum(rest_thumb) - lum(rest_track)).abs(),
            "hovering must not reduce the thumb's contrast against its track"
        );
    }

    /// Letting go restores whatever the drag had frozen: hover is blanked for
    /// the whole gesture, so on release a control under the pointer must light
    /// up immediately rather than on the next pixel of movement, and a bar the
    /// pointer has already left must not stay lit.
    #[test]
    fn test_releasing_the_drag_restores_frozen_hover() {
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
                title: "T".into(),
                width: 200,
                height: 300,
            },
            dummy,
        );
        app.content_height = 3000;

        let g = app.scrollbar_geometry();
        let (track, _) = g.vertical.expect("vertical bar");
        // The real order: the pointer moves onto the bar, then presses. During
        // a drag `handle_cursor_moved` still records the position before it
        // hands it to the drag, so release can re-derive from where the
        // pointer actually is.
        app.handle_cursor_moved((track.x + 1, 60));
        assert!(app.begin_scrollbar_drag(app.cursor_pos), "grab the bar");
        // What the press arm does: a grab owns the pointer outright.
        app.hovered_id = None;
        app.sync_cursor_icon();
        assert_eq!(
            app.cursor_kind,
            CursorKind::Grabbing,
            "the closed hand while the thumb is held"
        );

        // Move away from the bar while dragging. Hover stays frozen — that is
        // the point of the drag owning the pointer — and the bar the pointer
        // has left is still recorded as hovered, because nothing re-derives it
        // mid-drag.
        app.handle_cursor_moved((70, 70));
        assert_eq!(
            app.hovered_id, None,
            "hover is frozen for the whole gesture"
        );
        assert_eq!(
            app.cursor_kind,
            CursorKind::Grabbing,
            "…and so is the hand, however far the pointer wanders"
        );
        assert_eq!(
            app.scrollbar_hover,
            Some(ScrollbarAxis::Vertical),
            "and the stale bar is exactly what release has to clean up"
        );

        // A drag scrolls the page, and scrolling can carry a control
        // underneath a pointer that has not moved. Measured against the offset
        // the drag has already left behind, so it really is under there.
        let mut btn = test_box("button", "btn", 50, false);
        btn.rect = Rect {
            x: 40,
            y: app.scroll_y + 50,
            w: 60,
            h: 40,
        };
        app.layout_boxes = vec![btn];
        assert_eq!(
            app.hovered_id, None,
            "…and it still must not light up, with a control now under the pointer"
        );

        assert!(app.end_scrollbar_drag(), "there was a drag to end");
        assert!(app.scrollbar_drag.is_none());
        assert_eq!(
            app.hovered_id.as_deref(),
            Some("btn"),
            "the control under the released pointer lights up at once, not on the next move"
        );
        assert_eq!(
            app.scrollbar_hover, None,
            "…and no bar stays lit that the pointer has already left"
        );
        assert_ne!(
            app.cursor_kind,
            CursorKind::Grabbing,
            "the closed hand does not outlive the drag"
        );

        // Releasing *while still on* the bar keeps it hovered — it is simply
        // ready to be grabbed again.
        app.handle_cursor_moved((track.x + 1, 60));
        assert!(app.begin_scrollbar_drag(app.cursor_pos), "grab again");
        app.hovered_id = None;
        assert!(app.end_scrollbar_drag());
        assert_eq!(app.scrollbar_hover, Some(ScrollbarAxis::Vertical));
        assert_eq!(app.hovered_id, None, "still on the bar, so still no button");
        assert_eq!(app.cursor_kind, CursorKind::Grab, "…and the open hand");

        // Ending when nothing was dragging is a no-op rather than a panic.
        assert!(!app.end_scrollbar_drag());
    }

    /// Focus is what the border signals, and an inert field must not be able
    /// to borrow that ring. Its surface is the resting one washed toward the
    /// window background — the same treatment the disabled button gets, not a
    /// colour of its own.
    #[test]
    fn test_input_surface_washes_out_when_disabled() {
        let t = Theme::DARK;
        assert_eq!(
            input_surface(&t, false, false),
            (t.input_bg, t.input_border),
            "resting"
        );
        assert_eq!(
            input_surface(&t, true, false),
            (t.input_bg, t.input_border_focused),
            "focus shows in the border"
        );

        let inert = input_surface(&t, true, true);
        assert_eq!(
            inert,
            input_surface(&t, false, true),
            "an inert field ignores focus entirely"
        );
        assert_eq!(inert.1, t.border, "…and takes the plain border");
        assert_ne!(inert.0, t.input_bg, "its surface must be washed out");
        assert!(
            (inert.0 & 0xFF) < (t.input_bg & 0xFF),
            "washed toward the darker window background: got {:08x} vs input_bg {:08x}",
            inert.0,
            t.input_bg
        );
    }

    /// The cursor shape is what tells a user what a click will do *before*
    /// they click: the hand over a live button, the I-beam over a text field,
    /// nothing special anywhere else — least of all over a disabled control,
    /// which must not advertise an interaction it will refuse.
    #[test]
    fn test_cursor_kind_follows_what_is_under_the_pointer() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );
        app.layout_boxes = vec![
            test_box("button", "live", 50, false),
            test_box("input", "draft", 110, false),
            test_box("button", "off", 170, true),
        ];

        app.handle_cursor_moved((10, 10));
        assert_eq!(
            app.cursor_kind,
            CursorKind::Default,
            "empty space takes the arrow"
        );

        app.handle_cursor_moved((60, 60));
        assert_eq!(
            app.cursor_kind,
            CursorKind::Pointer,
            "a live button takes the hand"
        );

        app.handle_cursor_moved((60, 120));
        assert_eq!(
            app.cursor_kind,
            CursorKind::Text,
            "a text field takes the I-beam"
        );

        app.handle_cursor_moved((60, 180));
        assert_eq!(
            app.cursor_kind,
            CursorKind::Default,
            "a disabled button keeps the arrow"
        );
    }

    /// The tooltip is an overlay: it paints last, so it covers whatever lies
    /// beneath it, and because no layout box holds it in the document flow it
    /// is clamped inside the window instead of running off the edge.
    #[test]
    fn test_tooltip_paints_on_top_and_stays_inside_the_window() {
        const BG: u32 = 0xFF18_181B; // Theme::DARK.bg
        const UNDER: u32 = 0xFFFF_0000;
        let t = Theme::DARK;
        let stride = 140usize;
        let height = 100usize;
        let fonts = FontEngine::new(BASE_PX);
        let label = "Send the draft";
        let w = fonts.measure_width(FaceKind::Ui, label) + 2 * 7;
        let h = fonts.cell_h + 2 * 5;
        let interior = |x: i32, y: i32| ((y + h - 3) as usize) * stride + (x + w / 2) as usize;

        // Content already on screen underneath the pointer.
        let mut buf = vec![BG; stride * height];
        draw_rect(&mut buf, stride, height, 40, 40, 60, 30, UNDER);

        draw_tooltip(
            &mut buf,
            stride,
            height,
            &fonts,
            &t,
            stride as i32,
            height as i32,
            30,
            30,
            label,
        );
        assert_eq!(
            buf[interior(43, 43)],
            t.input_bg,
            "the card must cover the content beneath it rather than blend with it"
        );

        // Pointer in the bottom-right corner: the card slides back inside
        // instead of running off both edges.
        let mut buf2 = vec![BG; stride * height];
        draw_tooltip(
            &mut buf2,
            stride,
            height,
            &fonts,
            &t,
            stride as i32,
            height as i32,
            stride as i32 - 4,
            height as i32 - 4,
            label,
        );
        let x = (stride as i32 - 4 + 13).clamp(0, (stride as i32 - w).max(0));
        let y = (height as i32 - 4 + 13).clamp(0, (height as i32 - h).max(0));
        assert!(x + w <= stride as i32, "must still fit horizontally");
        assert!(y + h <= height as i32, "must still fit vertically");
        assert_eq!(
            buf2[interior(x, y)],
            t.input_bg,
            "the clamped card still paints its interior"
        );
    }

    /// Regression guard: a tooltip both appears and *follows* the pointer, so
    /// moving onto, within and off a labelled box must each ask for a repaint.
    /// Keying that off the button-hover change — which never moves here, since
    /// a text label is not hoverable — left the card frozen on screen and it
    /// was never erased.
    #[test]
    fn test_moving_around_a_labelled_box_requests_repaints() {
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
                title: "T".into(),
                width: 300,
                height: 300,
            },
            dummy,
        );
        let mut labelled = test_box("text", "tip", 50, false);
        labelled.tooltip = Some("what this does".into());
        app.layout_boxes = vec![labelled];

        assert!(
            !app.handle_cursor_moved((10, 10)),
            "empty space needs no repaint"
        );
        assert_eq!(app.hovered_id, None, "a label is not a button");
        assert!(
            app.handle_cursor_moved((60, 60)),
            "entering a labelled box must repaint to show the card"
        );
        assert!(app.tooltip_shown, "the card is on screen");
        assert!(
            app.handle_cursor_moved((70, 60)),
            "moving within it must repaint so the card follows the pointer"
        );
        assert!(
            app.handle_cursor_moved((10, 10)),
            "leaving it must repaint to erase the card"
        );
        assert!(!app.tooltip_shown, "…and now it is gone");
    }

    /// Hover, the pressed tone and the actual dispatch must all resolve a
    /// button through the same function; otherwise a press can light up one
    /// box and fire a different one the moment layout order shifts.
    #[test]
    fn test_button_key_is_shared_by_hover_press_and_dispatch() {
        fn btn_box(id: Option<&str>, on_click: Option<&str>) -> LayoutBox {
            LayoutBox {
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                tag: "button".into(),
                id: id.map(str::to_string),
                text: Some("b".into()),
                on_click: on_click.map(str::to_string),
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 0,
                disabled: false,
                tooltip: None,
                options: Vec::new(),
                checked: false,
                slider: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            }
        }

        assert_eq!(
            button_key(&btn_box(Some("send"), Some("on_send")), 7),
            "send",
            "an explicit id wins"
        );
        assert_eq!(
            button_key(&btn_box(None, Some("on_send")), 7),
            "on_send",
            "then the handler"
        );
        assert_eq!(
            button_key(&btn_box(None, None), 7),
            "__btn_7",
            "then layout order, so the key is never empty"
        );

        // Hover reports that same shared key.
        let boxes = vec![btn_box(None, None)];
        assert_eq!(
            find_hovered_button_in_boxes(&boxes, 0, 0, (5, 5)),
            Some("__btn_0".into()),
            "an unlabelled button still reports the shared key"
        );
    }

    /// WCAG relative luminance of an opaque `0xAARRGGBB` colour.
    fn rel_lum(c: u32) -> f64 {
        let lin = |ch: u32| {
            let v = ch as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin((c >> 16) & 0xFF) + 0.7152 * lin((c >> 8) & 0xFF) + 0.0722 * lin(c & 0xFF)
    }

    /// WCAG contrast ratio between two opaque colours, 1.0 … 21.0.
    fn contrast(a: u32, b: u32) -> f64 {
        let (la, lb) = (rel_lum(a), rel_lum(b));
        let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// The light theme must earn its place the same way the dark one did:
    /// every pairing a user actually reads has to clear its contrast bar on a
    /// bright page. This is the whole reason `LIGHT` re-picks each role rather
    /// than flipping the ladder — an inverted caret, an inverted shadow or an
    /// un-lifted accent would all pass a naive "not equal to dark" check and
    /// still be unusable.
    #[test]
    fn test_light_theme_is_legible_on_a_bright_surface() {
        let d = Theme::DARK;
        let l = Theme::LIGHT;

        assert_ne!(l, d, "a light theme that equals the dark one is no theme");

        // Body copy and dimmed copy on the page.
        assert!(
            contrast(l.text, l.bg) >= 7.0,
            "body text must be AAA on the light page: {}",
            contrast(l.text, l.bg)
        );
        assert!(
            contrast(l.text_dim, l.bg) >= 4.5,
            "dimmed text must stay readable on the light page: {}",
            contrast(l.text_dim, l.bg)
        );

        // White button label on a *brighter* accent than the dark theme uses.
        assert!(
            contrast(l.button_text, l.button_bg) >= 4.5,
            "white button text must clear AA on Blue-600: {}",
            contrast(l.button_text, l.button_bg)
        );
        assert!(
            l.button_bg != d.button_bg,
            "the accent must step up rather than stay at the dark theme's rung"
        );

        // The caret is the trap: on a white field a white caret vanishes.
        assert!(
            contrast(l.cursor, l.input_bg) >= 7.0,
            "the caret must be obvious in a light input: {}",
            contrast(l.cursor, l.input_bg)
        );

        // Selection sits *behind* dark text, so its own contrast with that
        // text is what matters — not with the page.
        assert!(
            contrast(l.text, l.selection_bg) >= 4.5,
            "selected text must stay readable over the selection: {}",
            contrast(l.text, l.selection_bg)
        );

        // Inputs and their borders have to be told apart from the page. On a
        // light theme the *fill* can barely differ from the page — white on
        // Zinc-50 is 1.04:1 by nature, and every light UI lives with that —
        // so the border carries the whole "this is a field" job alone. That
        // is why the light theme needs a darker step than the dark one does.
        assert!(
            contrast(l.input_border, l.bg) >= 2.0,
            "the light input must still read as a field: border {}",
            contrast(l.input_border, l.bg)
        );
        assert!(
            contrast(l.input_border, l.input_bg) >= 2.0,
            "the border must stand out against the field it frames: {}",
            contrast(l.input_border, l.input_bg)
        );

        // A black halo on a light page reads as dirt, not depth.
        assert_ne!(
            l.shadow, 0xFF00_0000,
            "the light theme must soften the shadow instead of reusing black"
        );
        assert_ne!(
            l.shadow, d.shadow,
            "the shadow has to actually differ from the dark theme's"
        );
    }

    /// `theme:` names come from `.ndg` text, so lookup forgives the ordinary
    /// ways that text arrives — capitalisation and padding — but a name it
    /// does not know must resolve to nothing at all, because "nothing" is what
    /// keeps the palette already in force and "dark" would silently undo a
    /// program that asked for something else.
    #[test]
    fn test_theme_named_forgives_case_and_padding_but_not_typos() {
        assert_eq!(Theme::named("dark"), Some(Theme::DARK));
        assert_eq!(Theme::named("light"), Some(Theme::LIGHT));
        assert_eq!(Theme::named("Light"), Some(Theme::LIGHT), "capitalised");
        assert_eq!(Theme::named("  LIGHT  "), Some(Theme::LIGHT), "padded");
        assert_eq!(Theme::named("DaRk"), Some(Theme::DARK));

        assert_eq!(Theme::named("solarized"), None, "an unknown name is None");
        assert_eq!(Theme::named(""), None, "so is an empty one");
        assert_eq!(Theme::named("lite"), None, "and so is a near miss");
    }

    /// The trait offers a theme but never insists on one: a dispatcher written
    /// before this existed keeps compiling *and* keeps its palette, because the
    /// default answer is "no opinion" and the frame then paints whatever is
    /// already in force.
    #[test]
    fn test_dispatcher_theme_defaults_to_no_opinion() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let mut dummy = DummyDisp {
            val: &mut val,
            notes: &mut notes,
            last_key: &mut key,
        };
        assert_eq!(
            dummy.theme(),
            None,
            "the default hook must express no preference"
        );
    }

    // -------------------------------------------------------------
    // select / dropdown (roadmap 2.2)
    // -------------------------------------------------------------

    /// The card is drawn where there is room for it: under the field when
    /// there is, over it only when there is nowhere else to go. It is never
    /// allowed to leave the window, because a list that runs off the edge is
    /// a list whose last rows cannot be clicked.
    #[test]
    fn test_select_popup_rect_flips_above_and_stays_in_the_window() {
        // Room below: the card hangs off the field's bottom edge.
        let below = select_popup_rect(
            Rect {
                x: 50,
                y: 60,
                w: 200,
                h: 36,
            },
            3,
            400,
            400,
        );
        assert_eq!(below.y, 96, "below the field when there is room");
        assert_eq!(below.h, 3 * SELECT_ROW_H + 2 * SELECT_POP_PAD);
        assert_eq!(below.x, 50, "the card starts where its field starts");

        // No room below: it flips above rather than running off the bottom.
        let above = select_popup_rect(
            Rect {
                x: 50,
                y: 330,
                w: 200,
                h: 36,
            },
            3,
            400,
            400,
        );
        assert_eq!(
            above.y + above.h,
            330,
            "above the field when there is no room below"
        );
        assert!(
            above.y >= 0,
            "…and it must not be pushed off the top instead"
        );

        // More options than either side can hold: the card is clipped to the
        // window and the list scrolls inside itself.
        let clipped = select_popup_rect(
            Rect {
                x: 50,
                y: 180,
                w: 200,
                h: 36,
            },
            40,
            400,
            400,
        );
        assert!(
            clipped.y + clipped.h <= 400,
            "a clipped card must still end inside the window"
        );
        assert!(
            select_visible_rows(clipped.h) < 40,
            "…which means not every option fits, so the list has to scroll"
        );
    }

    /// A press has to land on the row the eye sees: the hit test and the draw
    /// use one geometry, so a click in the padding or past the last drawn row
    /// takes nothing.
    #[test]
    fn test_select_row_hit_matches_the_drawn_rows() {
        let field = Rect {
            x: 50,
            y: 60,
            w: 200,
            h: 36,
        };
        let popup = select_popup_rect(field, 3, 400, 400);
        let rows = select_visible_rows(popup.h);
        assert_eq!(rows, 3, "three options, three rows");

        let first_row_y = popup.y + SELECT_POP_PAD + SELECT_ROW_H / 2;
        assert_eq!(select_row_at(popup, 3, 0, (60, first_row_y)), Some(0));
        assert_eq!(
            select_row_at(popup, 3, 0, (60, first_row_y + SELECT_ROW_H)),
            Some(1),
            "the second row is one row-height down, no more, no less"
        );
        assert_eq!(
            select_row_at(popup, 3, 0, (60, popup.y + 2)),
            None,
            "the card's own padding is not a row"
        );
        assert_eq!(
            select_row_at(popup, 3, 0, (60, first_row_y + 3 * SELECT_ROW_H)),
            None,
            "nor is the space under the last row"
        );
        assert_eq!(select_row_at(popup, 3, 0, (10, 10)), None, "nor is outside");
    }

    /// The whole point of the control: press it, press an option, and the
    /// handler is handed that option — and the list shuts behind it.
    #[test]
    fn test_select_click_opens_then_a_row_click_reports_the_value() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let mut app = select_app(
            &mut val,
            &mut notes,
            &mut key,
            vec![select_box(
                "model",
                60,
                "Small",
                &["Small", "Medium", "Large"],
                false,
            )],
        );

        app.cursor_pos = (60, 70);
        app.handle_click();
        let popup = app
            .open_select_popup()
            .expect("a press on a choice field opens its list");
        assert_eq!(
            popup.highlight, 0,
            "the keyboard starts on the current value"
        );

        // A press on a row reports that row's option and shuts the list.
        app.cursor_pos = (
            60,
            popup.rect.y + SELECT_POP_PAD + SELECT_ROW_H + SELECT_ROW_H / 2,
        );
        app.handle_click();
        assert_eq!(
            *app.dispatcher.notes, "Medium",
            "the pressed row is the value reported"
        );
        assert!(app.open_select.is_none(), "and the list shuts behind it");
    }

    /// The list has to survive the keyboard alone: arrows walk it, Enter takes
    /// the row they landed on, and Escape abandons the lot.
    #[test]
    fn test_select_keyboard_walks_the_list_and_enter_commits() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let mut app = select_app(
            &mut val,
            &mut notes,
            &mut key,
            vec![select_box(
                "model",
                60,
                "Small",
                &["Small", "Medium", "Large"],
                false,
            )],
        );

        app.cursor_pos = (60, 70);
        app.handle_click();
        app.cursor_pos = (0, 0); // pointer away, so the keyboard owns the row
        app.handle_key_input(&Key::Named(NamedKey::ArrowDown), None);
        app.handle_key_input(&Key::Named(NamedKey::ArrowDown), None);
        assert_eq!(
            app.open_select.as_ref().map(|o| o.highlight),
            Some(2),
            "two presses down walk two rows"
        );

        app.handle_key_input(&Key::Named(NamedKey::Enter), None);
        assert_eq!(
            *app.dispatcher.notes, "Large",
            "Enter takes the row the keyboard landed on"
        );
        assert!(app.open_select.is_none(), "…and shuts the list");

        // A fresh list abandoned with Escape reports nothing at all.
        app.cursor_pos = (60, 70);
        app.handle_click();
        app.handle_key_input(&Key::Named(NamedKey::Escape), None);
        assert!(app.open_select.is_none(), "Escape shuts the list");
        assert_eq!(
            *app.dispatcher.notes, "Large",
            "…without committing anything"
        );
    }

    /// A press outside a list is spent on shutting it. Letting it fall through
    /// would mean every dismissal also presses whatever control the card was
    /// covering — which is how people delete things by accident.
    #[test]
    fn test_a_press_outside_an_open_list_is_spent_shutting_it() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let mut app = select_app(
            &mut val,
            &mut notes,
            &mut key,
            vec![
                select_box("model", 60, "Small", &["Small", "Medium"], false),
                // A button far from the card, in the clear.
                test_box("button", "go", 300, false),
            ],
        );

        app.cursor_pos = (60, 70);
        app.handle_click();
        assert!(app.open_select.is_some(), "the list is up");

        app.cursor_pos = (60, 310);
        app.handle_click();
        assert!(app.open_select.is_none(), "the press shuts it");
        assert_eq!(*app.dispatcher.val, 0, "…and is spent, not passed through");
    }

    /// An inert field is inert the same way an inert button is: the press
    /// stops with it and nothing opens.
    #[test]
    fn test_a_disabled_select_absorbs_the_press_and_opens_nothing() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let mut app = select_app(
            &mut val,
            &mut notes,
            &mut key,
            vec![select_box("model", 60, "Small", &["Small", "Medium"], true)],
        );

        assert_eq!(app.find_hovered_button((60, 70)), None, "not hoverable");
        app.cursor_pos = (60, 70);
        app.handle_click();
        assert!(app.open_select.is_none(), "a disabled field opens nothing");
    }

    /// A list taller than the room below its field has to reach its last
    /// option — by scrolling inside its own card, never by growing past the
    /// window.
    #[test]
    fn test_a_long_list_scrolls_and_still_reaches_its_last_option() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let options: Vec<String> = (1..=12).map(|i| format!("Option {i}")).collect();
        let refs: Vec<&str> = options.iter().map(|s| s.as_str()).collect();
        let mut app = select_app(
            &mut val,
            &mut notes,
            &mut key,
            vec![select_box("pick", 60, "Option 1", &refs, false)],
        );

        app.cursor_pos = (60, 70);
        app.handle_click();
        let popup = app.open_select_popup().expect("the list opens");
        assert!(
            popup.rows < 12,
            "the window cannot show every row, so the list must scroll"
        );
        assert!(
            popup.rect.y + popup.rect.h <= 400,
            "…and stay inside the window"
        );

        app.cursor_pos = (0, 0);
        app.handle_key_input(&Key::Named(NamedKey::End), None);
        let popup = app.open_select_popup().expect("the list is still up");
        assert_eq!(popup.highlight, 11, "End walks to the last option");
        // The list scrolled to reach it, so the last option is drawn as the
        // card's last row rather than past the window's edge.
        let last_row_y = popup.rect.y + SELECT_POP_PAD + (11 - popup.first) as i32 * SELECT_ROW_H;
        assert!(
            last_row_y + SELECT_ROW_H <= popup.rect.y + popup.rect.h,
            "…and the row it lands on is a row the card actually draws"
        );
        assert_eq!(
            select_row_at(
                popup.rect,
                12,
                popup.first,
                (60, last_row_y + SELECT_ROW_H / 2)
            ),
            Some(11),
            "so a press there takes exactly that option"
        );

        app.handle_key_input(&Key::Named(NamedKey::Enter), None);
        assert_eq!(
            *app.dispatcher.notes, "Option 12",
            "Enter takes the option End landed on"
        );
    }

    /// Scrolling the field out of view has to take its list with it. A card
    /// left floating where the field used to be is a card about nothing.
    #[test]
    fn test_a_field_scrolled_out_of_sight_has_no_list() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let mut app = select_app(
            &mut val,
            &mut notes,
            &mut key,
            vec![select_box(
                "model",
                60,
                "Small",
                &["Small", "Medium"],
                false,
            )],
        );

        app.cursor_pos = (60, 70);
        app.handle_click();
        assert!(app.open_select.is_some(), "the list is up");

        app.scroll_y = 400; // the field is now far above the window
        assert_eq!(
            app.open_select_popup(),
            None,
            "the list is not drawn once its field is out of sight"
        );
    }

    /// A choice field has to look like one before it is ever pressed: a value,
    /// a chevron saying it opens, and a hover so the field that will open is
    /// visible before it does.
    #[test]
    fn test_a_closed_select_paints_its_value_and_a_chevron() {
        let t = Theme::DARK;
        let stride = 200usize;
        let height = 100usize;
        let mut buf = vec![t.bg; stride * height];

        draw_chevron_down(&mut buf, stride, height, None, 40, 20, t.text_dim);
        // The chevron is a downward triangle: wide at the top, one pixel at
        // the bottom, and centred on the width it was given.
        assert_eq!(buf[20 * stride + 43], t.text_dim, "its top row is wide");
        assert_eq!(
            buf[23 * stride + 43],
            t.text_dim,
            "its bottom row is a point"
        );
        assert_eq!(
            buf[23 * stride + 40],
            t.bg,
            "…so the bottom row is narrower than the top, not a slab"
        );
        assert_eq!(
            buf[21 * stride + 39],
            t.bg,
            "the triangle's left flank slopes"
        );
    }

    /// The list is drawn over whatever it drops onto — it is a card, not a
    /// hole — and it lights the row the pointer is on, which is the row a
    /// press would take.
    #[test]
    fn test_the_open_list_covers_the_page_and_lights_the_pointer_row() {
        let t = Theme::DARK;
        let fonts = FontEngine::new(BASE_PX);
        let stride = 300usize;
        let height = 200usize;
        let mut buf = vec![t.bg; stride * height];
        // Content already on screen underneath where the card will land.
        draw_rect(&mut buf, stride, height, 40, 60, 120, 60, 0xFF00_FF00);

        let popup = SelectPopupView {
            key: "model".into(),
            rect: Rect {
                x: 50,
                y: 60,
                w: 160,
                h: 2 * SELECT_ROW_H + 2 * SELECT_POP_PAD,
            },
            options: vec!["Small".into(), "Medium".into()],
            first: 0,
            rows: 2,
            highlight: 0,
            value: Some("Small".into()),
            id: Some("model".into()),
        };
        draw_select_popup(&mut buf, stride, height, &fonts, &t, &popup, Some(1));

        // The card's own padding, which sits over the green page content:
        // a card is opaque, so whatever it lands on is replaced, not tinted.
        assert_eq!(
            buf[(60 + 2) as usize * stride + 60],
            t.input_bg,
            "the card covers the page beneath it rather than blending with it"
        );
        let row1_y = 60 + SELECT_POP_PAD + SELECT_ROW_H + 1;
        assert_eq!(
            buf[row1_y as usize * stride + 60],
            t.selection_bg,
            "the row under the pointer lights up, since that is the row a press would take"
        );
        let row0_y = 60 + SELECT_POP_PAD + 1;
        assert_eq!(
            buf[row0_y as usize * stride + 60],
            t.input_bg,
            "…and the row the pointer is not on stays unlit"
        );
    }

    /// An id-less field cannot report a choice — there is nowhere for the
    /// value to go — so it must not pretend to open one.
    #[test]
    fn test_a_select_without_an_id_opens_nothing() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let mut box_ = select_box("model", 60, "Small", &["Small", "Medium"], false);
        box_.id = None;
        let mut app = select_app(&mut val, &mut notes, &mut key, vec![box_]);

        app.cursor_pos = (60, 70);
        app.handle_click();
        assert!(
            app.open_select.is_none(),
            "a field with nowhere to report to opens nothing"
        );
    }

    /// Hover and the open list agree on which control is which, so the field
    /// that lights up is the one that opens.
    #[test]
    fn test_select_keys_resolve_the_same_way_for_hover_and_the_open_list() {
        let with_id = select_box("model", 60, "Small", &["Small"], false);
        assert_eq!(select_key(&with_id, 3), "model", "an id is the key");

        let mut without_id = select_box("model", 60, "Small", &["Small"], false);
        without_id.id = None;
        assert_eq!(
            select_key(&without_id, 3),
            "__sel_3",
            "…and layout order is the fallback"
        );
    }

    /// A `checkbox` or `toggle` element laid out the way the renderer lays
    /// one out, so the interaction tests drive a real box rather than a
    /// hand-written one.
    fn flag_box(tag: &str, id: Option<&str>, label: &str, on: bool, disabled: bool) -> LayoutBox {
        let mut e = UiElement::new(tag);
        e.id = id.map(str::to_string);
        e.text = Some(label.into());
        if on {
            e.props.insert("value".into(), "true".into());
        }
        if disabled {
            e.props.insert("disabled".into(), "true".into());
        }
        let mut boxes = Vec::new();
        layout_element(&e, 50, 60, 300, &mut boxes);
        boxes
            .into_iter()
            .next()
            .expect("a flag lays out exactly one box")
    }

    /// A button sitting exactly where a control does, for the absorption
    /// checks: if the control stops the press, this one has to stay quiet.
    fn btn_box_at(rect: Rect) -> LayoutBox {
        LayoutBox {
            rect,
            tag: "button".into(),
            id: Some("behind".into()),
            text: Some("behind".into()),
            on_click: Some("behind".into()),
            on_change: None,
            on_key: None,
            pan: (0, 0),
            clips: None,
            pad: 0,
            disabled: false,
            tooltip: None,
            options: Vec::new(),
            checked: false,
            slider: None,
            draggable: false,
            props: std::collections::HashMap::new(),
        }
    }

    /// The dispatcher the flag tests drive: it keeps *which* control changed
    /// as well as what it became, because a press that reports the right
    /// word through the wrong box is a bug a value-only recorder would miss.
    struct FlagDisp<'a> {
        id: &'a mut String,
        value: &'a mut String,
        clicks: &'a mut i32,
    }

    impl<'a> UiDispatcher for FlagDisp<'a> {
        fn root(&mut self) -> Option<UiElement> {
            None
        }
        fn dispatch_click(&mut self, _button_id: &str) {
            *self.clicks += 1;
        }
        fn dispatch_change(&mut self, element_id: &str, value: &str) {
            *self.id = element_id.to_string();
            *self.value = value.to_string();
        }
        fn dispatch_key(&mut self, _key: &str, _target_id: Option<&str>) {}
    }

    fn flag_app<'a>(
        id: &'a mut String,
        value: &'a mut String,
        clicks: &'a mut i32,
    ) -> NativeApp<FlagDisp<'a>> {
        NativeApp::new(
            WindowConfig {
                title: "T".into(),
                width: 400,
                height: 400,
            },
            FlagDisp { id, value, clicks },
        )
    }

    /// The mark is two arms meeting at a low vertex, not a filled square —
    /// and it stays inside the eleven columns it was given, so a control that
    /// draws its own mark cannot spill past the square it belongs to.
    #[test]
    fn test_the_check_mark_is_two_arms_and_stays_where_it_was_put() {
        let t = Theme::DARK;
        let stride = 40usize;
        let height = 30usize;
        let mut buf = vec![t.bg; stride * height];

        draw_check_mark(&mut buf, stride, height, None, 10, 8, t.button_text);

        // The falling arm starts high and to the left…
        assert_eq!(
            buf[12 * stride + 10],
            t.button_text,
            "the mark begins on the left"
        );
        // …meets the climbing arm at the bottom…
        assert_eq!(
            buf[18 * stride + 15],
            t.button_text,
            "the two arms meet at a low vertex"
        );
        // …and the climbing arm finishes above where the falling one began.
        assert_eq!(
            buf[11 * stride + 20],
            t.button_text,
            "the second arm climbs higher than the first started"
        );

        // One pixel wide, so a check reads as a stroke rather than a slab.
        assert_eq!(
            buf[12 * stride + 11],
            t.bg,
            "the mark is a stroke, not a filled block"
        );
        // The arms do not reach each other's ends: this is a V, not a line.
        assert_eq!(
            buf[11 * stride + 10],
            t.bg,
            "the falling arm does not start at the top"
        );
        assert_eq!(
            buf[18 * stride + 20],
            t.bg,
            "…and the climbing arm does not reach the bottom"
        );
    }

    /// On and off must be told apart without reading anything: the square
    /// that is on wears the accent and carries a white mark, the one that is
    /// off is just a field — no mark, and nothing that looks switched on.
    #[test]
    fn test_an_on_checkbox_paints_accent_and_a_mark_while_an_off_one_is_bare() {
        let t = Theme::DARK;
        let stride = 60usize;
        let height = 60usize;
        let row = Rect {
            x: 10,
            y: 10,
            w: 200,
            h: FLAG_H,
        };

        let on_view = FlagView::new("checkbox", row, true, 0.0, false);
        let mut on_buf = vec![t.bg; stride * height];
        draw_flag_control(&mut on_buf, stride, height, None, &t, &on_view);

        let off_view = FlagView::new("checkbox", row, false, 0.0, false);
        let mut off_buf = vec![t.bg; stride * height];
        draw_flag_control(&mut off_buf, stride, height, None, &t, &off_view);

        let r = on_view.rect;
        assert_eq!(
            r.w, FLAG_BOX,
            "the square is as wide as a checkbox's own control"
        );
        let face = (r.y + 3) as usize * stride + (r.x + 8) as usize;
        assert_eq!(
            on_buf[face], t.input_border_focused,
            "a box that is on wears the accent"
        );
        assert_eq!(
            off_buf[face], t.input_bg,
            "…and one that is off is an ordinary field"
        );

        let mark = (r.y + 9) as usize * stride + (r.x + 3) as usize;
        assert_eq!(
            on_buf[mark], t.button_text,
            "the mark is drawn in white so it clears contrast on the accent"
        );
        assert_eq!(
            off_buf[mark], t.input_bg,
            "an off box carries no mark at all"
        );
        let off_white = off_buf
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                **c == t.button_text
                    && (*i % stride) >= r.x as usize
                    && (*i % stride) < (r.x + r.w) as usize
                    && (*i / stride) >= r.y as usize
                    && (*i / stride) < (r.y + r.h) as usize
            })
            .count();
        assert_eq!(off_white, 0, "nothing white inside a box that is off");

        // The corners are cut, so the square reads as a control rather than
        // as a stray pixel block.
        assert_eq!(
            on_buf[(r.y as usize) * stride + (r.x as usize)],
            t.bg,
            "the corner is rounded away"
        );
    }

    /// A toggle states itself by where the knob sits — the one difference a
    /// switch has that needs no reading — and it is the same size either
    /// way, so flipping it cannot shove the row it belongs to.
    #[test]
    fn test_a_toggle_states_itself_by_where_the_knob_sits() {
        let t = Theme::DARK;
        let stride = 60usize;
        let height = 60usize;
        let row = Rect {
            x: 10,
            y: 10,
            w: 200,
            h: FLAG_H,
        };

        let off = FlagView::new("toggle", row, false, 0.0, false);
        let on = FlagView::new("toggle", row, true, 0.0, false);
        assert_eq!(off.rect, on.rect, "flipping a switch must not move it");
        assert_eq!(off.rect.w, TOGGLE_W, "and it is as wide as a track");
        assert_eq!(off.rect.h, TOGGLE_H, "and as tall as one too");

        let mut off_buf = vec![t.bg; stride * height];
        draw_flag_control(&mut off_buf, stride, height, None, &t, &off);
        let mut on_buf = vec![t.bg; stride * height];
        draw_flag_control(&mut on_buf, stride, height, None, &t, &on);

        let r = off.rect;
        let mid = (r.y + r.h / 2) as usize;
        assert_eq!(
            off_buf[mid * stride + (r.x + 12) as usize],
            t.text_dim,
            "the knob sits at the left end when the switch is off"
        );
        assert_eq!(
            off_buf[mid * stride + (r.x + 36) as usize],
            t.input_bg,
            "…and the right end is empty track"
        );
        assert_eq!(
            on_buf[mid * stride + (r.x + 32) as usize],
            t.button_text,
            "the knob sits at the right end when it is on"
        );
        assert_eq!(
            on_buf[mid * stride + (r.x + 8) as usize],
            t.input_border_focused,
            "…and the left end is the lit track"
        );
    }

    /// A flag's row is the control and its label together, measured once.
    /// The state has to come from the element's `value`, and neither the
    /// label's length nor the state may change how tall the row is.
    #[test]
    fn test_a_flag_layouts_as_one_row_carried_by_its_value_and_its_words() {
        let on = flag_box("checkbox", Some("dark"), "Dark mode", true, false);
        assert_eq!(on.tag, "checkbox");
        assert!(
            on.checked,
            "…so the drawing never has to go looking for the value"
        );
        assert_eq!(
            on.text.as_deref(),
            Some("Dark mode"),
            "the label rides as the box's text, because that is what is drawn"
        );
        assert_eq!(on.id.as_deref(), Some("dark"));
        assert_eq!(
            on.rect.h, FLAG_H,
            "fixed height: a long label widens the row, never makes it taller"
        );
        assert_eq!(
            on.rect.w,
            FLAG_BOX + FLAG_GAP + 9 * CELL_W + 8,
            "control, gap, every character, and the room left at the end"
        );

        let off = flag_box("checkbox", Some("dark"), "Dark mode", false, false);
        assert!(
            !off.checked,
            "an absent or false `value` is off, which is the default"
        );
        assert_eq!(
            off.rect, on.rect,
            "…and the state does not change the room the row takes"
        );

        let tgl = flag_box("toggle", None, "Dark mode", false, false);
        assert_eq!(
            tgl.rect.w,
            TOGGLE_W + FLAG_GAP + 9 * CELL_W + 8,
            "a toggle claims more room for its own control than a checkbox"
        );
        assert!(tgl.rect.w > on.rect.w);
        assert_eq!(tgl.rect.h, FLAG_H, "and is still one row tall");
    }

    /// Only a flag that can report hovers: one with no `id` has nowhere to
    /// put the new value, and lighting a control that will not answer the
    /// click is a promise the window cannot keep. The label counts — it is
    /// part of the row the press covers.
    #[test]
    fn test_hover_lights_a_flag_that_can_report_and_leaves_the_rest_dark() {
        let with_id = flag_box("checkbox", Some("dark"), "Dark mode", false, false);
        let no_id = flag_box("checkbox", None, "Light mode", false, false);
        let disabled = flag_box("toggle", Some("beta"), "Beta", false, true);
        let label_pos = (
            with_id.rect.x + with_id.rect.w - 4,
            with_id.rect.y + with_id.rect.h - 4,
        );

        assert_eq!(
            find_hovered_button_in_boxes(std::slice::from_ref(&with_id), 0, 0, label_pos),
            Some("dark".to_string()),
            "the whole row — control and label — is the control"
        );
        assert_eq!(
            find_hovered_button_in_boxes(std::slice::from_ref(&no_id), 0, 0, label_pos),
            None,
            "one with nowhere to report must not pretend to be live"
        );
        assert_eq!(
            find_hovered_button_in_boxes(&[disabled], 0, 0, label_pos),
            None,
            "and a disabled one is inert even though it can report"
        );

        // Hover and the press resolve the key the same way, so the control
        // that lit up is the one that flips.
        assert_eq!(
            flag_key(&with_id, 7),
            "dark",
            "an id is the key when there is one"
        );
        assert_eq!(
            flag_key(&no_id, 7),
            "__flag_7",
            "…and layout order is the fallback"
        );
    }

    /// A press reports a *change* — the flipped value, through the control's
    /// id — and never a click, because a click that flips a switch has no
    /// meaning of its own beside the flip. The label presses too: the words
    /// belong to the control, and text next to a switch that does nothing
    /// when clicked is text people learn to avoid.
    #[test]
    fn test_pressing_a_flag_reports_the_flip_and_the_label_presses_too() {
        let mut id = String::new();
        let mut value = String::new();
        let mut clicks = 0;
        let mut app = flag_app(&mut id, &mut value, &mut clicks);
        app.layout_boxes = vec![flag_box(
            "checkbox",
            Some("dark"),
            "Dark mode",
            false,
            false,
        )];

        let b = app.layout_boxes[0].rect;
        app.cursor_pos = (b.x + b.w - 4, b.y + b.h - 4);
        app.handle_click();
        assert_eq!(
            *app.dispatcher.id, "dark",
            "the press reports through the control's id"
        );
        assert_eq!(
            *app.dispatcher.value, "true",
            "…and an off box comes back on"
        );
        assert_eq!(
            *app.dispatcher.clicks, 0,
            "a flag reports a change, never a separate click"
        );

        // The other direction, through the same route.
        app.layout_boxes[0].checked = true;
        app.cursor_pos = (b.x + 4, b.y + b.h / 2);
        app.handle_click();
        assert_eq!(
            *app.dispatcher.value, "false",
            "a box that is on comes back off"
        );

        // Hover is only the promise; a disabled control keeps it to itself.
        let mut id2 = String::new();
        let mut value2 = String::new();
        let mut clicks2 = 0;
        let mut off = flag_app(&mut id2, &mut value2, &mut clicks2);
        off.layout_boxes = vec![flag_box("toggle", Some("beta"), "Beta", false, true)];
        let rb = off.layout_boxes[0].rect;
        off.cursor_pos = (rb.x + 4, rb.y + rb.h / 2);
        off.handle_click();
        assert!(
            value2.is_empty(),
            "a disabled control dispatches nothing at all"
        );
        assert_eq!(clicks2, 0, "and is not mistaken for a button either");
    }

    /// A control that cannot report still has to *take* the press: the words
    /// next to it absorb it rather than letting it fall through to whatever
    /// is stacked behind. The control experiment is the button on its own —
    /// it fires when nothing stands in front of it.
    #[test]
    fn test_a_flag_that_cannot_report_takes_the_press_with_it() {
        let flag = flag_box("checkbox", None, "Decoration", false, false);
        let behind = btn_box_at(flag.rect);

        // Nothing in front: the button behind is reachable.
        let mut id = String::new();
        let mut value = String::new();
        let mut clicks = 0;
        let mut bare = flag_app(&mut id, &mut value, &mut clicks);
        bare.layout_boxes = vec![behind.clone()];
        bare.cursor_pos = (flag.rect.x + 4, flag.rect.y + flag.rect.h / 2);
        bare.handle_click();
        assert_eq!(clicks, 1, "control experiment: the button fires alone");

        // An id-less flag stands in front of it.
        let mut id1 = String::new();
        let mut value1 = String::new();
        let mut clicks1 = 0;
        let mut no_id = flag_app(&mut id1, &mut value1, &mut clicks1);
        no_id.layout_boxes = vec![flag.clone(), behind.clone()];
        no_id.cursor_pos = (flag.rect.x + 4, flag.rect.y + flag.rect.h / 2);
        no_id.handle_click();
        assert_eq!(
            clicks1, 0,
            "the press stops with the flag instead of reaching the button"
        );
        assert!(value1.is_empty(), "…and it reports nothing, since it can't");

        // So does a disabled one.
        let disabled = flag_box("checkbox", Some("dark"), "Decoration", false, true);
        let behind2 = btn_box_at(disabled.rect);
        let mut id2 = String::new();
        let mut value2 = String::new();
        let mut clicks2 = 0;
        let mut dim = flag_app(&mut id2, &mut value2, &mut clicks2);
        dim.layout_boxes = vec![disabled, behind2];
        dim.cursor_pos = (flag.rect.x + 4, flag.rect.y + flag.rect.h / 2);
        dim.handle_click();
        assert_eq!(
            clicks2, 0,
            "a disabled control absorbs the press rather than passing it on"
        );
    }

    // -------------------------------------------------------------
    // slider (roadmap 2.4)
    // -------------------------------------------------------------

    /// A `slider` element laid out the way the renderer lays one out, so the
    /// interaction tests drive a real box rather than a hand-written one.
    fn slider_box(
        id: Option<&str>,
        label: &str,
        value: f64,
        min: f64,
        max: f64,
        step: f64,
        disabled: bool,
    ) -> LayoutBox {
        let mut e = UiElement::new("slider");
        e.id = id.map(str::to_string);
        e.text = Some(label.into());
        e.props.insert("value".into(), value.to_string());
        e.props.insert("min".into(), min.to_string());
        e.props.insert("max".into(), max.to_string());
        e.props.insert("step".into(), step.to_string());
        if disabled {
            e.props.insert("disabled".into(), "true".into());
        }
        let mut boxes = Vec::new();
        layout_element(&e, 50, 60, 300, &mut boxes);
        boxes
            .into_iter()
            .next()
            .expect("a slider lays out exactly one box")
    }

    /// The middle of a laid-out slider's track, at a given fraction of its
    /// travel — the point a test wants to press or drag to.
    fn slider_track_at(lbox: &LayoutBox, t: f32) -> (i32, i32) {
        let track = slider_track_rect(lbox.rect);
        (
            track.x + (t * SLIDER_TRACK_W as f32) as i32,
            track.y + track.h / 2,
        )
    }

    /// The two mappings have to be inverses, and both have to leave the
    /// handle inside the groove: a control whose handle can be asked for one
    /// place and put somewhere else cannot be aimed at.
    #[test]
    fn test_slider_value_and_position_are_inverses() {
        let spec = SliderSpec {
            value: 25.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
        };
        // Every rung round-trips through a pixel position.
        for v in [0.0, 1.0, 25.0, 99.0, 100.0] {
            let x = spec.value_to_x(v);
            assert_eq!(spec.x_to_value(x), v, "value {v} round-trips through x");
        }
        // The extremes put the handle *inside* the track, not hanging off it.
        let half = SLIDER_KNOB / 2;
        assert_eq!(
            spec.value_to_x(0.0),
            half as f32,
            "min sits at the left stop"
        );
        assert_eq!(
            spec.value_to_x(100.0),
            (SLIDER_TRACK_W - half) as f32,
            "max sits at the right stop"
        );

        // A fractional slider is the same shape, one pixel further apart.
        let frac = SliderSpec {
            value: 0.7,
            min: 0.0,
            max: 1.0,
            step: 0.1,
        };
        assert_eq!(frac.x_to_value(frac.value_to_x(0.7)), 0.7);
        assert_eq!(frac.x_to_value(frac.value_to_x(0.0)), 0.0);
        assert_eq!(frac.x_to_value(frac.value_to_x(1.0)), 1.0);

        // Off both ends the answer is still a legal value, never an overshoot.
        assert_eq!(spec.x_to_value(-500.0), 0.0);
        assert_eq!(spec.x_to_value(5000.0), 100.0);
    }

    /// A handle must stay inside the groove it belongs to, at any value — a
    /// handle drawn past the end would read as a number the control cannot
    /// produce.
    #[test]
    fn test_slider_handle_stays_inside_its_groove() {
        let row = Rect {
            x: 40,
            y: 90,
            w: 260,
            h: SLIDER_H,
        };
        for value in [0.0, 33.3, 50.0, 100.0] {
            let spec = SliderSpec {
                value,
                min: 0.0,
                max: 100.0,
                step: 0.1,
            };
            let view = SliderView::new(row, 0.0, false, spec);
            let knob = view.knob_rect();
            // Horizontally the handle belongs to the *groove*: it may never
            // hang off either end, because a handle past the stop would read
            // as a value the control cannot produce.
            assert!(
                knob.x >= view.rect.x,
                "handle {knob:?} leaves the groove's left"
            );
            assert!(
                knob.x + knob.w <= view.rect.x + view.rect.w,
                "…or its right: {knob:?}"
            );
            // Vertically it belongs to the *row*: the handle is deliberately
            // taller than the groove it rides in, the way a knob rides a
            // switch, so only the row is the limit there.
            assert!(knob.y >= row.y, "…or above its own row: {knob:?}");
            assert!(
                knob.y + knob.h <= row.y + row.h,
                "…or below its own row: {knob:?}"
            );
            assert_eq!(
                view.filled,
                (spec.value_to_x(value).round() as i32).clamp(0, SLIDER_TRACK_W)
            );
        }
    }

    /// The point of the control: press it, drag it, and the handler is handed
    /// each number the finger passed over — and the handle is where the finger
    /// left it, not where the press happened to land.
    #[test]
    fn test_slider_drag_reports_each_step_and_leaves_the_handle_under_the_finger() {
        let mut id = String::new();
        let mut value = String::new();
        let mut clicks = 0;
        let box_ = slider_box(Some("temp"), "Temp", 0.0, 0.0, 100.0, 1.0, false);
        let track = slider_track_rect(box_.rect);
        let spec = box_.slider.expect("a slider carries its spec");
        let mut app = flag_app(&mut id, &mut value, &mut clicks);
        app.layout_boxes = vec![box_.clone()];

        // Press at the far left of the track and drag to the far right.
        app.cursor_pos = (track.x + 1, track.y + track.h / 2);
        app.begin_slider_drag(app.cursor_pos);
        assert!(
            app.dispatcher.value.is_empty(),
            "the slider already starts at the left stop, so there is nothing to report yet"
        );

        for x in [track.x + 40, track.x + 80, track.x + SLIDER_TRACK_W - 1] {
            app.update_slider_drag((x, track.y + track.h / 2));
        }
        assert_eq!(
            *app.dispatcher.value, "100",
            "…and the drag walked to the far end"
        );
        assert_eq!(
            app.dispatcher.id, "temp",
            "reported through the slider's own id"
        );

        // The handle is where the drag left it, which is the mapping's own
        // answer — not a value recomputed somewhere else. Rebuilt from the
        // value that was actually reported, so the drawing and the report are
        // checked against each other rather than against the start.
        let at_100 = SliderSpec {
            value: 100.0,
            ..spec
        };
        let handle = SliderView::new(box_.rect, 0.0, false, at_100).knob_rect();
        // The travel is inset by the handle's own radius, so at the right
        // stop the handle sits just inside the groove rather than hanging off
        // it — which is the whole reason the mapping insets in the first
        // place. Checked against the mapping's answer, not against the pixel
        // the finger reached.
        let stop = track.x + at_100.value_to_x(100.0).round() as i32;
        assert_eq!(
            handle.x + handle.w / 2,
            stop,
            "the handle sits on the mapping's own right stop: {handle:?}"
        );
        assert!(
            handle.x + handle.w <= track.x + SLIDER_TRACK_W,
            "…so it never hangs off the end of the groove: {handle:?}"
        );
    }

    /// Pressing the *handle* must not teleport it: the offset the press
    /// arrived at is carried through the whole drag, which is the difference
    /// between a control you can aim and one you can only shove.
    #[test]
    fn test_slider_press_on_the_handle_keeps_its_offset() {
        let mut id = String::new();
        let mut value = String::new();
        let mut clicks = 0;
        let box_ = slider_box(Some("temp"), "Temp", 50.0, 0.0, 100.0, 1.0, false);
        let track = slider_track_rect(box_.rect);
        let handle_x = track.x
            + SliderSpec {
                value: 50.0,
                min: 0.0,
                max: 100.0,
                step: 1.0,
            }
            .value_to_x(50.0)
            .round() as i32;

        // Press five pixels left of the handle's centre. The handle must not
        // leap under the pointer: the press alone reports nothing, because the
        // value has not moved.
        let mut app = flag_app(&mut id, &mut value, &mut clicks);
        app.layout_boxes = vec![box_.clone()];
        app.cursor_pos = (handle_x - 5, track.y + track.h / 2);
        app.begin_slider_drag(app.cursor_pos);
        assert!(
            app.dispatcher.value.is_empty(),
            "pressing off centre does not teleport the handle: nothing has moved yet"
        );
        // Without the recorded offset this press would have reported ~46,
        // which is the number at the *pointer*, not at the handle.
        let naive = SliderSpec {
            value: 50.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
        }
        .x_to_value((handle_x - 5 - track.x) as f32);
        assert!(
            naive < 48.0,
            "the naive answer is well below the value: {naive}"
        );

        app.update_slider_drag((handle_x - 4, track.y + track.h / 2));

        // Press dead centre and drag the same one pixel.
        let mut id2 = String::new();
        let mut value2 = String::new();
        let mut clicks2 = 0;
        let mut centred = flag_app(&mut id2, &mut value2, &mut clicks2);
        centred.layout_boxes = vec![slider_box(
            Some("temp"),
            "Temp",
            50.0,
            0.0,
            100.0,
            1.0,
            false,
        )];
        centred.cursor_pos = (handle_x, track.y + track.h / 2);
        centred.begin_slider_drag(centred.cursor_pos);
        centred.update_slider_drag((handle_x + 1, track.y + track.h / 2));

        // One pixel of drag is one step of the handle — the offset shifts
        // where the handle sits relative to the finger, not how it responds.
        assert_eq!(
            *app.dispatcher.value, "51",
            "one pixel of drag moves the handle one pixel: {}",
            *app.dispatcher.value
        );
        assert_eq!(
            *centred.dispatcher.value, "51",
            "pressing on centre moves the value by the distance dragged"
        );
    }

    /// Pressing bare track is the obvious reading of the gesture: the value
    /// goes to where the pointer landed, and the handle then stays under the
    /// finger for the rest of the drag.
    #[test]
    fn test_slider_press_on_bare_track_jumps_to_the_pointer() {
        let mut id = String::new();
        let mut value = String::new();
        let mut clicks = 0;
        let box_ = slider_box(Some("temp"), "Temp", 0.0, 0.0, 100.0, 1.0, false);
        let track = slider_track_rect(box_.rect);
        let mut app = flag_app(&mut id, &mut value, &mut clicks);
        app.layout_boxes = vec![box_.clone()];

        let press = slider_track_at(&box_, 0.75);
        let spec = box_.slider.expect("a slider carries its spec");
        app.cursor_pos = press;
        app.begin_slider_drag(app.cursor_pos);
        // The press lands on exactly the value the mapping names for that
        // pixel — which is *not* three-quarters of 100, because the handle's
        // travel is inset by its own radius and so starts a little in.
        let expected = spec.x_to_value((press.0 - track.x) as f32);
        assert_eq!(
            *app.dispatcher.value,
            crate::ui::slider_num_str(expected, spec.step),
            "a press on bare track lands at the mapping's own answer for that pixel"
        );
        assert_ne!(
            *app.dispatcher.value, "75",
            "a fraction of the row is not a fraction of the travel — the inset is real"
        );

        // …and the very next pixel of drag does not fling the handle, because
        // the grab offset was re-measured from where the handle now is.
        app.update_slider_drag((press.0 + 1, track.y + 5));
        let next = spec.x_to_value((press.0 + 1 - track.x) as f32);
        assert_eq!(
            *app.dispatcher.value,
            crate::ui::slider_num_str(next, spec.step),
            "one pixel of drag is one step, not a handle's width"
        );
    }

    /// A slider with nowhere to report is decoration: it lights up only if the
    /// drag will be answered, and a disabled one is dim and inert.
    #[test]
    fn test_a_slider_only_lights_when_it_can_answer() {
        let live = slider_box(Some("temp"), "Temp", 0.0, 0.0, 100.0, 1.0, false);
        let mut no_id = live.clone();
        no_id.id = None;
        let dim = slider_box(Some("temp"), "Temp", 0.0, 0.0, 100.0, 1.0, true);

        let here = slider_track_at(&live, 0.5);
        assert_eq!(
            find_hovered_button_in_boxes(std::slice::from_ref(&live), 0, 0, here),
            Some("temp".into()),
            "a live slider lights up under the pointer"
        );
        assert_eq!(
            find_hovered_button_in_boxes(std::slice::from_ref(&no_id), 0, 0, here),
            None,
            "…an id-less one does not: there is nowhere to put the value"
        );
        assert_eq!(
            find_hovered_button_in_boxes(std::slice::from_ref(&dim), 0, 0, here),
            None,
            "…and neither does a disabled one"
        );

        // Off the row entirely, no control claims it.
        assert_eq!(
            find_hovered_button_in_boxes(std::slice::from_ref(&live), 0, 0, (2, 2)),
            None
        );
    }

    /// A slider that cannot move must still take the press: letting it fall
    /// through would fire whatever control is stacked behind it.
    #[test]
    fn test_a_slider_absorbs_the_press_even_when_it_cannot_move() {
        let behind = |lbox: &LayoutBox| btn_box_at(lbox.rect);

        // Control experiment: with no slider in front, the button fires.
        let live = slider_box(Some("temp"), "Temp", 0.0, 0.0, 100.0, 1.0, false);
        let mut id = String::new();
        let mut value = String::new();
        let mut clicks = 0;
        let mut bare = flag_app(&mut id, &mut value, &mut clicks);
        bare.layout_boxes = vec![behind(&live)];
        bare.cursor_pos = slider_track_at(&live, 0.5);
        bare.handle_click();
        assert_eq!(clicks, 1, "control experiment: the button fires alone");

        // A disabled slider stands in front of it.
        let dim = slider_box(Some("temp"), "Temp", 0.0, 0.0, 100.0, 1.0, true);
        let mut id2 = String::new();
        let mut value2 = String::new();
        let mut clicks2 = 0;
        let mut app = flag_app(&mut id2, &mut value2, &mut clicks2);
        app.layout_boxes = vec![dim.clone(), behind(&dim)];
        app.cursor_pos = slider_track_at(&dim, 0.5);
        app.begin_slider_drag(app.cursor_pos);
        assert_eq!(
            clicks2, 0,
            "a disabled slider absorbs the press rather than passing it on"
        );
        assert!(
            value2.is_empty(),
            "…and reports nothing, since it cannot move"
        );

        // So does an id-less one.
        let mut no_id = live.clone();
        no_id.id = None;
        let mut id3 = String::new();
        let mut value3 = String::new();
        let mut clicks3 = 0;
        let mut inert = flag_app(&mut id3, &mut value3, &mut clicks3);
        inert.layout_boxes = vec![no_id.clone(), behind(&no_id)];
        inert.cursor_pos = slider_track_at(&no_id, 0.5);
        inert.begin_slider_drag(inert.cursor_pos);
        assert_eq!(clicks3, 0, "an id-less slider absorbs it too");
    }

    /// Dragging owns the pointer the way a scrollbar drag does: releasing it
    /// clears the drag, and nothing can be dragged twice.
    #[test]
    fn test_a_slider_drag_ends_on_release() {
        let mut id = String::new();
        let mut value = String::new();
        let mut clicks = 0;
        let box_ = slider_box(Some("temp"), "Temp", 0.0, 0.0, 100.0, 1.0, false);
        let track = slider_track_rect(box_.rect);
        let mut app = flag_app(&mut id, &mut value, &mut clicks);
        app.layout_boxes = vec![box_.clone()];
        app.cursor_pos = slider_track_at(&box_, 0.5);

        assert!(app.begin_slider_drag(app.cursor_pos), "the press was taken");
        assert!(app.slider_drag.is_some(), "…and is now held");
        // The drag is holding the control it started on, at the track's own
        // left edge — so the value it computes is measured from the groove
        // and not from wherever the window happens to begin.
        assert_eq!(
            app.slider_drag.as_ref().map(|d| d.track_x),
            Some(track.x),
            "the drag remembers the track it grabbed"
        );
        assert!(app.end_slider_drag(), "there was a drag to end");
        assert!(app.slider_drag.is_none(), "…and it is over");
        assert!(!app.end_slider_drag(), "ending twice ends nothing");
    }

    /// The drawing: a groove, a travelled part in the accent, and a handle
    /// whose position *is* the number. Painted, so the proof is pixels rather
    /// than an assertion about arithmetic the painter might ignore.
    #[test]
    fn test_the_slider_paints_a_groove_a_fill_and_a_handle_on_its_value() {
        let t = Theme::DARK;
        let stride = SLIDER_TRACK_W as usize + 8;
        let height = SLIDER_H as usize;
        let row = Rect {
            x: 0,
            y: 0,
            w: SLIDER_TRACK_W + 40,
            h: SLIDER_H,
        };
        let track = slider_track_rect(row);
        let mid_y = (track.y + track.h / 2) as usize;

        let paint = |value: f64| -> Vec<u32> {
            let spec = SliderSpec {
                value,
                min: 0.0,
                max: 100.0,
                step: 1.0,
            };
            let mut buf = vec![t.bg; stride * height];
            let view = SliderView::new(row, 0.0, false, spec);
            draw_slider_control(&mut buf, stride, height, None, &t, &view);
            buf
        };

        // At the left stop there is no travelled part to speak of, and the
        // handle is at the left end of the groove.
        let left = paint(0.0);
        let knob = SliderView::new(
            row,
            0.0,
            false,
            SliderSpec {
                value: 0.0,
                min: 0.0,
                max: 100.0,
                step: 1.0,
            },
        )
        .knob_rect();
        assert_eq!(
            left[mid_y * stride + (knob.x + knob.w / 2) as usize],
            t.button_text,
            "at the left stop the handle itself is the only bright thing"
        );
        assert_eq!(
            left[mid_y * stride + (track.x + track.w - 8) as usize],
            t.input_bg,
            "…and the far end of the groove is still empty"
        );

        // Half way, the handle has moved and the accent has travelled with it.
        let half = paint(50.0);
        let half_knob = SliderView::new(
            row,
            0.0,
            false,
            SliderSpec {
                value: 50.0,
                min: 0.0,
                max: 100.0,
                step: 1.0,
            },
        )
        .knob_rect();
        assert_eq!(
            half[mid_y * stride + (half_knob.x + half_knob.w / 2) as usize],
            t.button_text,
            "half way the handle has moved to the middle"
        );
        assert_ne!(
            half_knob.x, knob.x,
            "…which is the whole proof: the handle's position is the value"
        );
        assert_eq!(
            half[mid_y * stride + (track.x + 4) as usize],
            t.input_border_focused,
            "the travelled part is painted in the accent"
        );
        assert_eq!(
            half[mid_y * stride + (track.x + track.w - 8) as usize],
            t.input_bg,
            "the rest of the groove is not"
        );

        // And the two ends are never the same picture.
        assert_ne!(left, half, "0 and 50 are different drawings");
    }

    /// A disabled slider still draws — dimmed, so it reads as unavailable
    /// rather than absent — but nothing about it lights.
    #[test]
    fn test_a_disabled_slider_draws_dimmed() {
        let t = Theme::DARK;
        let stride = SLIDER_TRACK_W as usize + 8;
        let height = SLIDER_H as usize;
        let row = Rect {
            x: 0,
            y: 0,
            w: SLIDER_TRACK_W + 40,
            h: SLIDER_H,
        };
        let spec = SliderSpec {
            value: 50.0,
            min: 0.0,
            max: 100.0,
            step: 1.0,
        };

        let live = {
            let mut b = vec![t.bg; stride * height];
            let v = SliderView::new(row, 1.0, false, spec);
            draw_slider_control(&mut b, stride, height, None, &t, &v);
            b
        };
        let dim = {
            let mut b = vec![t.bg; stride * height];
            let v = SliderView::new(row, 0.0, true, spec);
            draw_slider_control(&mut b, stride, height, None, &t, &v);
            b
        };
        assert_ne!(
            live, dim,
            "a disabled slider is a visibly different drawing"
        );

        let track = slider_track_rect(row);
        let mid_y = (track.y + track.h / 2) as usize;
        let knob = SliderView::new(row, 0.0, true, spec).knob_rect();
        assert_ne!(
            dim[mid_y * stride + (knob.x + knob.w / 2) as usize],
            t.button_text,
            "the handle is not the bright white it is when live"
        );
        assert_ne!(
            dim[mid_y * stride + (track.x + 4) as usize],
            t.input_border_focused,
            "…and the travelled part is not the live accent either"
        );
    }

    /// A zero-width or zero-height track paints nothing rather than dividing
    /// by it: a control measured into nothing must not take the frame with it.
    #[test]
    fn test_an_empty_slider_track_paints_nothing() {
        let t = Theme::DARK;
        let stride = 8usize;
        let height = 8usize;
        let mut buf = vec![t.bg; stride * height];
        let view = SliderView {
            rect: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            hover_t: 0.0,
            disabled: false,
            spec: SliderSpec {
                value: 50.0,
                min: 0.0,
                max: 100.0,
                step: 1.0,
            },
            filled: 0,
        };
        draw_slider_control(&mut buf, stride, height, None, &t, &view);
        assert!(
            buf.iter().all(|p| *p == t.bg),
            "a track with no room paints nothing at all"
        );
    }

    /// The dispatch key: its `id` when it has one, layout order when it does
    /// not — so hover, the drag and the written-back value all resolve the
    /// same control.
    #[test]
    fn test_slider_key_falls_back_to_the_layout_order() {
        let with_id = slider_box(Some("temp"), "Temp", 0.0, 0.0, 100.0, 1.0, false);
        assert_eq!(slider_key(&with_id, 7), "temp");

        let mut no_id = with_id.clone();
        no_id.id = None;
        assert_eq!(slider_key(&no_id, 7), "__slider_7");
    }

    /// The track is one shape: flush with the row's left edge, vertically
    /// centred, and as long as the constant says — because the drawing, the
    /// press and the drag all read this one function.
    #[test]
    fn test_slider_track_rect_is_one_shape() {
        let row = Rect {
            x: 40,
            y: 90,
            w: 260,
            h: SLIDER_H,
        };
        let track = slider_track_rect(row);
        assert_eq!(track.x, 40, "flush with the row's left edge");
        assert_eq!(track.w, SLIDER_TRACK_W, "the length the mapping assumes");
        assert_eq!(track.h, SLIDER_TRACK_H, "and the groove's own height");
        assert_eq!(
            track.y,
            row.y + (row.h - SLIDER_TRACK_H) / 2,
            "vertically centred in its row"
        );

        // A row too short for the knob still puts the track somewhere
        // computable rather than above the row.
        let short = slider_track_rect(Rect {
            x: 0,
            y: 10,
            w: 100,
            h: 4,
        });
        assert!(short.y >= 10, "never above its own row: {short:?}");
    }

    /// The layout arm records the number the element carried, already snapped
    /// onto the control's own grid — so a starting value the program can never
    /// be handed is never the position the handle starts at either.
    #[test]
    fn test_the_layout_records_a_sliders_number_and_grid() {
        let lbox = slider_box(Some("temp"), "Temp", 33.0, 0.0, 100.0, 5.0, false);
        assert_eq!(lbox.tag, "slider");
        let spec = lbox.slider.expect("a slider carries its spec onto the box");
        assert_eq!(
            spec.value, 35.0,
            "33 is pulled onto the nearest rung of a five-wide step"
        );
        assert_eq!(spec.min, 0.0);
        assert_eq!(spec.max, 100.0);
        assert_eq!(
            spec.step, 5.0,
            "and the grid is the one the handler sees too"
        );

        // The row is the control: the track at the left edge, the label's
        // room beside it, one row tall.
        assert_eq!(lbox.rect.h, SLIDER_H);
        assert!(lbox.rect.w >= SLIDER_TRACK_W + FLAG_GAP);
        assert_eq!(
            lbox.text.as_deref(),
            Some("Temp"),
            "the label rides in `text`"
        );

        // A value already on a rung is left exactly where it is.
        let exact = slider_box(Some("t"), "T", 35.0, 0.0, 100.0, 5.0, false);
        assert_eq!(exact.slider.expect("spec").value, 35.0);

        // Out-of-range starts are clamped rather than drawn off the end.
        let over = slider_box(Some("t"), "T", 900.0, 0.0, 100.0, 5.0, false);
        assert_eq!(over.slider.expect("spec").value, 100.0);
        let under = slider_box(Some("t"), "T", -900.0, 0.0, 100.0, 5.0, false);
        assert_eq!(under.slider.expect("spec").value, 0.0);

        // A disabled slider still lays out and still records its number: it
        // only stops answering.
        let dim = slider_box(Some("t"), "T", 40.0, 0.0, 100.0, 10.0, true);
        assert!(dim.disabled, "the flag carries the dimmed state");
        assert_eq!(dim.slider.expect("spec").value, 40.0);

        // Every other tag has no slider at all, so nothing has to ask.
        let flag = flag_box("checkbox", Some("dark"), "Dark", false, false);
        assert!(flag.slider.is_none());
    }

    /// The row's whole width is the control — track *and* label — because
    /// pressing the words of a slider that does nothing is text people learn
    /// to avoid, exactly as with the flags.
    #[test]
    fn test_the_whole_slider_row_is_pressable() {
        let lbox = slider_box(
            Some("temp"),
            "A very long label here",
            0.0,
            0.0,
            100.0,
            1.0,
            false,
        );
        let label_x = lbox.rect.x + SLIDER_TRACK_W + FLAG_GAP;
        assert!(
            label_x < lbox.rect.x + lbox.rect.w,
            "the label has room beside the track"
        );
        // A press on the label is still a press on the control: the box's own
        // rect covers it, which is what `slider_at` hit-tests against.
        assert!(
            lbox.rect
                .contains(label_x + 2, lbox.rect.y + lbox.rect.h / 2),
            "…and the row's rect covers the label"
        );
    }

    /// The label takes the press but not the value. `x_to_value` clamps at
    /// both ends, so a caption click reaching it would throw a 500 to the
    /// maximum — a number nobody asked for by reading some words.
    #[test]
    fn test_pressing_the_label_absorbs_the_press_and_moves_nothing() {
        let lbox = slider_box(
            Some("temp"),
            "A very long label here",
            50.0,
            0.0,
            100.0,
            1.0,
            false,
        );
        // The label starts just past the groove, with the row's own margin
        // after it — all of it part of the pressable row.
        let label_x = lbox.rect.x + SLIDER_TRACK_W + FLAG_GAP + 4;
        assert!(
            lbox.rect.contains(label_x, lbox.rect.y + lbox.rect.h / 2),
            "the label really is inside the row"
        );

        let mut id = String::new();
        let mut value = String::new();
        let mut clicks = 0;
        let mut app = flag_app(&mut id, &mut value, &mut clicks);
        app.layout_boxes = vec![lbox.clone()];

        // Nothing behind the row can be fired through it…
        let behind = btn_box_at(lbox.rect);
        app.layout_boxes = vec![lbox.clone(), behind.clone()];
        app.cursor_pos = (label_x, lbox.rect.y + lbox.rect.h / 2);
        let taken = app.begin_slider_drag(app.cursor_pos);
        assert!(taken, "the row takes the press either way");
        assert_eq!(
            *app.dispatcher.clicks, 0,
            "…but the button behind still cannot be reached"
        );
        assert!(
            app.dispatcher.value.is_empty(),
            "and the label reports no number, because none was picked"
        );
        assert!(
            app.slider_drag.is_none(),
            "…so there is no drag to carry on with"
        );

        // While the same press on the groove does the obvious thing.
        let mut id2 = String::new();
        let mut value2 = String::new();
        let mut clicks2 = 0;
        let mut dial = flag_app(&mut id2, &mut value2, &mut clicks2);
        dial.layout_boxes = vec![lbox.clone()];
        dial.cursor_pos = (
            lbox.rect.x + SLIDER_TRACK_W - 2,
            lbox.rect.y + lbox.rect.h / 2,
        );
        assert!(dial.begin_slider_drag(dial.cursor_pos));
        assert_eq!(
            *dial.dispatcher.value, "100",
            "the right end of the groove is still the dial"
        );
        assert!(dial.slider_drag.is_some(), "…and it begins a drag");
    }

    /// The boundary between "the dial" and "the words beside it" is one
    /// pixel wide and has to fall in the gap, or the two readings collide:
    /// everything up to the groove's right stop plus the handle's own
    /// overhang is the dial, and the label's very first pixel is beyond it.
    #[test]
    fn test_the_groove_ends_before_the_label_begins() {
        let lbox = slider_box(Some("t"), "Label", 0.0, 0.0, 100.0, 1.0, false);
        let track = slider_track_rect(lbox.rect);
        let overhang = SLIDER_KNOB / 2;
        let dial_end = track.x + track.w + overhang;
        let label_x = track.x + track.w + FLAG_GAP;

        // The track is flush with the row's left edge, so the row's own edge
        // *is* the left stop — there is nothing further left to press, and
        // nothing there could be mistaken for the label.
        assert_eq!(track.x, lbox.rect.x, "the groove starts the row");
        assert!(
            lbox.rect.contains(track.x, track.y),
            "…so the left stop is in it"
        );

        // The far end: still the dial up to and including the overhang…
        assert!(lbox.rect.contains(dial_end, track.y));
        // …and the label begins on the very next pixel, inside the row but
        // past the dial's reach. A one-pixel margin is enough precisely
        // because both are integers — no pointer position is between them.
        assert_eq!(label_x, dial_end + 1, "the gap leaves exactly one pixel");
        assert!(
            lbox.rect.contains(label_x, track.y),
            "…and the label is in the row"
        );
    }

    #[test]
    fn test_normalize_rich_text_paste() {
        let input = "<b>Bold</b> and <i>italic</i> and <code>code</code><br/>Next line";
        let out = normalize_rich_text_paste(input);
        assert_eq!(out, "**Bold** and *italic* and `code`\nNext line");

        let p_input = "<p>First paragraph</p><p>Second paragraph</p>";
        let out_p = normalize_rich_text_paste(p_input);
        assert_eq!(out_p, "First paragraph\nSecond paragraph\n");
    }

    #[test]
    fn test_toast_notification_struct() {
        let t = ToastNotification {
            message: "Copied!".into(),
            status: "success".into(),
        };
        assert_eq!(t.message, "Copied!");
        assert_eq!(t.status, "success");
    }

    #[test]
    fn test_wcag_relative_luminance_and_contrast_ratio() {
        let black = 0xFF00_0000;
        let white = 0xFFFF_FFFF;

        assert!((relative_luminance(black) - 0.0).abs() < 1e-4);
        assert!((relative_luminance(white) - 1.0).abs() < 1e-4);

        let cr_bw = contrast_ratio(white, black);
        assert!((cr_bw - 21.0).abs() < 1e-3);

        // Symmetry
        assert_eq!(contrast_ratio(white, black), contrast_ratio(black, white));

        // Self-contrast is 1.0
        assert!((contrast_ratio(0xFF3B_82F6, 0xFF3B_82F6) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_wcag_theme_contrast_compliance() {
        // Dark theme: Zinc-100 text on Zinc-900 bg
        let dark_text_cr = contrast_ratio(Theme::DARK.text, Theme::DARK.bg);
        assert!(
            dark_text_cr >= 7.0,
            "Dark theme text contrast ({dark_text_cr}) must satisfy WCAG AAA (>= 7.0:1)"
        );
        assert!(wcag_aaa_compliant(Theme::DARK.text, Theme::DARK.bg, false));

        // Light theme: Zinc-900 text on Zinc-50 bg
        let light_text_cr = contrast_ratio(Theme::LIGHT.text, Theme::LIGHT.bg);
        assert!(
            light_text_cr >= 7.0,
            "Light theme text contrast ({light_text_cr}) must satisfy WCAG AAA (>= 7.0:1)"
        );
        assert!(wcag_aaa_compliant(
            Theme::LIGHT.text,
            Theme::LIGHT.bg,
            false
        ));

        // Button contrast for UI elements (>= 3.0:1)
        assert!(wcag_aa_compliant(
            Theme::DARK.button_text,
            Theme::DARK.button_bg,
            true
        ));
        assert!(wcag_aa_compliant(
            Theme::LIGHT.button_text,
            Theme::LIGHT.button_bg,
            true
        ));
    }

    #[test]
    fn test_focus_ring_rendering() {
        let stride = 50;
        let height = 50;
        let mut buf = vec![0u32; stride * height];
        let ring_col = 0xFF3B_82F6;

        draw_focus_ring(&mut buf, stride, height, None, 15, 15, 20, 20, 4, ring_col);

        // Outside rectangle middle left edge (at (13, 25) which is offset by 2px from x=15) should have ring color
        let px = buf[25 * stride + 13];
        assert_ne!(px, 0, "Focus ring outer boundary should be drawn");
        assert_eq!(px, ring_col, "Focus ring pixel should match ring color");

        // Deep interior (e.g. at (25, 25)) should not be overwritten
        let interior = buf[25 * stride + 25];
        assert_eq!(
            interior, 0,
            "Focus ring must not draw over component interior"
        );
    }

    #[test]
    fn test_accessible_tree_generation() {
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
                title: "A11y Test Window".into(),
                width: 600,
                height: 400,
            },
            dummy,
        );

        app.layout_boxes = vec![
            LayoutBox {
                rect: Rect {
                    x: 10,
                    y: 10,
                    w: 120,
                    h: 36,
                },
                tag: "button".into(),
                id: Some("send_btn".into()),
                text: Some("Send".into()),
                on_click: Some("send_click".into()),
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 0,
                disabled: false,
                checked: false,
                slider: None,
                options: vec![],
                tooltip: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            },
            LayoutBox {
                rect: Rect {
                    x: 10,
                    y: 60,
                    w: 200,
                    h: 36,
                },
                tag: "input".into(),
                id: Some("msg_input".into()),
                text: Some("hello world".into()),
                on_click: None,
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 0,
                disabled: false,
                checked: false,
                slider: None,
                options: vec![],
                tooltip: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            },
            LayoutBox {
                rect: Rect {
                    x: 10,
                    y: 110,
                    w: 100,
                    h: 24,
                },
                tag: "checkbox".into(),
                id: Some("agree_chk".into()),
                text: Some("Agree".into()),
                on_click: None,
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 0,
                disabled: false,
                checked: true,
                slider: None,
                options: vec![],
                tooltip: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            },
        ];

        let tree = app.build_accessible_tree();
        assert_eq!(tree.role, AccessibleRole::Window);
        assert_eq!(tree.name, "A11y Test Window");
        assert_eq!(tree.children.len(), 3);

        let btn = &tree.children[0];
        assert_eq!(btn.role, AccessibleRole::Button);
        assert_eq!(btn.name, "Send");
        assert_eq!(btn.id.as_deref(), Some("send_btn"));
        assert!(!btn.disabled);

        let input = &tree.children[1];
        assert_eq!(input.role, AccessibleRole::TextInput);
        assert_eq!(input.value.as_deref(), Some("hello world"));

        let chk = &tree.children[2];
        assert_eq!(chk.role, AccessibleRole::Checkbox);
        assert_eq!(chk.checked, Some(true));
    }

    #[test]
    fn test_keyboard_accessibility_focus_and_controls() {
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
                title: "Focus Test Window".into(),
                width: 600,
                height: 400,
            },
            dummy,
        );

        app.layout_boxes = vec![
            LayoutBox {
                rect: Rect {
                    x: 10,
                    y: 10,
                    w: 120,
                    h: 36,
                },
                tag: "button".into(),
                id: Some("btn1".into()),
                text: Some("Button 1".into()),
                on_click: Some("btn1_click".into()),
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 0,
                disabled: false,
                checked: false,
                slider: None,
                options: vec![],
                tooltip: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            },
            LayoutBox {
                rect: Rect {
                    x: 10,
                    y: 60,
                    w: 200,
                    h: 36,
                },
                tag: "input".into(),
                id: Some("input1".into()),
                text: Some("Input text".into()),
                on_click: None,
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 0,
                disabled: false,
                checked: false,
                slider: None,
                options: vec![],
                tooltip: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            },
            LayoutBox {
                rect: Rect {
                    x: 10,
                    y: 110,
                    w: 100,
                    h: 24,
                },
                tag: "checkbox".into(),
                id: Some("chk1".into()),
                text: Some("Checkbox 1".into()),
                on_click: None,
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 0,
                disabled: false,
                checked: false,
                slider: None,
                options: vec![],
                tooltip: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            },
            LayoutBox {
                rect: Rect {
                    x: 10,
                    y: 150,
                    w: 160,
                    h: 24,
                },
                tag: "slider".into(),
                id: Some("slider1".into()),
                text: Some("Slider 1".into()),
                on_click: None,
                on_change: None,
                on_key: None,
                pan: (0, 0),
                clips: None,
                pad: 0,
                disabled: false,
                checked: false,
                slider: Some(SliderSpec {
                    value: 50.0,
                    min: 0.0,
                    max: 100.0,
                    step: 5.0,
                }),
                options: vec![],
                tooltip: None,
                draggable: false,
                props: std::collections::HashMap::new(),
            },
        ];

        // 1. Initial state: unfocused
        assert_eq!(app.focused_id, None);

        // 2. Tab -> focuses first element ("btn1")
        app.handle_key_input(&Key::Named(NamedKey::Tab), None);
        assert_eq!(app.focused_id.as_deref(), Some("btn1"));

        // 3. Tab -> focuses second element ("input1")
        app.handle_key_input(&Key::Named(NamedKey::Tab), None);
        assert_eq!(app.focused_id.as_deref(), Some("input1"));

        // 4. Tab -> focuses third element ("chk1")
        app.handle_key_input(&Key::Named(NamedKey::Tab), None);
        assert_eq!(app.focused_id.as_deref(), Some("chk1"));

        // 5. Space on checkbox -> toggles and dispatches change "true"
        app.handle_key_input(&Key::Named(NamedKey::Space), None);
        assert_eq!(*app.dispatcher.notes, "true");

        // 6. Tab -> focuses fourth element ("slider1")
        app.handle_key_input(&Key::Named(NamedKey::Tab), None);
        assert_eq!(app.focused_id.as_deref(), Some("slider1"));

        // 7. ArrowRight on slider -> increases by step (50 -> 55) and dispatches change "55"
        app.handle_key_input(&Key::Named(NamedKey::ArrowRight), None);
        assert_eq!(*app.dispatcher.notes, "55");

        // 8. Shift+Tab -> moves backward to "chk1"
        app.shift_pressed = true;
        app.handle_key_input(&Key::Named(NamedKey::Tab), None);
        assert_eq!(app.focused_id.as_deref(), Some("chk1"));
        app.shift_pressed = false;

        // 9. Escape -> clears focus
        app.handle_key_input(&Key::Named(NamedKey::Escape), None);
        assert_eq!(app.focused_id, None);
    }

    #[test]
    fn test_zoom_animation_convergence() {
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
                title: "Zoom Test".into(),
                width: 500,
                height: 400,
            },
            dummy,
        );

        assert_eq!(app.current_zoom_pct, 100.0);
        app.set_zoom_target(150.0);
        assert_eq!(app.target_zoom_pct, 150.0);

        for _ in 0..10 {
            app.step_zoom_animation(0.5);
        }
        assert_eq!(app.current_zoom_pct, 150.0);
    }

    #[test]
    fn test_error_banner_lifecycle_retry_and_dismissal() {
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
                title: "Error Test".into(),
                width: 500,
                height: 400,
            },
            dummy,
        );

        // 1. Initial state has no error
        assert!(app.current_error.is_none());

        // 2. Set retryable network error
        app.set_error(
            "Connection timeout to LLM provider",
            "timeout",
            true,
            Some("retry_send".into()),
        );
        assert!(app.current_error.is_some());
        let err = app.current_error.as_ref().unwrap();
        assert_eq!(err.error_type, "timeout");
        assert!(err.retryable);
        assert_eq!(*app.dispatcher.last_key, "ErrorShown:timeout");
        assert_eq!(*app.dispatcher.notes, "timeout");

        // 3. Trigger retry
        app.trigger_error_retry();
        assert!(app.current_error.is_none());
        assert_eq!(*app.dispatcher.last_key, "Action:retry_send");
        assert_eq!(*app.dispatcher.notes, "retry");

        // 4. Set non-retryable error and dismiss
        app.set_error("Schema validation failed", "schema", false, None);
        assert!(app.current_error.is_some());
        app.clear_error();
        assert!(app.current_error.is_none());
        assert_eq!(*app.dispatcher.last_key, "ErrorDismissed");
        assert_eq!(*app.dispatcher.notes, "none");
    }

    #[test]
    fn test_draft_recovery_persistence() {
        let mut val = 0;
        let mut notes = String::new();
        let mut key = String::new();
        let dummy = DummyDisp {
            val: &mut val,
            notes: &mut notes,
            last_key: &mut key,
        };
        let app = NativeApp::new(
            WindowConfig {
                title: "Draft Test".into(),
                width: 500,
                height: 400,
            },
            dummy,
        );

        let input_id = "test_draft_input";
        let draft_text = "Important unsaved message draft";

        // Save draft
        app.save_draft_recovery(input_id, draft_text);

        // Load draft
        let loaded = app.load_draft_recovery(input_id);
        assert_eq!(loaded.as_deref(), Some(draft_text));

        // Clean up
        let _ = std::fs::remove_file(
            std::env::temp_dir()
                .join("nudge_drafts")
                .join(format!("draft_{input_id}.json")),
        );
    }

    #[test]
    fn test_async_llm_execution_chunks_and_cancellation() {
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
                title: "Async Test".into(),
                width: 500,
                height: 400,
            },
            dummy,
        );

        // 1. Initial state not in flight
        assert!(!app.async_state.in_flight);

        // 2. Start async task
        app.start_async_task("task_42", "Generating response...");
        assert!(app.async_state.in_flight);
        assert_eq!(app.async_state.task_id.as_deref(), Some("task_42"));
        assert_eq!(*app.dispatcher.last_key, "TaskStarted:task_42");
        assert_eq!(*app.dispatcher.notes, "task_42");

        // 3. Receive streaming chunks
        app.update_async_chunk("Hello ");
        app.update_async_chunk("world!");
        assert_eq!(app.async_state.chunks_received, 2);
        assert_eq!(*app.dispatcher.notes, "world!");

        // 4. Cancel task
        app.cancel_async_task();
        assert!(!app.async_state.in_flight);
        assert_eq!(*app.dispatcher.last_key, "TaskCancelled:task_42");
        assert_eq!(*app.dispatcher.notes, "task_42");
        assert!(app.toast.is_some());
    }

    #[test]
    fn test_window_state_persistence_and_sidebar_toggle() {
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
                title: "Persistence Test".into(),
                width: 640,
                height: 480,
            },
            dummy,
        );

        // Toggle sidebar
        assert!(app.sidebar_open);
        app.toggle_sidebar();
        assert!(!app.sidebar_open);
        assert_eq!(*app.dispatcher.last_key, "SidebarToggled:false");

        // Save window state
        let dir = std::env::temp_dir().join("nudge_state_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_window_state.json");
        let path_str = path.to_str().unwrap();
        app.save_window_state(path_str).expect("save state");

        // Modify app state
        app.width = 300;
        app.height = 250;
        app.sidebar_open = true;

        // Load window state back
        let loaded = app.load_window_state(path_str).expect("load state");
        assert_eq!(loaded.width, 640);
        assert_eq!(loaded.height, 480);
        assert!(!loaded.sidebar_open);
        assert_eq!(app.width, 640);
        assert_eq!(app.height, 480);
        assert!(!app.sidebar_open);

        // Clean up
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_partial_redraw_dirty_rect_tracking() {
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
                title: "Dirty Rect Test".into(),
                width: 500,
                height: 400,
            },
            dummy,
        );

        assert_eq!(app.dirty_rect, None);

        // Mark first rect dirty
        let r1 = Rect {
            x: 10,
            y: 10,
            w: 100,
            h: 50,
        };
        app.mark_dirty(r1);
        assert_eq!(app.dirty_rect, Some(r1));

        // Mark second rect dirty, should union
        let r2 = Rect {
            x: 50,
            y: 30,
            w: 120,
            h: 80,
        };
        app.mark_dirty(r2);
        let union_rect = app.dirty_rect.unwrap();
        assert_eq!(union_rect.x, 10);
        assert_eq!(union_rect.y, 10);
        assert_eq!(union_rect.w, 160); // (50 + 120) - 10
        assert_eq!(union_rect.h, 100); // (30 + 80) - 10

        // Clear dirty
        app.clear_dirty();
        assert_eq!(app.dirty_rect, None);
    }

    #[test]
    fn test_native_app_hud_overlay_mode() {
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
                title: "HUD Test".into(),
                width: 800,
                height: 600,
            },
            dummy,
        );
        assert!(!app.hud_mode);

        app.hud_mode = true;
        let stride = 800usize;
        let height = 600usize;
        let mut buffer = vec![0xFF12_1826u32; stride * height];

        let _ = crate::jarvis_hud::with_hud(|h| {
            h.push_ticker("Testing HUD compositing on native app surface.");
            h.set_pip("web", true, "Test Viewport", Some("Button"));
            h.render_to_u32_buffer(&mut buffer, stride, height);
            Ok(())
        });

        let modified_count = buffer.iter().filter(|&&px| px != 0xFF12_1826).count();
        assert!(
            modified_count > 0,
            "HUD overlay modified pixels on the buffer"
        );
    }
}
