# 38/60 - yes/no questions and probability p

Use `yes/no` for binary propositions: fraud signals, cancellation risk, or
whether an action needs review. The answer has one field, `p`, the estimated
probability that the answer is yes. It is a probability, not a guarantee.

Compare `p` with a policy threshold before automating an irreversible action.

Try:
  nudgec check yesno_questions.ndg
  nudgec build yesno_questions.ndg
