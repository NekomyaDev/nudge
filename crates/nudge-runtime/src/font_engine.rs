//! TrueType font engine for the native GUI, backed by `ab_glyph`.
//!
//! Why this exists: the old renderer used a fixed 8x8 bitmap font with a
//! hard-coded advance of `8 * scale` px. That looked blocky and could not be
//! anti-aliased. This module rasterizes real TTF outlines (DejaVu Sans for UI,
//! Hack for code) with grayscale anti-aliasing.
//!
//! Layout compatibility: to keep click/selection/cursor math working without
//! rewriting all of it, text is laid out on a *fixed monospace advance*
//! (`cell_w` x `cell_h`), exactly like the old bitmap font. Proportional
//! metrics are measured separately (`measure_text`) so container widths are
//! still tight. Cursor mapping therefore uses the same fixed cell grid the
//! layout engine uses, while glyphs themselves are drawn with real outlines.
#![cfg(feature = "native-gui")]

use ab_glyph::{point, Font, FontVec, PxScale, ScaleFont};

/// Embedded UI font (proportional, full Turkish support).
const UI_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/DejaVuSans.ttf");
/// Embedded UI font, bold (titles, emphasis).
const UI_FONT_BOLD_BYTES: &[u8] = include_bytes!("../../../assets/fonts/DejaVuSans-Bold.ttf");
/// Embedded monospace font for code / terminal-like text.
const MONO_FONT_BYTES: &[u8] = include_bytes!("../../../assets/fonts/Hack-Regular.ttf");

/// Base UI font size in px.
pub const BASE_PX: f32 = 16.0;
/// Fixed layout cell width at [`BASE_PX`] (px). Layout, cursor and click
/// mapping all share this grid so they stay in sync.
pub const CELL_W: i32 = 10;
/// Fixed layout cell height at [`BASE_PX`] (px).
pub const CELL_H: i32 = 20;

/// A loaded font face at a given pixel size, with cached vertical metrics.
struct Face {
    font: FontVec,
    px: f32,
    ascent: f32,
    descent: f32,
    line_gap: f32,
}

impl Face {
    fn new(bytes: &'static [u8], px: f32) -> Option<Self> {
        let font = FontVec::try_from_vec(bytes.to_vec()).ok()?;
        let sf = font.as_scaled(PxScale::from(px));
        Some(Self {
            ascent: sf.ascent(),
            descent: sf.descent(), // negative
            line_gap: sf.line_gap(),
            font,
            px,
        })
    }
}

/// Which semantic face to use for a run of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceKind {
    /// Default proportional UI text.
    Ui,
    /// Bold proportional text.
    UiBold,
    /// Monospace text (code).
    Mono,
}

/// Central font engine: owns every face at a base size and exposes the
/// measurement + rasterization primitives the renderer and layout need.
pub struct FontEngine {
    base_px: f32,
    ui: Face,
    ui_bold: Face,
    mono: Face,
    /// Fixed monospace cell width used for layout/cursor math (px).
    pub cell_w: i32,
    /// Fixed line height used for layout/cursor math (px).
    pub cell_h: i32,
}

impl FontEngine {
    /// Build the engine at `base_px`. Falls back to [`BASE_PX`] on failure.
    /// Cell metrics scale linearly with the base size so cursor/click mapping
    /// stays consistent at any size.
    pub fn new(base_px: f32) -> Self {
        let px = if base_px.is_finite() && base_px >= 6.0 {
            base_px
        } else {
            BASE_PX
        };
        // Fixed cell grid: keeps click <-> index math on an integer grid, the
        // same contract the old 8x8 bitmap renderer relied on.
        let factor = px / BASE_PX;
        let cell_w = ((CELL_W as f32) * factor).round().max(4.0) as i32;
        let cell_h = ((CELL_H as f32) * factor).round().max(8.0) as i32;

        let ui = Face::new(UI_FONT_BYTES, px).expect("embedded UI font must parse");
        let ui_bold = Face::new(UI_FONT_BOLD_BYTES, px).expect("embedded bold font must parse");
        let mono = Face::new(MONO_FONT_BYTES, px).expect("embedded mono font must parse");

        Self {
            base_px: px,
            ui,
            ui_bold,
            mono,
            cell_w,
            cell_h,
        }
    }

    pub fn base_px(&self) -> f32 {
        self.base_px
    }

    fn face(&self, kind: FaceKind) -> &Face {
        match kind {
            FaceKind::Ui => &self.ui,
            FaceKind::UiBold => &self.ui_bold,
            FaceKind::Mono => &self.mono,
        }
    }

    /// Baseline offset from the top of a line box to the text baseline (px).
    pub fn baseline_offset(&self, kind: FaceKind) -> i32 {
        self.face(kind).ascent.round() as i32
    }

    /// Full line box height (px) for a face.
    pub fn line_height(&self, kind: FaceKind) -> i32 {
        let f = self.face(kind);
        (f.ascent - f.descent + f.line_gap).round() as i32
    }

    /// Proportional advance width of `text` in px (tight, kerned).
    pub fn measure_width(&self, kind: FaceKind, text: &str) -> i32 {
        let f = self.face(kind);
        let sf = f.font.as_scaled(PxScale::from(f.px));
        let mut width = 0.0f32;
        let mut prev: Option<ab_glyph::GlyphId> = None;
        for ch in text.chars() {
            if ch == '\n' || ch == '\r' {
                continue;
            }
            let id = sf.glyph_id(ch);
            if let Some(p) = prev {
                width += sf.kern(p, id);
            }
            width += sf.h_advance(id);
            prev = Some(id);
        }
        width.round() as i32
    }

    /// Proportional advance width of a single `ch` in px (kerning-free).
    pub fn advance_of(&self, kind: FaceKind, ch: char) -> i32 {
        let f = self.face(kind);
        let sf = f.font.as_scaled(PxScale::from(f.px));
        sf.h_advance(sf.glyph_id(ch)).round() as i32
    }

    /// Pixel x-offset of the first `col` glyphs of `line`.
    ///
    /// This is the **caret/selection/click metric**: it sums the exact same
    /// kerning-free `advance_of` values that the free function
    /// `draw_text_ttf` uses to place glyphs, so a caret computed from it
    /// lands exactly on the drawn glyph boundary. Using the fixed `cell_w`
    /// grid instead would drift from the rendered text (e.g. "olur" is 27px
    /// drawn but 4*cell_w = 40px on the grid, leaving a phantom gap).
    pub fn prefix_width(&self, kind: FaceKind, line: &str, col: usize) -> i32 {
        line.chars()
            .take(col)
            .filter(|c| *c != '\n' && *c != '\r')
            .map(|c| self.advance_of(kind, c))
            .sum()
    }

    /// Inverse of [`FontEngine::prefix_width`]: the column inside `line` whose
    /// glyph starts at or before pixel offset `x`. Ties (pointer past the end
    /// of the line) clamp to the line length, so clicking after the last
    /// character puts the caret at the end.
    pub fn col_at_offset(&self, kind: FaceKind, line: &str, x: i32) -> usize {
        let mut rem = x.max(0);
        let mut col = 0usize;
        for ch in line.chars() {
            if ch == '\n' {
                break;
            }
            if ch == '\r' {
                // `draw_text_ttf` skips CR without advancing: zero-width, but it
                // still occupies a column index.
                col += 1;
                continue;
            }
            let adv = self.advance_of(kind, ch);
            if rem < adv {
                return col;
            }
            rem -= adv;
            col += 1;
        }
        col
    }

    /// Rasterize one glyph's anti-aliased coverage into `buf`.
    ///
    /// `draw_px(x, y, w, h, color, coverage)` receives each covered pixel with
    /// a `coverage` in `[0.0, 1.0]` (grayscale anti-aliasing). The caller
    /// blends it into the framebuffer.
    pub fn draw_glyph<F>(&self, kind: FaceKind, ch: char, x: i32, y: i32, mut draw_px: F)
    where
        F: FnMut(i32, i32, i32, i32, f32),
    {
        if ch == '\n' || ch == '\r' || ch == '\t' {
            return;
        }
        let f = self.face(kind);
        let sf = f.font.as_scaled(PxScale::from(f.px));
        let id = sf.glyph_id(ch);
        // Position by baseline: y is the *top* of the line box.
        let baseline = y + sf.ascent().round() as i32;
        let glyph = ab_glyph::Glyph {
            id,
            scale: PxScale::from(f.px),
            position: point(x as f32, baseline as f32),
        };
        if let Some(outlined) = f.font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, coverage| {
                if coverage <= 0.0 {
                    return;
                }
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                draw_px(px, py, 1, 1, coverage.clamp(0.0, 1.0));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> FontEngine {
        FontEngine::new(BASE_PX)
    }

    /// The caret metric must be exactly the sum of the advances used to draw,
    /// otherwise the caret drifts off the rendered glyphs.
    #[test]
    fn prefix_width_equals_draw_advance_sum() {
        let e = engine();
        let line = "olur, merhaba!";
        let expected: i32 = line.chars().map(|c| e.advance_of(FaceKind::Ui, c)).sum();
        assert_eq!(
            e.prefix_width(FaceKind::Ui, line, line.chars().count()),
            expected,
            "full-line prefix must equal the drawn advance sum"
        );
    }

    /// Regression: on the old fixed grid the caret for "olur" sat at
    /// 4*CELL_W = 40px while the glyphs end at 27px, a phantom ~13px gap.
    #[test]
    fn caret_metric_is_not_the_fixed_grid() {
        let e = engine();
        let line = "olur";
        let drawn = e.prefix_width(FaceKind::Ui, line, line.chars().count());
        let grid = (line.chars().count() as i32) * CELL_W;
        assert!(
            drawn < grid,
            "proportional width ({drawn}px) must be tighter than the grid ({grid}px)"
        );
        // Sanity: the drawn width is in a plausible range for 16px DejaVu.
        assert!((15..=40).contains(&drawn), "unexpected width {drawn}px");
    }

    /// col_at_offset must invert prefix_width for every column, so clicking
    /// where a glyph is drawn puts the caret on that glyph.
    #[test]
    fn col_at_offset_round_trips_every_column() {
        let e = engine();
        for line in ["olur", "merhaba", "hi", "Nudge Studio", "ğüşıöç", ""] {
            let cols = line.chars().count();
            for col in 0..=cols {
                let x = e.prefix_width(FaceKind::Ui, line, col);
                assert_eq!(
                    e.col_at_offset(FaceKind::Ui, line, x),
                    col,
                    "round-trip failed for {line:?} col {col}"
                );
            }
            // Clicking well past the end clamps to the line length.
            let past = e.prefix_width(FaceKind::Ui, line, cols) + 500;
            assert_eq!(e.col_at_offset(FaceKind::Ui, line, past), cols);
            // Clicking before the start clamps to column 0.
            assert_eq!(e.col_at_offset(FaceKind::Ui, line, -5), 0);
        }
    }

    /// CR is drawn without advancing (zero width) and must not skew the metric.
    #[test]
    fn cr_is_zero_width_in_caret_metric() {
        let e = engine();
        // "a\rb" occupies 3 columns but draws exactly as wide as "ab".
        assert_eq!(
            e.prefix_width(FaceKind::Ui, "a\rb", 3),
            e.prefix_width(FaceKind::Ui, "ab", 2)
        );
        assert_eq!(e.prefix_width(FaceKind::Ui, "\r\r", 2), 0);
    }
}
