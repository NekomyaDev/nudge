# 42/60 - route { arm: expr when cond, otherwise: fallback }

`route {}` is a typed policy switch. Each arm has a label, a value expression,
and either `when <condition>` or `otherwise`. Nudge checks that every arm
returns the same type, so your fallback cannot accidentally break callers.

Order conditions from the most specific policy to the broad fallback. Give the
fallback a visible label such as `review` for trace readability.

Try:
  nudgec check route_syntax.ndg
  nudgec build route_syntax.ndg
