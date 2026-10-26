# 56/60 - Property fuzzing and shrinking

`for_all` executes a property over generated input. `gen.str(64)` supplies
strings up to 64 characters, including useful edge cases. If an assertion
fails, Nudge shrinks the original input toward a smallest counterexample, which
makes a failing case practical to add as a regression test.

Try:
  nudgec check property_fuzzing.ndg
  nudgec test property_fuzzing.ndg
