# 9/60 - Variables and mutable state

`let` creates an immutable local binding. Add `mut` only when a value
must be reassigned with `=`. This keeps ordinary data flow explicit and lets
the checker reject accidental mutation.

Try:
  nudgec check variables_mut.ndg
  nudgec build variables_mut.ndg
