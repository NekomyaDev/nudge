# 46/60 - cache repeated decisions with NUDGE_DECISION_CACHE

A decision cache prevents paying twice for the same decision input and model
configuration. Set `NUDGE_DECISION_CACHE` to a cache path before running the
compiled program. Repeated calls with the same state and options reuse the
stored answer.

Caching improves cost and latency, but cache keys should only cover stable,
non-secret inputs. Invalidate or namespace caches when policy prompts change.

Try:
  nudgec check decision_caching.ndg
  nudgec build decision_caching.ndg
  NUDGE_DECISION_CACHE=.nudge-decisions.json python3 out/decision_caching.py
