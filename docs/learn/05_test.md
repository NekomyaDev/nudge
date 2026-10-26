# 5/60 - $0 tests and property fuzzing

test blocks run against the fake provider: deterministic, $0, no API
keys — your whole suite runs on every save.

`for_all` goes further: it loops a property over generated cases (edge
values + a seeded sweep) and *shrinks* any failure to a minimal
counterexample you can paste into a regression assert. gen.injection()
feeds adversarial prompt-injection strings into pure code.

Try:
  nudgec test test.ndg
