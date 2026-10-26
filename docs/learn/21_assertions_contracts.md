# 21/60 - Assertions and lightweight contracts

`assert` checks an invariant in a function or test. Use it near boundaries
to document assumptions and protect important properties. Dedicated
`precondition` and `postcondition` keywords are not currently in the grammar,
so assertions are the portable contract mechanism.

Try:
  nudgec check assertions_contracts.ndg
  nudgec test assertions_contracts.ndg
