# 34/60 - System prompts and role management

Keep durable behavioral rules separate from per-request user data. The
`system` option carries role and safety instructions, while the `llm` block
contains the task-specific input. This separation makes reviews easier and
prevents user text from silently redefining the assistant's role.

State boundaries positively: describe what the model should do, what format it
must return, and which instructions are authoritative.

Try:
  nudgec check system_prompts.ndg
  nudgec build system_prompts.ndg
