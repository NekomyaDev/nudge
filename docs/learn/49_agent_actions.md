# 49/60 - Agent actions and effect labels

An agent method is written exactly like a function, but it can read and
mutate that agent's state. Its `uses` list remains part of the public contract:
`UI` covers interface actions, `LLM` covers model calls, and other effects are
similarly visible to the compiler and callers.

Try:
  nudgec check agent_actions.ndg
  nudgec build agent_actions.ndg
