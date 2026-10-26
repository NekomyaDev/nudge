# 27/60 - Selecting and routing models

The `model` option names a provider and model with `provider:model`. This
makes routing explicit and keeps provider decisions close to the task:
`anthropic:sonnet-4.6`, `openai:gpt-4o`, and `fake:default` are typical
examples.

Use `fake:default` for deterministic offline development and tests. Switch to
a production model only when the call is ready to use real credentials and
spend real budget.

Try:
  nudgec check model_routing.ndg
  nudgec build model_routing.ndg
