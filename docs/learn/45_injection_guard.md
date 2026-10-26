# 45/60 - gen.injection() and prompt-injection defenses

Treat decision state as untrusted input. Keep system policy in the question
prompt, keep the user-controlled text only in `on state`, and test that hostile
strings do not bypass deterministic safeguards.

`gen.injection()` provides a corpus of adversarial prompt-injection strings for
offline property tests. Property bodies must remain pure and token-free, so the
test below validates the deterministic trust boundary while `guard` performs
the separately traced decision call.

Try:
  nudgec check injection_guard.ndg
  nudgec test injection_guard.ndg
