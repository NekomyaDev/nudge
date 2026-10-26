# 59/60 - Trace diffs as CI regression gates

Compare a trusted baseline trace to a candidate trace with
`nudgec trace-diff baseline.ntf.jsonl candidate.ntf.jsonl --fail-on-regression`.
The command turns growth in cost, token use, latency, or failures into a
non-zero exit code, so CI can reject an agent regression before deployment.

Try:
  nudgec check trace_diff_ci.ndg
  nudgec trace-diff baseline.ntf.jsonl candidate.ntf.jsonl --fail-on-regression
