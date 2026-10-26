# 48/60 - Agent state and durable mutations

An `agent` groups durable state with the operations allowed to mutate it.
The `state { ... }` declaration gives every field both a type and a deterministic
initial value. Inside agent methods, `state.field = value` and `+=` are tracked
state writes, so the runtime can checkpoint them for recovery and replay.

Try:
  nudgec check agent_state.ndg
  nudgec test agent_state.ndg
