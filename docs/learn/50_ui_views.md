# 50/60 - Typed views with columns, rows, and controls

A `view` returns `View` and is composed from typed primitives. `column` and
`row` organize a tree; `text`, `button`, and `input` carry visible content,
stable IDs, and event-handler names. Keeping UI in Nudge means its state and
handlers share the same types as the rest of the agent.

Try:
  nudgec check ui_views.ndg
  nudgec build ui_views.ndg
