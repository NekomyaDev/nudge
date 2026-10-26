# 55/60 - Deterministic fakes: offline and zero cost

Use `model: "fake:default"` when developing and testing. The deterministic
provider gives repeatable responses without API keys, network access, or spend.
It is the default testing foundation for fast local and CI feedback.

Try:
  nudgec check deterministic_fakes.ndg
  nudgec test deterministic_fakes.ndg
