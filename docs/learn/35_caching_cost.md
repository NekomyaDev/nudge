# 35/60 - Prompt caching and cost optimization

Large stable instructions can be cached by providers that support prompt
caching. For Anthropic-compatible calls, `cache_control: "ephemeral"` marks
reusable prompt context. Pair it with a stable system prompt and keep changing
user data in the request body.

Caching reduces repeated-input cost and latency, but it does not replace a
per-call budget. Measure the effect with traces and static cost reports.

Try:
  nudgec check caching_cost.ndg
  nudgec cost caching_cost.ndg
