# 25/60 - Nested structured-output schemas

A `schema:` makes an LLM call return a typed value instead of unstructured
text. Records may be nested and may contain lists of other records. Describe
every field in the prompt, including the allowed values of enum-like string
fields, so the model and runtime share the same contract.

Nudge's current schema syntax expresses enum-like choices as a string field
with an explicit finite set of values in the prompt. Use a discriminator field
inside a nested record when variants need different payloads.

Try:
  nudgec check structured_schemas.ndg
  nudgec build structured_schemas.ndg
