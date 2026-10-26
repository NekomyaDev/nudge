# 57/60 - Open NTF traces

Set `NUDGE_TRACE=<path>` before running generated code to record calls and
outcomes as NTF JSONL. NTF is an open, versioned trace schema: inspect it with
ordinary tooling, validate it with `nudgec trace-check`, and retain it as a
portable test artifact.

Try:
  nudgec check ntf_traces.ndg
  nudgec build ntf_traces.ndg
  NUDGE_TRACE=run.ntf.jsonl python3 out/ntf_traces.py
  nudgec trace-check run.ntf.jsonl
