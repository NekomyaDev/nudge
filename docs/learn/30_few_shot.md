# 30/60 - Few-shot prompting and context conditioning

Few-shot prompts teach a format by showing representative input/output pairs
inside the prompt. Choose small, diverse examples that demonstrate the rule,
then clearly mark the new input. This is context conditioning, not training:
the examples only affect this request.

Keep examples relevant and compact because they consume context and cost.
Use a schema when the result must be machine-consumable.

Try:
  nudgec check few_shot.ndg
  nudgec build few_shot.ndg
