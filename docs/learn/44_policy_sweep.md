# 44/60 - calibrate thresholds offline with policy-sweep

Thresholds are product policy, not magic constants. First record decision
traces, then use `nudgec policy-sweep` to compare candidate thresholds offline.
This replays existing distributions and makes zero new model calls.

For example, sweep a routing question at several confidence thresholds before
changing the value used by a deployed `route {}` policy.

Try:
  NUDGE_TRACE=triage.jsonl python3 out/policy_sweep.py
  nudgec policy-sweep triage.jsonl --question team --thresholds 0.60,0.75,0.85
  nudgec check policy_sweep.ndg
