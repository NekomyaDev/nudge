# 28/60 - Retry and automatic JSON repair

Structured responses can fail validation because a provider returns malformed
JSON or omits a required field. `retry: 3 with repair` gives the runtime up to
three attempts and asks it to repair invalid structured output before giving
up.

Use repair with schemas, but still make the prompt state every field and its
shape. Repair is a reliability net, not a substitute for a clear contract.

Try:
  nudgec check retry_repair.ndg
  nudgec lint retry_repair.ndg
