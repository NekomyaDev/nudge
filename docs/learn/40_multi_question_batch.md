# 40/60 - batch three questions in one decision call

Put related questions in one `decide` block. Nudge sends the shared state and
all questions as one decision request, reducing latency, cost, and opportunities
for the model to interpret the same evidence inconsistently.

The returned record retains one typed result per question. This is preferable
to three independent calls whenever the questions use the same evidence.

Try:
  nudgec check multi_question_batch.ndg
  nudgec build multi_question_batch.ndg
