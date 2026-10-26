# 47/60 - audit decisions with nudgec explain

Decision traces make automation inspectable. Run a program with `NUDGE_TRACE`
and then ask `nudgec explain` for a human-readable report of the prompts,
answers, probabilities, policy arms, and costs recorded in that trace.

Use reports in incident review and policy changes: the goal is to show not only
what action was selected, but the evidence, confidence gate, and fallback path
that produced it.

Try:
  nudgec check explain_reports.ndg
  NUDGE_TRACE=decision.jsonl nudgec run explain_reports.ndg
  nudgec explain decision.jsonl
