# 2/60 - Typed outputs (schemas)

Free-text LLM output is a bug factory. In Nudge you declare a
record type and pin it with `schema:` — the runtime validates every
response against the type and repairs or rejects mismatches.

The prompt must describe the contract in words too (the compiler checks
that schema fields actually appear in the prompt — Prompt Clippy W0003).

Try:
  nudgec check types.ndg && nudgec build types.ndg
