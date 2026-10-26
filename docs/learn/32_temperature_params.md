# 32/60 - Temperature, top_p, and seed parameters

Sampling parameters control output variability. Lower `temperature` favors
repeatable, conservative wording. `top_p` limits sampling to the most likely
probability mass. A `seed` requests reproducibility from providers that
support deterministic sampling.

Use low-variance settings for extraction and classification. Use higher
creativity only when variation is the point, while retaining a schema and
budget for safety.

Try:
  nudgec check temperature_params.ndg
  nudgec build temperature_params.ndg
