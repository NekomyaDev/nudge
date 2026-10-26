# 4/60 - Confidence policies (route{})

raw model output should not reach users unchecked. `route{}` is a
policy switch: arms carry conditions (confidence thresholds, winner
equality) and an `otherwise` fallback. The compiler unifies the arm
types, so every path returns the same shape.

This is the "safe to deploy" pattern: high confidence → automate,
anything else → a human.

Try:
  nudgec check policy.ndg && nudgec build policy.ndg && python3 out/policy.py
