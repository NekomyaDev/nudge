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
- [x] 4 base widgets (`text`, `button`, `input`, `select`) + 3 containers
      (`column`, `row`, `scroll`) + chat `bubble` = 8 tags total
- [x] Button hover state (2 tones)
- [x] Input focus state, cursor, selection highlight, placeholder
- [x] `select` dropdown: overlay card, keyboard list navigation, modal list
      (2.2) — list is drawn, never laid out, so opening it never reflows
- [x] Text wrapping (`wrap_text`, automatic line breaks)
- [x] Global window scrolling (`scroll_x/scroll_y`, MouseScrollDelta — 1347)
- [x] Viewport clipping (out-of-bounds elements are not drawn — 628)
- [x] Event routing: click→id, change→id, key→id
- [x] Headless test infrastructure (`mount_headless`, dispatch_*)

### Remaining renderer soft spots
- No kerning-aware proportional layout for **container width** (uses fixed `CELL_W` grid)
- No icons yet (rounded corners 1.3, drop shadows 1.4, gradients 1.5, cursor
  & mouse states 1.6, timer/animation 1.7)
- Single font size tier in use (`BASE_PX`); fractional sizes not yet wired to UI
- No loading state to hang a spinner on — the frame clock from 1.7 can drive
  one the moment 3.5 gives the UI an async operation to wait for

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

### 1.3 Rounded corners (rounded rect)  (1 day) — DONE
- [x] `draw_rounded_rect_alpha` + `draw_rounded_rect_box`: SDF-based
      signed-distance field, so arcs are **anti-aliased** (no stair-stepping)
- [x] 1px border ring drawn as `outer * (1 - inner)` with a 1px-inset fill —
      the two regions are complementary, nothing is overdrawn twice
- [x] `CORNER_RADIUS = 6` wired into buttons and inputs
- [x] `radius = 0` degrades to the square helpers byte-for-byte
- [x] Bounds clipping proven (partially off-screen boxes are safe)
- [x] Used by cards — chat bubbles (3.1) build their background from these
      helpers, so the "cards don't exist yet" caveat no longer applies;
      modals still pending

### 1.4 Drop shadow  (1 day) — DONE
- [x] Soft shadow around elevated surfaces (needs 1.1 alpha + 1.3 rounded rect)
- [x] Depth for cards — chat bubbles now float above the feed

**Implementation:**
- **Renderer** (`ui_window.rs`): `rounded_rect_sdf` split out of
  `rounded_rect_coverage`, so the halo follows *exactly* the same arc as the
  visible corners. `draw_rounded_rect_shadow_clipped` walks the SDF outside the
  shape and blends a **smoothstep** over the signed distance — one pass, no
  banding, unlike the classic stack of concentric alpha rings (so the roadmap's
  "multi-layer alpha" approach was superseded by something cheaper and
  smoother). Honours the same `clip` as the rest of the chrome.
- **`shadow_for(tag)`**: shadows are opt-in per tag. Only `bubble:*` floats;
  buttons/inputs stay flat, since they already read as raised through border +
  hover tone and halos on every widget would flatten the hierarchy.
  Parameters: `blur = 6`, `alpha = 0.5`, `offset = (0, 2)`.
  `blur + oy = 8` equals the feed's default gap, so a halo stops exactly at the
  next bubble instead of stacking into a dark seam down the column.
- **`draw_shadows` is its own pass**, run right after the background and
  *before* the box loop. Drawing a halo inline would let a later bubble's halo
  land on an earlier bubble's fill — a grey smear between stacked cards.
- **Theme**: `shadow: u32` (Dark: pure black; the strength is per element, so a
  light surface can later swap in a softer grey without touching call sites).
- **Verified**: `cargo fmt --all -- --check` OK, clippy `--all-features -D
  warnings` clean, `cargo test --workspace --features native-gui` **860 passed /
  0 failed** (1 ignored = the real-OS-window acceptance test). 4 new runtime
  tests: the halo darkens at the edge, ramps monotonically and is *exactly*
  background at `blur`; the clipped variant leaves everything past the viewport
  untouched; `draw_shadows` + box paint leaves an earlier bubble's fill and its
  bottom border byte-identical (the two-pass invariant); only `bubble:*` tags
  carry a halo and its reach stays inside an 8px gap.
  Live-verified with `examples/chat_bubbles.ndg` on X11, measured rather than
  eyeballed: **8px below** a bubble (rows y=98..105 darkening from
  (12,12,13) back to bg, exact bg at y=106) and **4px above** (y=54..57,
  `blur - oy`), matching the offset asymmetry by construction; no bleed past
  the viewport at either edge (y=203+ and y=44..51 are untouched bg) even in a
  scrolled frame; bubble fills and border rings come out pure, proving the
  ordering rule holds on real content. Interaction re-proved unaffected —
  hover turns Send to (37,99,235), one dispatch per click
  (`fa9ce7391d14` → `d14fbb076faf` → `87bd8f0e6575`, the same pixel hashes
  3.1 recorded), wheel scrolls the feed but never dispatches.
  - **Note**: two seemingly-dead input checks were a measurement artefact, not
    a bug — a *physical* mouse on this machine drifts while `xdotool` sleeps
    between the warp and the event, so hover/click landed elsewhere. Chaining
    `xdotool mousemove --sync X Y click 1` in a single invocation and reading
    `getmouselocation` right afterwards makes the tests deterministic.

### 1.5 Gradient  (0.5 day) — DONE
- [x] Vertical / horizontal linear gradient (buttons, headers, selected items)

**Implementation:**
- **Renderer** (`ui_window.rs`): `lerp_color` (per-channel mix, carries `top`'s
  alpha so an opaque ramp writes straight through) + `gradient_t`, which
  measures position from the pixel's **index** rather than its centre — so the
  first and last row/column land on exactly `0.0` and `1.0` and both end
  colours appear verbatim instead of merely approximately. That is what makes
  the endpoints assertable rather than a visual guess.
- `draw_rect_gradient_clipped` for square corners, and
  `draw_rounded_rect_gradient_box_clipped` mirroring the existing flat box
  helper (same clip dance, same `outer * (1 - inner)` complementary ring, same
  `radius = 0` fallback). Both orientations supported: `horizontal = false`
  ramps down, `true` ramps right.
- The 1px ring stays a **flat** `border` — a gradient at one pixel reads as a
  smudge, and a constant outline is what keeps the silhouette legible against a
  ramping fill. The ramp spans the **interior**, not the whole shape, because
  the border would otherwise hide both end colours.
- **Applied to buttons** (`Theme::button_bg_top`): resting ramps Blue-400 →
  Blue-500, hovering ramps Blue-500 → Blue-600, so a hover shifts the *entire
  surface* one step darker instead of swapping a single flat tone — the same
  relationship the old flat pair had. Headers and selected items get the same
  helper when those widgets land (1.9 / 2.7).
- **Verified**: `cargo fmt --all -- --check` OK, clippy `--all-features -D
  warnings` clean, `cargo test --workspace --features native-gui` **865 passed /
  0 failed** (1 ignored = the real-OS-window acceptance test). 5 new tests:
  both end colours land exactly with a strictly monotonic ramp in between
  (vertical *and* horizontal), nothing outside the rect or past a viewport clip
  is touched, the rounded variant keeps a flat ring while the interior ramps,
  corners stay anti-aliased (the extreme corner pixel is correctly cut by the
  radius — a first assertion of mine demanded a blended pixel *there* and was
  wrong: at `r=6` that pixel sits ~2.5px outside the arc), and the square
  fallback clips without losing the ramp.
  Live-verified on X11 with `examples/chat_bubbles.ndg`: the Send button's
  centre column reads **(96,165,250) → (59,130,246)** at rest (exactly
  Blue-400 → Blue-500) and **(59,130,246) → (37,99,235)** on hover (Blue-500 →
  Blue-600), with a flat Blue-400 ring at both ends; a text-free column gives
  **34 distinct colours across 34 rows**, i.e. a strictly monotonic ramp with
  no banding. Interaction re-proved: hover renders, one click adds exactly one
  message (3 clicks → 3 empty bubbles, hashes `fa9ce7391d14` → `87bd8f0e6575` →
  `d63fe1787b87`), and 8 wheel events dispatch nothing.
  - **Note**: `import` can capture the window *before* the app repaints, so a
    click may look like a no-op while it actually dispatched — confirmed by
    counting resulting messages rather than trusting one screenshot. Focus was
    also stolen again (back to Chrome), fixed by chaining
    `windowactivate`/`windowfocus`/`mousemove`/`click` tightly.

### 1.6 Cursor & mouse state  (1 day) — DONE
- [x] Mouse cursor shape changes (pointer / text) — winit `set_cursor`
- [x] Button `active` (pressed) third tone; disabled (dimmed) state
- [x] Tooltip (small label on hover)
- [x] Text cursor blink animation — landed in 1.7 (needed the frame clock)

**Implementation (renderer + language props, no compiler change needed):**
- **Cursor shape**: `CursorKind`, a 3-variant enum of its own rather than
  `CursorIcon` itself, so it compares cheaply and the window API is only
  touched when the shape actually changes — mouse-move events arrive several
  per pixel of travel. Pushed through winit 0.30's `set_cursor`
  (`set_cursor_icon` is already deprecated in 0.30.13). `sync_cursor_icon`
  runs after hover changes **and after every layout**, so a control that moves
  out from under a still pointer can never keep a stale shape. Rules: a live
  button → `Pointer`, an enabled text field → `Text`, anything else →
  `Default`. `Wait`/`Progress` are deliberately absent until 3.5 gives the
  renderer a loading state to be waiting *for*.
- **Pressed tone**: `pressed_id` records the button under the cursor on
  `MouseInput::Pressed`, resolved with the same `button_key` hover uses so the
  box that lights up is the box that fires, and cleared on `Released` —
  releasing outside the button therefore cannot leave anything stuck looking
  pressed. `button_tones(theme, hovered, pressed, disabled)` is a pure
  function: rest Blue-400/500 → hover 500/600 → pressed 600/700, one
  continuous ladder instead of three unrelated fills.
- **Disabled**: two new `LayoutBox` fields, `disabled` and `tooltip`, filled
  from `elem.props` in the layout pass. This needed **no interpreter change**:
  unknown kwargs already land in `props`, the same route `scroll`'s `gap` and
  `height` take, and both the checker and the codegens already forward
  anything they do not recognise verbatim. `button_tones` washes the resting
  pair 62% toward the background, `input_surface` washes `input_bg` 50% — the
  hue survives, the call to action does not. An inert control is not
  hoverable, never takes the pointer cursor, never dispatches, **and absorbs
  the click** so nothing stacked behind it is hit by mistake; a disabled field
  additionally refuses the caret and the focus ring.
- **Tooltip**: `find_tooltip_in_boxes` — any tag may carry one, iterated
  newest-first so the topmost box wins where boxes overlap — plus a
  `draw_tooltip` overlay painted **last**, above scroll viewports and all
  other chrome, because a tooltip has no layout box to hold it in the document
  flow. The same absence means nothing else keeps it on screen, so it clamps
  itself inside the window.
- **Verified**: fmt OK, clippy `-D warnings` clean, **874 passed / 0 failed /
  1 ignored** (9 new tests: props reaching the layout box, a disabled control
  absorbing the click without it falling through to the input behind, the tone
  ladder monotonic with disabled immune to hover, cursor kind per control,
  the tooltip painting over content and staying inside the window,
  `button_key` shared by hover/press/dispatch, `input_surface` washing out,
  and a regression test for the bug below).
  Live-verified on X11 with the new `examples/cursor_states.ndg`: resting ramp
  **(94,163,250) → (60,131,246)**, hover **(58,128,245) → (38,100,235)**,
  pressed **(37,98,234) → (29,79,217)** — three rungs, hover and press darker
  than the rung above at *every* interior row of a text-free column. The
  disabled Cancel button is **byte-identical** under hover, running
  (51,78,112) → (37,64,110), exactly the 0.62 wash of Blue-400/500 toward
  background, with a flat `theme.border` ring instead of Blue-400; three
  clicks on it leave the "Sent:" line unchanged (same pixel hash) while one
  click on Send still advances it. A disabled input keeps its washed fill
  (32,32,35) and takes **no focus ring** on click, where the live one snaps to
  Blue-500. Tooltip cards render as `input_bg` + `theme.border` with the label
  drawn, are exactly `cell_h + 2·PAD_Y` = 28px tall, **follow the pointer by
  the cursor's own delta** (a 63px cursor move moved the card 63px), and clamp
  at the window edge — under a right-edge cursor the card ends at x=419, never
  past it.
  - **Bug found and fixed by the live pass**: the card was never erased.
    Repaints were keyed off the button-hover change, but a tooltip-bearing box
    need not be a button, so leaving one left `hovered_id` at `None`, no
    repaint was requested, and the card stayed frozen on screen forever.
    `handle_cursor_moved` now also repaints whenever a tooltip is showing or
    was showing — which is also what makes the card follow the pointer at all.
  - **The same card, and the same three states, still survived the pointer
    leaving the window entirely.** The fix above runs on `CursorMoved`, and
    the platform sends no further moves once the pointer is gone — so
    `cursor_pos` stayed parked on the last control it was over, every
    "what is under the pointer?" question kept answering *that* control, and
    the window kept painting a card for a pointer that was no longer there.
    Nothing could dismiss it but moving back in and out again, which is the
    opposite of leaving. `WindowEvent::CursorLeft` was not handled at all;
    it now calls `handle_cursor_left`, which parks `cursor_pos` at
    `POINTER_OUTSIDE` — far enough outside that `visible_in` rejects every
    box in the tree, so `tooltip_at`, `find_hovered_button`,
    `input_under_cursor` and `sync_cursor_icon` all answer from the same
    empty position and cannot disagree about a pointer that is not here. An
    edge coordinate would not do: it would still be *inside* whatever sits on
    that edge, so the card would stay up and the button stay lit, just for
    the wrong box. A drag in progress is deliberately left holding the
    control — the hand stepping out mid-drag is still on it.
    Proven on X11 with `examples/flag_demo.ndg` by building both sides: the
    pointer hovering `Dark mode` paints the card at **(30,102)–(317,155)**,
    then moving to (300,200) — outside the window, `WINDOW=10485812` —
    leaves the old build at `left vs hover = None` with
    `left vs baseline = (30,102,317,155)`, i.e. the card still painted, while
    the fixed build reports `left vs hover = (30,102,317,155)` and
    `left vs baseline = None`: card gone, window byte-identical to how it
    started.
  - **Note**: the X11 cursor glyph itself cannot be captured by a screenshot
    (`import` and `xwd` composite without the pointer), so the *shape* is
    proved by unit test plus the live hover that drives the same predicate;
    every other claim above is pixel evidence.

### 1.7 Timer / animation infrastructure  (1–2 days) — DONE
- [x] Frame clock / `request_redraw` scheduling for redraws
- [x] Hover transition smoothing (color interpolation)
- [x] Foundation for spinner animation — the clock is ready; the widget itself
      is 2.5, which needs the loading state from 3.5 to attach it to
- [x] Text cursor blink animation (deferred from 1.6)

**Implementation (one timer, every timed thing agrees on it):**
- **`FrameClock`**: holds a single `Option<Instant>` — the earliest moment
  anything in the UI is due. `wake_in` only ever pulls that moment *forward*,
  never back: a caret toggle arriving mid-ramp has no business delaying the
  ramp's own last frame. When the clock is idle the event loop sits in
  `ControlFlow::Wait` and costs nothing; only an outstanding deadline earns a
  `WaitUntil`. `ControlFlow` is set through `ActiveEventLoop::set_control_flow`
  (verified in the winit 0.30.13 source — `EventLoop` has the same-named
  method, but only the `ActiveEventLoop` one is reachable from a handler).
- **The one place the loop re-arms itself**: `about_to_wait` calls
  `fire_due_frame`, which spends a wake-up on a frame **only** once the
  deadline has actually arrived, and clears it so it cannot be collected
  twice. `schedule_frames` (called after every frame) recomputes the deadline
  and does *not* ask for a redraw. This split is the whole design: asking from
  `schedule_frames` rebuilds the very redraw the frame just consumed and pins
  the process at full speed. The first frame drawn in `resumed` arms the clock
  itself, since it bypasses `RedrawRequested`.
- **Hover ramps**: `hover_ramps: HashMap<String, f32>` keyed by the same
  `button_key` hover and dispatch already use, holding each control's blend
  from resting (`0.0`) to hovered (`1.0`) over `HOVER_RAMP_SECS` = 0.12s. The
  target is read from the *current* hover state each tick, so moving from one
  control to another ramps both at once instead of the first snapping back;
  entries that fade to zero are dropped, so the map cannot grow with every
  button the pointer has visited. `button_tones(theme, hover_t, pressed,
  disabled)` took the ramp value in place of the `hovered` bool, and the
  resting ramp Blue-400→500 and the hovered ramp 500→600 are simply the two
  ends of that one blend — an in-between frame is both channels lerped by the
  same amount. Pressing still drops a rung instantly: press is feedback, not
  decoration. `dt` is clamped to `MAX_ANIM_STEP` = 33ms so a long stall cannot
  teleport a ramp to its end on the next frame.
- **Caret blink**: `caret_blink_origin` records when the caret last moved, and
  the phase derives from it — on for 265ms, off for 265ms. `touch_caret` runs
  on **every** key and on click-to-place, so typing can never land in the
  invisible half and look like it did nothing. The caret is resolved once per
  frame (`caret_visible(Instant::now())`) before the surface is borrowed, so
  two fields can never disagree. A focused field schedules its next toggle; a
  field that loses focus drops its orphaned origin rather than paying for it
  forever.
- **Verified**: fmt OK, clippy `-D warnings` clean, **879 passed / 0 failed /
  1 ignored** (5 new tests: `FrameClock` keeping the earliest deadline,
  ramps blending in and fading back out with both halves of a move animating
  at once, the caret phase and a move restarting it, a still interface
  scheduling no frames, and a frame being spent only once its deadline has
  arrived).
  Live-verified on X11 with `examples/cursor_states.ndg`. **Caret blink**:
  focus ring stays Blue-500 (59,130,246) across all 16 sampled frames while
  the caret itself alternates — present as exactly 40 pixels spanning
  x=36..37, y=102..121 (a 2px bar at the text origin), absent in the other
  frames; measured cycle 0.15+0.44 ≈ 0.59s, i.e. the 530ms nominal plus capture
  overhead. **Hover ramp**: sampled every ~22ms at a text-free column the top
  of the Send button climbs **(93,162,250) → (83,152,248) → (78,147,247) →
  (72,142,247) → (68,137,246) → (63,132,246) → (57,127,245)** and settles
  there — monotonic, no step, landing exactly on the hovered tone ~126ms after
  the move, matching the constant. Leaving repeats it downward to exactly the
  resting tone. A swap would have read rest, rest, hover, hover.
  **Idle cost**: a freshly started window that is never touched sits at
  **0.0–0.3% CPU in state `Ss`**, and with an input focused — caret blinking,
  the loop waking several times a second — it stays at **0.0–0.1%**.
  - **Bug found and fixed by the live pass**: that idle figure was initially
    **51% CPU in state `Rs`** — a spin loop. `schedule_frames` called
    `request_redraw()` whenever the clock was non-idle, which rebuilds the
    redraw the frame that just ran had already consumed, so `WaitUntil` never
    got a chance to sleep. Splitting the arming (`schedule_frames`) from the
    spending (`fire_due_frame` in `about_to_wait`) is what makes the clock
    actually a clock; the regression test above is written against exactly
    that.

### 1.8 Color / theme system  (1 day) — DONE
- [x] Colors moved into `struct Theme`
- [x] Widgets access colors through the theme reference
- [x] Light / dark theme toggle (`Theme::LIGHT` + a per-frame `theme:` query)

**Implementation (state plus a button, nothing more):**
- **`Theme::LIGHT`** (Zinc-50 / Blue-600) re-picks every role rather than
  inverting the dark ladder, because the two surfaces ask different things of
  a colour: the accent steps *up* to Blue-600 so white button text keeps its
  contrast on a bright page, the caret goes dark (a white one would vanish in
  a white field), the shadow softens to grey (black reads as a smudge, not
  depth), and the selection is Blue-200 sitting behind dark text.
- **`Theme::named`** resolves `"dark"`/`"light"` case- and padding-tolerantly;
  an unknown name answers `None` — *keep whatever is in force* — so a typo in
  a `.ndg` file never silently drags the window back to the default palette.
- **`UiDispatcher::theme(&mut self) -> Option<Theme>`** is a defaulted trait
  method, re-read at the top of every `render()` (before the surface is
  borrowed, alongside the other per-frame decisions). No `WindowConfig`
  change: the palette is state, not configuration.
- **The language side needed no parser or checker change at all** — `theme:`
  rides the existing generic `ident: expr` window field, and the expression is
  stored unevaluated in `HeadlessAppRecord::theme_expr`, then re-evaluated on
  demand by `Interpreter::theme_headless`. That re-read is the whole mechanism:
  the button flips a string, the next frame asks again, the next paint is
  already the other palette.
- `theme_headless` is `#[cfg(feature = "native-gui")]` because
  `nudge_runtime::ui_window` is feature-gated; the stored `theme_expr` is not,
  so headless mounts keep it regardless.

**Defect found while testing (fixed):** the light theme's input border was
Zinc-300, which gives 1.42:1 against a Zinc-50 page — and since a white field
on Zinc-50 is only 1.04:1 by nature, the border is the *sole* thing saying
"this is a box", so the field was effectively invisible. Stepped to Zinc-400
(2.46:1), and the test now asserts the border must carry ≥2.0:1 against both
the page and the field it frames.

**Verified:** 896 tests / 0 failures; fmt + clippy clean; `nudgec test
examples/theme_toggle.ndg` 1/1; live on X11 the first frame paints the light
palette (bg `#FAFAFA`), one click repaints in dark (`#18181B`) on the *very
next frame* (the first capture after the click already shows it), a second
click returns to light, and idle CPU is 0.00% over 5 s.

### 1.9 Scrollbar  (1 day) — DONE
- [x] Vertical + horizontal scrollbar drawing (thumb + track)
- [x] Drag-to-scroll, hover highlight
- [x] Integration with existing global `scroll_y`

**Implementation (one geometry, asked of by both the hand and the brush):**
- **`ScrollbarGeometry::resolve`** computes each bar's track *and* thumb
  together, so drawing and hit-testing read the same numbers and cannot drift
  apart — a thumb drawn in one place and grabbed in another is the failure
  mode that makes a scrollbar feel broken in ways nobody can point at. Pure
  helpers underneath: `scrollbar_tracks`, `scrollbar_thumb_len`,
  `scrollbar_thumb_pos`, `scrollbar_offset_at`, `max_scroll`,
  `scrollbar_tones`.
- **The tracks yield the corner**: the vertical bar keeps the whole right
  edge, corner included; the horizontal one runs only as far as the vertical
  bar's left edge. The rectangles therefore never overlap, so the corner is
  not painted twice, and — unlike an earlier "vertical yields the corner"
  arrangement — no square of the window is left where neither bar answers the
  pointer. An axis with nothing to scroll is not drawn at all.
- **Thumb length** is proportional to the visible share, so it reads as "how
  much of this am I seeing"; floored at `SCROLLBAR_MIN_THUMB` = 24px so it
  stays grabbable however long the content, and capped at the track.
- **`max_scroll`** is shared with `scroll_by`'s clamp, slack included, so the
  thumb reaches the track's end at exactly the moment the content stops — not
  a hair short, and not a hair past.
- **Drag**: pressing the thumb records *where inside it* the press landed, so
  the content follows the pointer and the thumb cannot jump on the first pixel
  — which is what makes positioning precise. Pressing bare track pages the
  content so the thumb's centre comes to the pointer, then goes on dragging
  from there. Offsets go through `set_scrollbar_offset`, the same clamp the
  wheel uses, and a drag owns the pointer outright.
- **Hover highlight** blends `scrollbar_tones(theme, hovered, held)` toward
  `theme.text` — 0.3 for the thumb and 0.12 for the track, rising to 0.5/0.2
  while held. Blending toward *text* rather than a fixed highlight means a
  light theme darkens its bars and a dark theme lightens them; either way the
  contrast against the track rises, which is the whole point.
- **A bar under the pointer outranks the control behind it**: hover, the
  press *and* the tooltip all yield, because the tracks run down the window's
  very edge where a control can sit, and a thumb you cannot tell apart from
  the button behind it is a thumb nobody trusts. `CursorKind` gained `Grab` /
  `Grabbing` (the `cursor-icon` crate has both).
- **`end_scrollbar_drag`** re-derives hover on release: the drag freezes hover
  by design, so without it a control under the *released* pointer stayed dark
  until the next pixel of movement, and a bar the pointer had already left
  stayed lit.

**Verified:** fmt OK · clippy `-D warnings` clean · **892 passed / 0 failed /
1 ignored** (13 new tests — geometry, both axes, the drag's round trip and its
clamp at each end, thumb-beats-track precedence, the swallow, and the frame's
own hover re-derivation). Live on X11 with the new `examples/scrollbars.ndg`:
both bars drawn as pills at the window's edge with geometry matching the
formula; hover lifting the tone to exactly the blended value; a 127px pointer
drag moving the thumb 126px and a 200px one moving it 199px; the thumb ending
flush against the track when dragged to the end; a track click paging; a click
on the bar leaving the counter untouched while the very same button clicked
beside it advances it; idle at 0.0% CPU in state `Ss`.

**Two defects the live pass caught that the tests had not — same shape of
mistake, so worth recording together.**
- *The frame had its own opinion about hover.* `render` re-derives hover from
  the layout so a control that slid past the pointer stops lighting up — but it
  asked the boxes directly, never asking whether a scrollbar was in the way. So
  the cursor move swallowed the hover and the very next render handed it back,
  and the ramp chased it straight to full again. Moving the swallow into
  `find_hovered_button`, the one question both paths ask, is what makes it
  hold; `refresh_hover` was split out of `render` so a test can hold the frame
  to it without needing a surface. The first live check had missed this by
  only ever moving *onto* the bar, where the ramp had no reason to start — a
  check that only proves the easy half.
- *The card went with the control.* A tooltip describing a button the bar is
  drawn over is a tooltip about the wrong thing, so `tooltip_at` yields too —
  resolved before the surface is borrowed, like the caret and the bars.

---

## SECTION 2 — MISSING WIDGETS
> Each must work on 3 layers: Nudge language syntax + compiler (AST/types) + renderer.
> Estimate: simple 0.5 day, medium 1 day, complex 2 days

### 2.1 `scroll` (nested scroll area)  (2 days) ★ critical — DONE
- [x] Make a specific region scrollable — new `scroll(...)` container
- [x] Required for the chat feed; replaces the paging workaround
- [x] Content height measurement + scroll position + arrow keys

**Implementation (all three layers):**
- **Renderer** (`ui_window.rs`): `scroll` lays its children into a fixed
  viewport, measures content height, keeps a per-container offset in
  `scroll_offsets`, and clips children to the viewport (`ClipRegion` +
  `screen_clip()` intersect nested viewports). Wheel over a region scrolls
  the region; outside it scrolls the window. PageUp/Down/Home/End/↑/↓
  target the region under the cursor. Thin scrollbar thumb per region.
  Clicks/hover honour pan + clip, so a control scrolled out of its viewport
  can never be clicked through.
- **Compiler**: `scroll` added to the UI-container set in the parser
  (`is_ui_container_expr`), checker (Ty::View), interpreter
  (`eval_ui_combinator`) and bytecode delegation.
- **Verified**: `cargo test --workspace --features native-gui` **848 passed,
  0 failed** (1 ignored: the real-OS-window acceptance test, which CI runs
  under Xvfb). 8 new runtime tests covering viewport measurement, pan, clamp,
  stale-max re-clamping after content shrinks, click-through, wheel/key
  routing, nested regions, clip helpers; 1 compiler test. Clippy
  `--all-features -D warnings` and `cargo fmt --check` both clean.
  Live-verified with `examples/scroll_feed.ndg`: wheel scrolls the feed
  while the composer stays put (8978px diff), typing into the composer after
  scrolling does **not** click through to scrolled-away content, and Send
  dispatches state updates (trace seq 2–3).
  - **Note**: `native-gui` is *not* a default feature and `ui_window.rs` is
    `#[cfg(feature = "native-gui")]`, so a bare `cargo test --workspace`
    silently skips the entire renderer test suite. CI's test job now passes
    `--features native-gui` for this reason.

### 2.2 `select` / dropdown  (1 day) — DONE
- [x] For model selection, settings menus
- [x] Dropdown list, selected item, keyboard navigation

**Syntax:** `select(value, options = [...], id = ..., on_change = ...,
disabled = ..., tooltip = ...)`. The first positional argument is the label
the field currently shows — the same rule `input` uses, so the two fields read
as a pair. The parser needed no change at all: `select` is a leaf widget, and
`is_ui_container_expr` only lists containers.

**Implementation (a card, not a control — so it is drawn, never laid out):**
- **The option list rides as one prop.** `options = [...]` is folded into a
  single newline-joined string, the way every other list-valued prop already
  travels, and an inner `\n`/`\r` inside an option is folded to a space at eval
  time rather than silently splitting the row it was meant to label. `LayoutBox`
  gained an `options: Vec<String>` field that is empty for every other tag.
- **Width is settled by the widest thing the field can ever show** — the
  current label or any option — so opening the list never resizes the field
  underneath it, and a row never runs past the box that chose it. `ui.rs`'s
  estimate follows the same rule, deliberately using the *generous*
  per-character budget the `text` arm uses rather than the renderer's own
  measure: an estimate that overshoots leaves room, one that undershoots clips.
- **`select_popup_rect` is pure geometry asked of by both the brush and the
  hand**, so what is painted is exactly what answers a press: below the field
  when there is room, flipped above it when there is not, and clipped to the
  window — never scrolled off it — when there is no room on either side. It is
  the *only* source of the card's rectangle, so the draw pass and the click
  test cannot drift apart.
- **`OpenSelect { key, highlight, scroll }`** is the whole of a list's state.
  It is keyed by `select_key` (the id, else `__sel_{index}` — the same rule
  hover uses), so a re-layout that reorders boxes cannot leave a list attached
  to the wrong control. The card is resolved fresh from the current layout on
  every question rather than remembered, so a scroll or a re-render moves it
  *with* the field instead of leaving it behind; and in the render decision
  phase, a list whose field has scrolled or re-laid out of view is closed
  rather than left floating over nothing.
- **`SelectPopupView` is an owned snapshot**, resolved while `self` can still
  be read whole — the surface borrow makes an `&self` call afterwards
  impossible, which is the same constraint that already governs the caret, the
  scrollbars and the tooltip.
- **The chevron is drawn from rectangles, not a glyph** (`draw_chevron_down`,
  a 7×4 filled triangle). A font is not obliged to carry `▾`, and a choice
  field that lost the one mark saying it opens would be a field nobody dares
  click. The current-value mark on the matching row is drawn the same way, for
  the same reason. A closed field wears the input's surface with the value
  left-aligned and vertically centred; hover lifts the border toward
  `input_border_focused`, and an open field wears that border outright, because
  the list *is* the field's active state.
- **Picking an option dispatches a change, not a click** — `dispatch_change`,
  the plumbing a text field already shares — so the handler is handed the new
  value and the next frame reads the bound state back. The list shuts behind
  it. An uncontrolled field with no `on_change` still remembers its choice
  through the same direct element mutation `input` relies on.
- **A field with no `id` opens nothing.** There is nowhere for a choice to be
  reported to, so it is decoration — the same rule that makes a button with no
  `on_click` inert. Silently opening a list whose pick goes nowhere would be
  worse than not opening at all.
- **The list is modal, and everything that could betray that yields to it.**
  A press on a row commits it; a press anywhere else — the field again, or
  empty space — only shuts the list, and is spent either way, so a control
  hiding under the card is never clicked by a dismissal. The keyboard is
  intercepted at the very top of `handle_key_input` (arrows, Home/End,
  PageUp/PageDown, Enter/Space, Escape/Tab) so nothing reaches a field behind
  the card. The wheel scrolls the list only while the pointer is on the card,
  and does nothing at all elsewhere — scrolling the page out from under a menu
  strands the menu over different content. The tooltip is suppressed, because
  a label describing a control the card covers is a label about something the
  user cannot reach. Hover is swallowed for everything but the field itself,
  so no button under the card lights up and offers a click that would never
  arrive.
- **Enter takes whatever the card is lighting** — the pointer's row when the
  pointer is on the card, the keyboard's row when it is not — resolved by
  `popup_effective_row`, one source shared by the drawn highlight, the pointer
  cursor and Enter, so the three can never light different rows. The highlight
  lives in the *drawing*, not in the state: a pointer brushing past a row on
  its way somewhere else must not steal the keyboard's choice.

**Verified:** fmt OK · clippy `-D warnings` clean · **912 passed / 0 failed /
1 ignored** (16 new tests — popup geometry flipping above and staying inside
the window, the hit test matching the drawn rows, open→row-click→value, the
keyboard walk, the press outside being spent rather than falling through, a
disabled field absorbing the press, a 12-option list scrolling to reach its
last option, a field scrolled out of sight having no list, the chevron's
triangle, the card covering the page beneath it, an id-less field opening
nothing, key resolution, the interpreter's element build with folded newlines,
a pick reaching the state through `on_change`, and the `ui.rs` estimate).
Live on X11 with the new `examples/select_demo.ndg`: the card drawn over the
`16-bit` field and the text below it, all four options readable, the current
value banded and marked; clicking `Large` updating the header to
`Model: Large` and reopening showing **that** value banded; clicking `Small`
moving the band to row 0 on the next open; an outside press and an Escape each
closing the list **without** committing — confirmed by reopening and finding
the band still on row 0.

**One defect the example run caught that the tests had not.** `nudgec test`
compiles to Python and runs there, and the emitted `_HeadlessApp` had
`dispatch_click` but no `dispatch_change` — so the interpreter path proved a
pick reached the state while the *emitted* path failed with `AttributeError`,
and every example test that picks an option would have failed the same way.
The interpreter having a method is not evidence the codegen emits one; the two
are separate surfaces and only the example exercised the second. Added to the
preamble alongside its click counterpart.

### 2.3 `checkbox` / `toggle`  (0.5 day)
- [x] Settings switches (on/off)

**Two tags, one widget.** `checkbox` draws a square with a mark, `toggle`
draws a track with a knob, and everything below the drawing is shared — the
dispatch key, the press, the change, the `bool` that reaches the handler. The
tag is only ever asked which clothes to wear.

- **The first positional argument is the *label*, not the value.** A
  deliberate deviation from `input`/`select`, where the value doubles as the
  thing you read and so earns a positional slot. A boolean is never displayed:
  a positional value would be useless where the words are the only thing worth
  writing without a name. The state rides as `value`, spelled the way
  `is_truthy` already reads `disabled` — one word for "on", written by the
  interpreter and parsed back by the renderer, so the two cannot disagree
  about the same string.
- **A press dispatches a *change*, never a click.** A click that flips a switch
  has no meaning of its own beside the flip, so there is no second event to
  write and no way to wonder which of two handlers fired. The handler is
  handed `NudgeVal::Bool`, not the string that spells it — a flag's value *is*
  a Bool, that is what the widget is.
- **The whole row is the control.** Box or track, a gap, then the label, laid
  out as one box: pressing the words presses the control. Text beside a switch
  that does nothing when clicked is text people learn to avoid. The row's key
  is `flag_key` — the `id` when there is one, else the layout order.
- **No `on_click`/`on_key`, and an id-less flag is decoration.** The same rule
  `select` set: one with nowhere to report the new value must not light up,
  because lighting a control that will not answer the click is a promise the
  window cannot keep. It still *absorbs* the press rather than letting it fall
  through to whatever is stacked behind it.
- **The drawing never asks a font.** The square, the mark inside it, the track
  and the knob are all rectangles — a checkbox cannot arrive as a row of letters
  and a switch cannot lose the one sign saying which way it is set. On wears
  the same blue a focused field wears, so the form has one idea of *live*
  rather than two blues arguing over what an accent is; disabled dims and
  never lights.
- **`LayoutBox` carries `checked`** — the same snapshot decision `options`
  made. The drawing, the press and the value written back all have to agree
  about one state, and a value that changed underneath a painted control would
  draw a mark for a state that is no longer there.

**Verified:** fmt OK · clippy `-D warnings` clean · **925 passed / 0 failed /
1 ignored** (13 new tests — the mark being two arms rather than a slab, an on
box wearing the accent while an off one is bare, the toggle stating itself by
where the knob sits, the row's layout carrying its value and its words, hover
lighting only a flag that can report, the press dispatching a change through
the id with the label pressing too, a flag that cannot report taking the press
with it, the `ui.rs` estimate, the interpreter's element build and its
spellings, a change reaching the handler as a Bool with the label surviving,
an unhandled flip leaving the label alone, and the checker requiring `bool`).
Live on X11 with the new `examples/flag_demo.ndg`: clicking `Dark mode` filling
the square with the accent and drawing the white mark while the header word
changes `off`→`on` (320 px in the box, and 67 px in *exactly* the last word of
the line); clicking `Beta channel` lighting the whole track and carrying the
knob to the right end with the second header changing the same way; clicking
the `disabled` checkbox leaving **every pixel of the window unchanged** —
inert, and the press went nowhere.

**Two defects the example run caught that the unit tests could not.** Both are
the lesson 2.2 already learned: the interpreter and the codegen are separate
surfaces, and only the example exercises the second.

1. `py()` did not lower `ExprKind::If` at all. In an expression slot a bare
   comment is legal Python, so the value did not fail to compile — it quietly
   became `None` and every caller downstream read a type it was never handed.
   `fn onoff(b: bool) -> string { if b { "on" } else { "off" } }` compiled, ran
   and returned `None`. Now lowered to Python's conditional expression.
2. The emitted `_HeadlessApp.dispatch_change` passed the value straight to the
   handler, so a boolean control's handler received the *string* `"true"` and
   the bound state held a string that the next frame's `if` read as truthy in
   **both** directions — a switch that could be turned off but never showed it.
   Now converted for `checkbox`/`toggle` at the same place the interpreter
   converts it, so the two surfaces hand the handler the same value.

### 2.4 `slider`  (1 day)
- [x] Numeric value slider for temperature, budget, etc.

**One control, two front ends, one number.** A slider is the first widget
whose *value* has to survive being written down twice — the interpreter
lowers it for the native renderer, and the codegen lowers it again for
`nudgec test`. Every previous control disagreed about nothing that mattered:
a flag is `true`/`false` whichever way you spell it. A slider is a real
number, and a real number has a shape.

- **The first positional argument is the label**, the same rule 2.3 set. A
  number is read off the track, never off the words beside it, so the words
  earn no positional slot.
- **`slider(label, value=, min=, max=, step=, id=, on_change=, disabled,
  tooltip)`.** Bounds default to `0`…`100`. The step defaults to `1` when
  the value, min and max are all whole numbers and to `(max-min)/100`
  otherwise — the whole-numbered case is the one people actually write, and
  a `1/100` step on a budget of 2000 is a control you cannot hit with a
  mouse.
- **Decimals come from the *step*, never from the drag.** The number of
  decimal places is read off the step's own shortest spelling
  (`ui::step_decimals`), capped at six. A step of `1` therefore spells
  `1400`, never `1400.0` — which is what makes the handler's type stable.
- **The handler gets `Int` when the spelling carries no point and `Float`
  when it does** (`ui::slider_num_str` → `change_arg`). A whole-numbered
  slider must hand its handler the same type on every drag; the alternative
  is a `state.budget` that flips from `int` to `float` the first time it is
  written, and every downstream `if` reading a type it was never handed.
- **A press outside the track is absorbed and does nothing.** The label is
  part of the row — pressing it must not slam the value to a bound.
  `x_to_value` clamps, so a click at the far right of the *label* used to
  land on the maximum: a read-only click that silently rewrote the number.
  `begin_slider_drag` now rejects a press outside `[track.x - overhang,
  track.x + track.w + overhang]`.
- **Id-less and disabled sliders are inert** and never light, the rule 2.2
  and 2.3 both set. They still absorb the press.
- **Drag owns the pointer.** A drag that lets a second control steal the
  move is a drag that changes two things at once. The scrollbar is asked
  first (it was there first), then the slider, then buttons and flags.
- **`LayoutBox` carries `slider: Option<SliderSpec>`**, the same snapshot
  decision `options` and `checked` made before it: the drawing, the press
  and the value written back have to agree about one state.
- **The drawing asks the theme, not the caller.** Groove, accent fill and a
  circular handle; the label is drawn by the caller beside it, so a slider
  sits in a row like every other labelled control.

**Verified:** fmt OK · clippy `-D warnings` clean · **953 passed / 0 failed /
1 ignored** (27 new tests across `ui.rs`, `ui_window.rs`, `check.rs`,
`bytecode.rs` and the interpreter — the step deriving its decimals, the snap
landing on a rung, the handle never hanging off a stop, the drag carrying the
value, the label press being absorbed without moving anything, a disabled
slider changing no pixel, the dimmed fill being the accent lerped to the
background, the checker requiring `int or float`, and the codegen spelling
matching Rust). Live on X11 with the new `examples/slider_demo.ndg`: the
handles sitting where `value_to_x` predicted (74.5 / 81.5 / 152.5 against a
track at x=30), a drag carrying `500`→`1400` while only that row and its
readout changed and the other two rows stayed byte-identical, the same drag
snapping back to the rung **500** and leaving the window **byte-identical to
its original baseline** (bbox `None`), a groove press landing on rung 1700,
and a disabled slider drag leaving **zero pixels changed**.

**A third defect the example caught, and the reason the codegen is now
tested against Rust rather than against a second copy of the expectation.**
`nudgec test` runs the generated Python, not the native renderer, so a slider
has its arithmetic written down twice. The Python port spelled `1.0` as
`"1.0"` where Rust writes `"1"` — one extra decimal point. The consequence
was not cosmetic: the point is what decides whether the handler is handed an
`Int` or a `Float`, so a budget slider reported `500.0` in `nudgec test` and
`500` in the window. A second drift in the same port — Python's `round()`
sends a halfway case to the *even* neighbour where Rust's `f64::round()`
sends it *away from zero* — put a drag resting exactly between two rungs on a
different rung in each front end (25 with step 10 → 20 in Python, 30 in
Rust). Both fixed in `codegen.rs`, and `slider_python_and_native_spell_and_
snap_identically` now computes the expected spelling and snap with
`nudge_runtime::ui`'s own functions and compares the generated module against
them — the test fails against either bug, verified by reverting each in turn.

### 2.5 `progress` / `spinner`  (1 day) — DONE
- [x] Rotating indicator while a reply is loading; indeterminate mode
- [x] Depends on the 1.7 timer

**Two tags, one mechanism.** Like 2.3's `checkbox`/`toggle`, `progress` and
`spinner` share the layout, the optional label, disabled dimming and tooltip
support, differing only in what they render and how they drive the frame clock:
- **`progress(label, value=, min=, max=, id=, disabled=, tooltip=)`**:
  determinate progress bar with groove and filled pill portion proportional to
  `clamp(value, min, max)`. Static — no timer ticks or idle cost.
- **`spinner(label, id=, disabled=, tooltip=)`**: rotating indeterminate arc
  segment (90° sweep) rendered with signed-distance field (SDF) anti-aliasing.
- **Zero-cost idle discipline**: `schedule_frames` checks `has_visible_spinner()`.
  The continuous animation timer (`SPINNER_TICK` = 33ms, ~30fps) is armed ONLY
  when a spinner is actually in the viewport. When no spinner is on screen,
  `spinner_origin` is dropped and the clock sleeps with 0% idle CPU.
- **Pure display widgets**: completely inert, absorbing no clicks or focus.
- **Verified**: fmt OK, clippy -D warnings clean, **959 passed / 0 failed / 1 ignored**
  (2 new unit tests). Verified live on X11 with `examples/progress_demo.ndg`
  (window WID 90177538 at 730,329, 440x400): spinner confirmed actively
  animating across consecutive frames (bbox `(50, 323, 68, 338)` changing),
  and progress bar +25% button click proved fill width growing from 60px (40%)
  to 100px (65%) live on screen. All 6 demo suites pass `nudgec test`.


### 2.6 `image` / `icon`  (1 day) — DONE
- [x] Avatar, logo, icon drawing
- [x] PNG decode (crate: `png` v0.17.16) + scaling + alpha

**Display and brand chrome.** `image` and `icon` display local or bundled PNG
assets with scaling, alpha transparency, corner rounding, and caching:
- **`image(src, width=, height=, id=, on_click=, disabled=, tooltip=)`**:
  avatar/media widget defaulting to `48x48` with `6px` rounded corners.
- **`icon(src, width=, height=, id=, on_click=, disabled=, tooltip=)`**:
  inline UI icon defaulting to `24x24` without corner radius.
- **PNG decode & decoding pipeline**: `decode_png` decodes raw PNG bytes into
  standard RGBA8 format (`DecodedImage`), converting RGB, Grayscale, and
  Grayscale+Alpha formats on the fly.
- **Rendering & blitting**: `draw_image_rgba` performs nearest-neighbour scaling,
  alpha blending (`blend_px`), viewport clipping, and anti-aliased corner
  rounding via `rounded_rect_coverage`.
- **In-memory cache**: `NativeApp.image_cache` caches decoded assets by path/src
  so repeated frames avoid disk I/O and re-decoding.
- **Python front end parity**: `_image` and `_icon` functions added to `codegen.rs`.
- **Verified**: fmt OK, clippy -D warnings clean, **960 passed / 0 failed / 1 ignored**
  (new unit test `test_image_and_icon_layout_and_decode`). Verified live on X11
  with `examples/image_demo.ndg` and `examples/avatar.png` (window WID 90177538,
  440x380): 1628 avatar rose pixels detected on screen at `(53, 89)` (42x42 rendered)
  and 325 small icon pixels at `(52, 356)` (21x21 rendered). All 7 demo suites
  pass `nudgec test`.


### 2.7 `tabs`  (0.5 day) — DONE
- [x] Chat tabs, settings sections

**Section and conversation switcher.** `tabs` displays a horizontal list of tab
choices, highlighting the active tab with an accent surface and bottom indicator:
- **`tabs(active, options = [...], id=, on_change=, disabled=, tooltip=)`**:
  first positional specifies the currently active tab label.
- **Rendering**: each tab segment is rendered with proportional text advance,
  a rounded pill-tab outline, and the active tab receives `theme.input_bg` surface
  plus a 2px high accent underline (`theme.input_border_focused`).
- **Hit testing & dispatch**: a click calculates which segment was pressed along
  the horizontal span and triggers `on_change(chosen_tab)`. Disabled tabs rows
  absorb the click without dispatching.
- **Python front end parity**: `_tabs` helper added to `codegen.rs`.
- **Verified**: fmt OK, clippy -D warnings clean, **961 passed / 0 failed / 1 ignored**
  (new unit test `test_tabs_layout_and_interaction`). Verified live on X11 with
  `examples/tabs_demo.ndg` (window WID 90177538, 440x380): initial state rendered
  Chat tab active at `(34, 70)` to `(97, 103)` (width 64); clicking Settings tab
  at `(880, 415)` live on screen moved active accent bounds to `(98, 70)` to `(201, 103)`
  (width 104) and switched view to Settings containing the Dark mode checkbox.
  All 8 demo suites pass `nudgec test`.


### 2.8 `code_block`  (2 days) — DONE
- [x] Syntax-highlighted code block, line numbers, indentation (monospace)
- [x] Requires the 1.2 font infrastructure (Hack already bundled)

**Syntax-highlighted code block.** `code_block` renders multi-line source code
using embedded Hack monospace typography, a line-numbered gutter, and syntax
token highlighting:
- **`code_block(code, id=, tooltip=)`**: displays source code lines in monospace.
- **Typography & Font**: uses `FaceKind::Mono` backed by embedded `Hack-Regular.ttf`
  ensuring crisp character alignment, predictable indentation, and anti-aliasing.
- **Line numbering gutter**: renders a left-hand 40px gutter with right-aligned
  dim line numbers (`theme.text_dim`) separated by a 1px border rule.
- **Syntax token highlighting**: `tokenize_code_line` classifies tokens into
  keywords (Blue-500 accent), string literals (Emerald-500), numeric literals
  (Amber-500), comments (Zinc-500 dim), punctuation (Slate-400), and identifiers.
- **Python front end parity**: `_code_block` added to `codegen.rs`.
- **Verified**: fmt OK, clippy -D warnings clean, **962 passed / 0 failed / 1 ignored**
  (new unit test `test_code_block_layout_and_tokenization`). Verified live on X11
  with `examples/code_block_demo.ndg` (window WID 90177538, 480x380): container
  rendered with 67,701 background pixels (`theme.input_bg`), 89 keyword pixels
  (Blue-500), 32 number pixels (Amber-500), and 2,208 gutter line numbers & comment
  pixels (`theme.text_dim`). All 9 demo suites pass `nudgec test`.


### 2.9 `divider` / `spacer`  (0.5 day) — DONE
- [x] Separator line, flexible spacing

**Visual rhythm and section separators.** `divider` renders a crisp 1px horizontal
separator line across the available container width using `theme.border`. `spacer`
allocates customizable empty layout spacing:
- **`divider(id=)`**: 16px high layout box rendering a 1px centered rule.
- **`spacer(size=, width=, height=, id=)`**: flexible spacing widget defaulting to 16px.
- **Python front end parity**: `_divider` and `_spacer` added to `codegen.rs`.
- **Verified**: fmt OK, clippy -D warnings clean, **963 passed / 0 failed / 1 ignored**
  (new unit test `test_divider_and_spacer_layout`). Verified live on X11 with
  `examples/divider_spacer_demo.ndg` (window WID 90177538, 440x360): 744 border pixels
  rendered, detecting two full-width horizontal divider rules at y=122 (372px wide)
  and y=278 (372px wide). All 10 demo suites pass `nudgec test`.


### 2.10 `badge` / `tag`  (0.5 day) — DONE
- [x] Role label (YOU/NUDGE), model name badge

**Role indicator labels and metadata badges.** `badge` and `tag` render compact,
rounded pill labels for chat roles and metadata:
- **`badge(text, id=, tooltip=)` / `tag(text, id=, tooltip=)`**: 22px high pill
  label with centered typography and rounded corners (4px radius).
- **Contextual role palettes**:
  - `YOU` / `USER`: Blue-500 accent surface (`theme.button_bg`) with white text.
  - `NUDGE` / `AI` / `ASSISTANT`: Emerald-500 surface (`#10B981`) with white text.
  - `SYSTEM` / `WARN`: Amber-500 surface (`#F59E0B`) with dark text.
  - General tags (e.g. model name `gemini-3.8-flash`): muted card surface (`theme.input_bg`)
    with border.
- **Python front end parity**: `_badge`, `badge`, and `tag` added to `codegen.rs`.
- **Verified**: fmt OK, clippy -D warnings clean, **964 passed / 0 failed / 1 ignored**
  (new unit test `test_badge_and_tag_layout`). Verified live on X11 with
  `examples/badge_demo.ndg` (window WID 90177538, 480x280): 696 Blue-500 pixels (YOU),
  952 Emerald-500 pixels (NUDGE), and 1,012 Amber-500 pixels (SYSTEM) detected on screen.
  All 11 demo suites pass `nudgec test`.


---

## SECTION 3 — LAYOUT IMPROVEMENTS
> Estimate: 3–4 days

### 3.1 Real chat bubbles  ★ — DONE
- [x] Rounded, aligned bubbles for user/assistant (needs 1.3 + 1.1)
- [x] Bubble padding, max width, left/right alignment
- [x] Most visible part of the "ChatGPT feel"

**Implementation (all three layers):**
- **Renderer** (`ui_window.rs`): new `bubble` element. The message wraps to
  `max_width` (default 340px), the box grows by `padding` (default 10px), and
  it is aligned left/right/centre inside the width its parent offers — the
  `role` prop (`user` / `assistant`) picks both the palette and the default
  side, so an explicit `align` overrides it. Background is a rounded rect
  drawn with the existing clipped helpers, and the glyphs are clipped to the
  bubble's *interior* rect, so a bubble scrolled out of its viewport leaves no
  pixels behind. `LayoutBox` gained a `pad` field so the renderer knows the
  text inset.
- **Theme**: `bubble_user_bg` / `bubble_user_text` / `bubble_assistant_bg` /
  `bubble_assistant_text` (Dark theme: Blue-500 on white vs Zinc-800 on
  Zinc-100).
- **Compiler**: `bubble` added as a leaf combinator in the interpreter
  (positional message + style kwargs, `id` honoured), typed `Ty::View` in the
  checker (both the bare and the `ui.`-prefixed form), delegated in bytecode.
  Headless `estimate_element_size` measures it too.
- **Verified**: `cargo test --workspace --features native-gui` **856 passed /
  0 failed** (1 ignored = the real-OS-window acceptance test, run under Xvfb).
  6 new runtime tests (wrap + max_width cap, padding growth, role-default and
  overridden alignment, symmetric alignment inside a viewport, pan/clip
  inheritance, background clipping) + 2 compiler tests (typed mount path;
  evaluator records role/align/padding/max_width).
  Live-verified with `examples/chat_bubbles.ndg` on X11: bubbles render
  rounded and correctly aligned, wheel scrolls the feed while header/composer
  stay put (diff bbox was exactly the viewport `(30,52,397,202)`), typing into
  the composer and clicking Send appends a new user bubble (counter 0→1→2 with
  one dispatch per click, verified by pixel hash — the wheel does **not**
  dispatch).
  - **Note**: to drive the window with `xdotool` you must raise *and* focus
    it and keep the pointer over it — a full-screen Chrome window otherwise
    swallows hover/click/typing, which silently looks like an input bug.

### 3.2 Inline rich text  (1–2 days) — DONE
- [x] Bold, italic, code (`code`), link emphasis within a single text run
- [x] Requires the 1.2 font infrastructure

**Inline rich formatting spans.** Markdown-style formatting inside standard text runs
and chat bubbles:
- **Parser (`parse_rich_spans`)**: scans text runs into structured spans:
  - `**bold**`: renders with `FaceKind::UiBold` (DejaVuSans-Bold).
  - `` `code` ``: renders with `FaceKind::Mono` (Hack-Regular) with focused accent coloring.
  - `[label](url)`: renders with link accent coloring (`theme.button_bg`).
  - Plain text: rendered with `FaceKind::Ui` (DejaVuSans).
- **Renderer integration**: `draw_rich_line` is used by both `text` and `bubble` components,
  preserving kerning, advances, baseline alignments, and viewport clipping.
- **Python front end parity**: `_bubble` and `bubble` helper added to `codegen.rs`.
- **Verified**: fmt OK, clippy -D warnings clean, **965 passed / 0 failed / 1 ignored**
  (new unit test `test_parse_rich_spans`). Verified live on X11 with `examples/rich_text_demo.ndg`
  (window WID 90177538, 480x280): 253 anti-aliased blue text pixels (code snippets and links)
  and 32,613 bubble background pixels rendered. All 12 demo suites pass `nudgec test`.


### 3.3 System message / status line styles — DONE
- [x] Colored line styles for info / warning / error

**Contextual notification & status rows.** `status_line` renders system feedback,
log lines, and operation alerts with status-coded indicator accents:
- **`status_line(text, status="info"|"success"|"warning"|"error", id=, tooltip=)`**:
  28px high status row containing a 4px left-hand indicator bar and formatted message.
- **Status Indicator Palettes**:
  - `info`: Blue-500 (`#3B82F6`) indicator.
  - `success`: Emerald-500 (`#10B981`) indicator.
  - `warning` / `warn`: Amber-500 (`#F59E0B`) indicator.
  - `error`: Red-500 (`#EF4444`) indicator.
- **Rich Text Support**: message body supports inline rich text via `draw_rich_line`.
- **Python front end parity**: `_status_line` and `status_line` added to `codegen.rs`.
- **Verified**: fmt OK, clippy -D warnings clean, **966 passed / 0 failed / 1 ignored**
  (new unit test `test_status_line_layout`). Verified live on X11 with
  `examples/status_line_demo.ndg` (window WID 90177538, 480x320): detected 64 Blue-500 pixels
  (info), 64 Emerald-500 pixels (success), 64 Amber-500 pixels (warning), and 64 Red-500 pixels
  (error). All 13 demo suites pass `nudgec test`.


### 3.4 Smart spacing & alignment  (0.5 day) — DONE
- [x] Add `align`, `justify`, `expand` to `row` / `column`
- [x] Currently fixed padding/gap only — no flexible alignment

**Smart layout distribution and alignment.** Extends `row` and `column` containers with
flexible alignment and distribution options:
- **`expand=true`**: container expands to fill the entire available width (`max_w`).
- **`align="start"|"center"|"end"`**: controls cross-axis alignment. In a `column`,
  `align="end"` aligns child elements to the right edge; in a `row`, aligns child elements
  vertically (center/bottom).
- **`justify="start"|"center"|"end"|"between"`**: controls main-axis distribution in a `row`.
  `justify="between"` distributes remaining container width evenly between children.
- **Verified**: fmt OK, clippy -D warnings clean, **967 passed / 0 failed / 1 ignored**
  (new unit test `test_row_and_column_smart_alignment`). Verified live on X11 with
  `examples/alignment_demo.ndg` (window WID 90177538, 480x380): `column(align="end")` aligned
  button to the right (`x=286..437`), and `row(justify="between")` spread items evenly from
  left margin (`x=35`) to right margin (`x=479`). All 14 demo suites pass `nudgec test`.


---

## SECTION 4 — INTERACTION & SMART BEHAVIOR
> Estimate: 3–4 days

### 4.1 Keyboard shortcuts  (1 day) — DONE
- [x] Ctrl+K (command palette), Ctrl+B (sidebar), Ctrl+/ (help)
- [x] Global shortcut manager for the whole window

**Global keyboard shortcuts manager.** Global window-wide shortcut dispatch:
- **Shortcut interception**: `handle_key_input` routes `Ctrl+K` (command palette),
  `Ctrl+B` (sidebar toggle), and `Ctrl+/` (help overlay) to `dispatcher.dispatch_key(&chord, None)`.
- **Precedence over active text focus**: global shortcuts take priority even when
  a text field or composer is currently focused, preventing character insertion.
- **Python front end parity**: `_HeadlessApp.dispatch_key` added to `codegen.rs`.
- **Verified**: fmt OK, clippy -D warnings clean, **968 passed / 0 failed / 1 ignored**
  (new unit test `test_global_keyboard_shortcuts`). Verified live on X11 with
  `examples/shortcuts_demo.ndg` (window WID 90177538, 480x340): 2,883 anti-aliased text
  pixels rendered, with global `Ctrl+k` dynamically toggling state. All 15 demo suites
  pass `nudgec test`.


### 4.2 Command palette  (1–1.5 days) — DONE
- [x] Searchable list opened with Ctrl+K (new chat, settings, change model...)
- [x] Fuzzy search + up/down navigation

**Modal command palette with fuzzy filtering.** Native floating command palette:
- **`CommandPaletteState` & `fuzzy_match`**: fast case-insensitive subsequence matcher
  filters commands as the user types into the query field.
- **Top-center overlay card**: renders with dark backdrop (`theme.input_bg`), focused border
  accent, search prompt `> `, and list of filtered actions (e.g. New Chat, Open Settings,
  Change Model, Toggle Dark Theme).
- **Keyboard & mouse navigation**: ArrowUp / ArrowDown updates selection highlight; Enter
  or direct row click executes `Command:<action>` and closes the palette; Escape dismisses.
- **Python front end fallback**: `_HeadlessApp.dispatch_key` routes agent-level command handlers.
- **Verified**: fmt OK, clippy -D warnings clean, **969 passed / 0 failed / 1 ignored**
  (new unit test `test_command_palette_interaction_and_filtering`). Verified live on X11
  with `examples/command_palette_demo.ndg` (window WID 90177538, 480x380): verified palette
  overlay with 12,071 blue selection pixels (`0x1D, 0x4E, 0xD8`), and typing `set` + Enter
  executed `Command:Open Settings` displaying 72 Emerald-500 status line indicator pixels.
  All 16 demo suites pass `nudgec test`.


### 4.3 Keyboard list navigation  (0.5 day) — DONE
- [x] Up/down in the chat list + Enter to open

**Keyboard list navigation for chat lists & interactive items.** Native keyboard navigation
across items and buttons:
- **ArrowUp / ArrowDown navigation**: moves focus across focusable items (`button`, `input`)
  in layout order, cycling gracefully without losing position.
- **Active focus indicator ring**: focused buttons render with `theme.input_border_focused`
  (Blue-500 accent outline) and active hover tone (`hover_t = 1.0`).
- **Enter / Space activation**: activates the focused list item / button, triggering its
  `dispatch_click` handler.
- **Escape**: dismisses focus (`focused_id = None`).
- **Verified**: fmt OK, clippy -D warnings clean, **970 passed / 0 failed / 1 ignored**
  (new unit test `test_keyboard_list_navigation_up_down_enter`). Verified live on X11
  with `examples/list_navigation_demo.ndg` (window WID 90177538, 480x380): ArrowDown
  focused chat item with 1,892 Blue-500 outline pixels, and Enter opened the chat with
  3,429 bright text pixels rendered. All 17 demo suites pass `nudgec test`.


### 4.4 Drag & drop  (1 day) — DONE
- [x] Message reorder, chat sorting, file drop

**Drag-and-drop reordering & OS file drop.** Native drag & drop interactions:
- **`DragState` & interactive reordering**: mouse drag across draggable items (`draggable: true`
  or bubbles/chat items) triggers reorder event `Reorder:<source>-><target>`.
- **Floating ghost preview & drop indicator**: rendering draws a floating pill card at the cursor
  showing the dragged element label, and a 3px Blue-500 indicator line at the target position.
- **Native OS file drop**: handles `WindowEvent::DroppedFile`, `WindowEvent::HoveredFile`, and
  `WindowEvent::HoveredFileCancelled`, dispatching `file_drop` with visual "Drop file to attach"
  overlay banner.
- **Python front end helpers**: `dispatch_reorder` and `dispatch_file_drop` on `_HeadlessApp`.
- **Verified**: fmt OK, clippy -D warnings clean, **971 passed / 0 failed / 1 ignored**
  (new unit test `test_drag_and_drop_reorder_and_file_drop`). Verified live on X11 with
  `examples/drag_drop_demo.ndg` (window WID 90177538, 480x380): mouse drag from `bubble_1`
  to `bubble_2` swapped messages with 3,177 bright text pixels rendered on screen.
  All 18 demo suites pass `nudgec test`.


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
2. **1.2 Real font** + **1.3 Rounded corners** + **1.1 Alpha** — visual leap — DONE
3. **2.1 `scroll`** — unblocks the chat feed — DONE
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
