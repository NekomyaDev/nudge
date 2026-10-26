# 16/60 - Optional values and error-shaped data

The current optional type syntax is `T?`, the checked equivalent of an
`Option<T>`. It accepts either a value or `none`; use `if` to select a fallback.
Generic `Option<T>`, `Result<T, E>`, `unwrap`, and `if let` are planned language
features. Until then, model rich success or failure data with a named record.

Try:
  nudgec check options_results.ndg
