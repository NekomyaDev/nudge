# 39/60 - score [low, medium, high] and weighted scores

`score [low, medium, high]` asks the decision model to grade a state against
an ordered rubric. The response exposes a numeric `score` and a probability
`distribution` over the rubric labels.

You can combine the numeric score with ordinary deterministic weights. The
model supplies judgment; your program owns the final business arithmetic.

Try:
  nudgec check score_questions.ndg
  nudgec build score_questions.ndg
