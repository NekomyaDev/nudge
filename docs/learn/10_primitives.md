# 10/60 - Primitive types

Nudge has `int`, `float`, `string`, `bool`, and `none` (the unit-like
no-value type). `()` is also accepted in type position as an alias for `none`.
Numeric operations widen `int` to `float` when necessary. There are no implicit
string-to-number cast builtins yet, so make conversions at an input boundary.

Try:
  nudgec check primitives.ndg
