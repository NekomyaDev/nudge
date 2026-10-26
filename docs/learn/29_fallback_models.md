# 29/60 - Fallback models for failures and timeouts

A fallback model keeps a bounded task available when the primary provider
fails or times out. Declare the primary in `model` and the secondary in
`fallback`. Keep both prompts and schema-compatible expectations identical.

A fallback is not unlimited retrying: retain a budget, a response-size cap,
and a schema so the degraded path is safe and predictable too.

Try:
  nudgec check fallback_models.ndg
  nudgec build fallback_models.ndg
