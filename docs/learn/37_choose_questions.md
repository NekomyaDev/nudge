# 37/60 - choose [a, b, c]: classifying with a winner

A `choose` question classifies the state into one of the listed labels. Its
answer includes `winner`, the selected label, plus `p`, `distribution`, and
`confidence`. Read `winner` to route work, but retain the distribution for
auditing and later threshold calibration.

Labels can be bare identifiers or quoted strings when a label has spaces.

Try:
  nudgec check choose_questions.ndg
  nudgec build choose_questions.ndg
