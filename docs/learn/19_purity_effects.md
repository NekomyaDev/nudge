# 19/60 - Purity and tracked effects

A function without `uses` is pure. Add `uses LLM`, `uses Decision`, or
other declared effects when it performs that operation. Effects propagate
through calls, so callers must declare the effects they expose. Keep pure
formatting and validation helpers separate from effectful boundaries.

Try:
  nudgec check purity_effects.ndg
