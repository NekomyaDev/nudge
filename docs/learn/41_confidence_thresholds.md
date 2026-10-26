# 41/60 - filter with p and confidence thresholds

Decision results distinguish probability from confidence. For a `choose`
answer, `p` is the selected winner's probability and `confidence` measures
how decisively the model separated that answer from alternatives. A safe policy
usually requires both.

Use ordinary expressions to filter: only automate when the selected category
and confidence satisfy your policy. Otherwise return a conservative fallback.

Try:
  nudgec check confidence_thresholds.ndg
  nudgec build confidence_thresholds.ndg
