# 3/60 - Typed decisions (decide{})

`decide{}` asks a *decision model* (the JEV family: Laya, Jev, Valen)
typed questions about a state — one batched call, answers with
probabilities and confidence. A decision is an effect like LLM: tracked,
traced, replayable.

The fake decision provider synthesizes deterministic distributions, so
this too is $0 offline.

Try:
  nudgec check decide.ndg && nudgec build decide.ndg
  python3 out/decide.py
