# Nudge Studio — Rich UI Roadmap (Inventory & Backup)

> Goal: evolve the existing native renderer (`winit 0.30` + `softbuffer 0.4`) toward
> a modern, "web-app feel" interface.
> This document lists every missing item **evidence-based** (derived from the code).
> Date: 21 Oct 2026 · Repo: NekomyaDev/nudge

---

## SECTION 0 — CURRENT STATE (evidence)

### Technology stack (verified in code)
- Window / events: `winit 0.30` (Wayland + X11) — `ui_window.rs:9-14`
- Presentation: `softbuffer 0.4` (software rasterize, CPU) — `ui_window.rs:1,1283-1288`
- Font: `ab_glyph` TrueType engine (`font_engine.rs`), embedded DejaVu Sans + Hack
- Drawing primitives: `draw_rect`, `draw_rect_alpha`, `draw_rect_outline`, `draw_char_ttf`, `draw_text_ttf`
- Framebuffer: `u32` (ARGB), `stride`-based, with alpha compositing

### Color palette (centralized in `Theme`)
| Use | Value |
|---|---|
| Background (Zinc-900) | `0xFF18181B` |
| Text (Zinc-100) | `0xFFF4F4F5` |
| Button normal (Blue-500) | `0xFF3B82F6` |
| Button hover (Blue-600) | `0xFF2563EB` |
| Button border (Blue-400) | `0xFF60A5FA` |
| Input fill (Zinc-800) | `0xFF27272A` |
| Input border focused (Blue-500) | `0xFF3B82F6` |
| Input border unfocused (Zinc-600) | `0xFF52525B` |
| Selection highlight (Blue-700) | `0xFF1D4ED8` |
| Cursor (white) | `0xFFFFFFFF` |
| Placeholder (Zinc-500) | `0xFF71717A` |
| Box text | `0xFFE4E4E7` |

### Capabilities present
- [x] Horizontal / vertical layout: `row`, `column` (padding / gap supported)
- [x] Text, button, input (3 base widgets + 2 containers = 5 total)
- [x] Button hover state (2 tones)
- [x] Input focus state, cursor, selection highlight, placeholder
- [x] Text wrapping (`wrap_text`, automatic line breaks)
- [x] Global window scrolling (`scroll_x/scroll_y`, MouseScrollDelta — 1347)
- [x] Viewport clipping (out-of-bounds elements are not drawn — 628)
- [x] Event routing: click→id, change→id, key→id
- [x] Headless test infrastructure (`mount_headless`, dispatch_*)

### Remaining renderer soft spots
- No kerning-aware proportional layout for **container width** (uses fixed `CELL_W` grid)
- No rounded corners, shadows, gradients, or icons yet
- Single font size tier in use (`BASE_PX`); fractional sizes not yet wired to UI
- No animation / timer infrastructure (needed for cursor blink and spinners)

---

## SECTION 1 — RENDERER VISUAL FOUNDATIONS (base layer)
> Nothing in the later layers looks right without this. DO THESE FIRST.
> Estimated total: 8–12 days

### 1.1 Alpha blending / translucency  (1 day) — DONE
- [x] `draw_rect_alpha` + `blend_px`: `dst = src*a + dst*(1-a)`
- [x] Required for overlays, shadows, and modal backdrop dimming
- Note: feasible because the buffer format is ARGB

### 1.2 Real font rasterization (TTF/OTF)  (2–3 days) ★ biggest visual gain — DONE
- [x] Font crate added: `ab_glyph` (lightweight)
- [x] Font files embedded as binary (`include_bytes!`) — DejaVu Sans, DejaVu Sans Bold, Hack
- [x] Scalable size (8, 10, 12, 14, 16, 20, 24...)
- [x] Anti-aliasing (grayscale coverage)
- [x] UTF-8 / Turkish character support (`ğüşıöç` render correctly)
- [x] Measurement: `measure_width` for correct wrapping and vertical alignment
- [x] Proportional glyph advances (natural spacing)
- [x] Caret / selection / click use the **same** proportional metric as drawing
      (`prefix_width` / `col_at_offset`). The old fixed `cell_w` grid put the
      caret ~13px past the glyphs for "olur" — a phantom gap while typing.
- [x] Inline style: **bold** (`FaceKind::UiBold`)
- Note: `draw_text` call sites routed through `draw_text_ttf`

### 1.3 Rounded corners (rounded rect)  (1 day)
- [ ] `draw_rounded_rect`: radius + border
- [ ] Used by buttons, inputs, cards, modals

### 1.4 Drop shadow  (1 day)
- [ ] Soft shadow (multi-layer alpha instead of blur)
- [ ] Depth for cards and modals

### 1.5 Gradient  (0.5 day)
- [ ] Vertical / horizontal linear gradient (buttons, headers, selected items)

### 1.6 Cursor & mouse state  (1 day)
- [ ] Text cursor blink animation (requires timer)
- [ ] Mouse cursor shape changes (pointer/text/wait) — winit `set_cursor`
- [ ] Button `active` (pressed) third tone; disabled (dimmed) state
- [ ] Tooltip (small label on hover)

### 1.7 Timer / animation infrastructure  (1–2 days)
- [ ] Frame clock / `request_redraw` scheduling for redraws
- [ ] Hover transition smoothing (color interpolation)
- [ ] Foundation for spinner animation

### 1.8 Color / theme system  (1 day) — DONE
- [x] Colors moved into `struct Theme`
- [x] Widgets access colors through the theme reference
- [ ] Light / dark theme toggle (structure ready, palette entry pending)

### 1.9 Scrollbar  (1 day)
- [ ] Vertical + horizontal scrollbar drawing (thumb + track)
- [ ] Drag-to-scroll, hover highlight
- [ ] Integration with existing global `scroll_y`

---

## SECTION 2 — MISSING WIDGETS
> Each must work on 3 layers: Nudge language syntax + compiler (AST/types) + renderer.
> Estimate: simple 0.5 day, medium 1 day, complex 2 days

### 2.1 `scroll` (nested scroll area)  (2 days) ★ critical
- [ ] Make a specific region scrollable (currently only global window scroll exists)
- [ ] Required for the chat feed; replaces the paging workaround
- [ ] Content height measurement + scroll position + arrow keys

### 2.2 `select` / dropdown  (1 day)
- [ ] For model selection, settings menus
- [ ] Dropdown list, selected item, keyboard navigation

### 2.3 `checkbox` / `toggle`  (0.5 day)
- [ ] Settings switches (on/off)

### 2.4 `slider`  (1 day)
- [ ] Numeric value slider for temperature, budget, etc.

### 2.5 `progress` / `spinner`  (1 day)
- [ ] Rotating indicator while a reply is loading; indeterminate mode
- [ ] Depends on the 1.7 timer

### 2.6 `image` / `icon`  (1 day)
- [ ] Avatar, logo, icon drawing
- [ ] PNG decode (crate: `image` or lightweight `png`) + scaling + alpha

### 2.7 `tabs`  (0.5 day)
- [ ] Chat tabs, settings sections

### 2.8 `code_block`  (2 days)
- [ ] Syntax-highlighted code block, line numbers, indentation (monospace)
- [ ] Requires the 1.2 font infrastructure (Hack already bundled)

### 2.9 `divider` / `spacer`  (0.5 day)
- [ ] Separator line, flexible spacing

### 2.10 `badge` / `tag`  (0.5 day)
- [ ] Role label (YOU/NUDGE), model name badge

---

## SECTION 3 — LAYOUT IMPROVEMENTS
> Estimate: 3–4 days

### 3.1 Real chat bubbles  ★
- [ ] Rounded, aligned bubbles for user/assistant (needs 1.3 + 1.1)
- [ ] Bubble padding, max width, left/right alignment
- [ ] Most visible part of the "ChatGPT feel"

### 3.2 Inline rich text  (1–2 days)
- [ ] Bold, italic, code (`code`), link emphasis within a single text run
- [ ] Requires the 1.2 font infrastructure

### 3.3 System message / status line styles
- [ ] Colored line styles for info / warning / error

### 3.4 Smart spacing & alignment  (0.5 day)
- [ ] Add `align`, `justify`, `expand` to `row` / `column`
- [ ] Currently fixed padding/gap only — no flexible alignment

---

## SECTION 4 — INTERACTION & SMART BEHAVIOR
> Estimate: 3–4 days

### 4.1 Keyboard shortcuts  (1 day)
- [ ] Ctrl+K (command palette), Ctrl+B (sidebar), Ctrl+/ (help)
- [ ] Global shortcut manager for the whole window

### 4.2 Command palette  (1–1.5 days)
- [ ] Searchable list opened with Ctrl+K (new chat, settings, change model...)
- [ ] Fuzzy search + up/down navigation

### 4.3 Keyboard list navigation  (0.5 day)
- [ ] Up/down in the chat list + Enter to open

### 4.4 Drag & drop  (1 day)
- [ ] Message reorder, chat sorting, file drop

### 4.5 Context menu (right-click)  (0.5 day)
- [ ] Copy/regenerate message, delete/rename chat

### 4.6 Copy / paste enhancement  (0.5 day)
- [ ] Rich text paste, copy-code notification

---

## SECTION 5 — ARCHITECTURE / INFRASTRUCTURE (persistence, errors, performance)
> Estimate: 3–5 days

### 5.1 Error capture & display  ★ (2 days)
- [ ] Show LLM call errors in the UI (CAPABILITIES.md: try/catch not verified)
- [ ] Retry button for network/timeout errors
- [ ] Crash recovery: reopen with draft

### 5.2 Async LLM call & cancellation  (2 days) ★
- [ ] Background thread + live updates instead of synchronous blocking
- [ ] Stop (cancel) button; streaming responses
- [ ] Caution: safe integration with UI redraw not verified in CAPABILITIES

### 5.3 True scrolling (independent chat)  → overlaps with 2.1 `scroll`

### 5.4 Window state persistence  (0.5 day)
- [ ] Remember window size/position, theme, sidebar state

### 5.5 Performance: partial redraw  (1 day)
- [ ] Currently the whole framebuffer is redrawn on every event
- [ ] Smoothness on long chats via dirty-rect tracking

---

## SECTION 6 — FILE / EXPORT (extensions)
> Further goals; optional for now

- [ ] 6.1 Chat export (Markdown/JSON/PNG)
- [ ] 6.2 Separate dialog window for settings
- [ ] 6.3 Multi-window (separate chat windows)
- [ ] 6.4 Localization (TR/EN) — requires the 1.2 font infrastructure
- [ ] 6.5 Accessibility: screen-reader semantics, contrast ratio
- [ ] 6.6 Keyboard focus rings — partially present (input)

---

## SUMMARY COUNTS

| Section | Items | Estimate |
|---|---|---|
| 1 — Renderer visual foundations | 9 major | 8–12 days |
| 2 — Missing widgets | 10 widgets | 9–10 days |
| 3 — Layout improvements | 4 | 3–4 days |
| 4 — Interaction | 6 | 3–4 days |
| 5 — Architecture / infrastructure | 5 | 3–5 days |
| 6 — Extensions (optional) | 6 | — |
| **TOTAL (1–5)** | **~34 items** | **~26–35 days** |

### Suggested order (by dependency)
1. **1.8 Theme** (centralize colors) — quick win, simplifies everything — DONE
2. **1.2 Real font** + **1.3 Rounded corners** + **1.1 Alpha** — visual leap — font & alpha DONE
3. **2.1 `scroll`** — unblocks the chat feed
4. **3.1 Bubbles** — where the "ChatGPT feel" comes from
5. **5.2 Async + 5.1 Errors** — required for real usage
6. Remaining widgets (SECTION 2) and interaction (SECTION 4)

### Critical dependency notes
- `code_block` (2.8) and Turkish/text → impossible without **1.2 font**
- Bubbles (3.1) → do not look right without **1.3 rounded corners + 1.1 alpha**
- Spinner (2.5) → cannot animate without **1.7 timer**
- All new widgets → require the compiler + renderer + language syntax trio

### Out of scope (intentional)
- Web/HTML interface (would require replacing the renderer entirely)
- External GUI library (egui/iced) — separate path
- GPU acceleration (softbuffer stays software; wgpu would be a separate project)
