# 53/60 - A2A Agent Cards, capabilities, and skills

`nudgec a2a agent.ndg` derives an `agent.json` card from your agent
structure. The card advertises identity, capabilities, stateful skills, and
the typed methods peers may invoke. Keep exposed methods narrow and typed so
other agents can discover precisely what is safe to call.

Try:
  nudgec check a2a_agent_card.ndg
  nudgec a2a a2a_agent_card.ndg
