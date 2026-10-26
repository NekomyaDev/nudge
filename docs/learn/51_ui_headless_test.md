# 51/60 - Headless UI tests

UI tests do not need a desktop or browser. `mount_headless()` renders a
window in memory, `find(...).exists()` checks stable element IDs, and
`dispatch_click(...)` invokes a handler. This gives ordinary CI a deterministic
way to test UI state transitions.

Try:
  nudgec check ui_headless_test.ndg
  nudgec test ui_headless_test.ndg
