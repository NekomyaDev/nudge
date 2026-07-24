# Nudge Strategy — Six Locked Doors

*Why Nudge exists, and the order in which it becomes indispensable.*

The agent industry in 2026 ships demos easily and production systems barely.
Six problems block serious adoption; nobody owns good answers to any of them.
Nudge's plan is to take these doors one at a time — each unlock makes Nudge
indispensable to a wider audience, and each door compounds the previous ones.

## The six doors

### Door 1 — Debuggability: *"Why did my agent do that?"*
Nobody can answer this today; people debug agents with `print`.
**Our move:** traces (shipped) → replay (shipped) → DAP time-travel
debugger + trace viewer.
**Unlock:** "if an agent needs debugging, it's written in Nudge."
*Audience: every agent developer. Status: ~70% built — the nearest door.*

### Door 2 — Testability: *"I changed a prompt and everything broke."*
Agents have no CI; regressions are found by users in production.
**Our move:** NTF open trace standard → `nudgec trace-diff` → property-based
fuzzing → the `nudge-ci` GitHub Action.
**Unlock:** agent CI becomes a category, and it is named Nudge.
*Audience: teams and internal platforms.*

### Door 3 — Safety: *"I can't put an agent in prod; it will get injected."* 🔥
The industry's biggest blocker. Serious companies cannot deploy agents
because tools are unprotected and "please don't do bad things" prompts are
not a defense.
**Our move:** capability-based tool security — compiler-proven tool
isolation. Injected instructions cannot invoke calls that were never granted;
the call graph is proven, not hoped for.
**Unlock:** "agents that go to production are written in Nudge."
*This door is the moat — the hardest to cross, the most valuable to hold.
It becomes the language thesis: the language where agents are safe to deploy.*

### Door 4 — Compliance: *"The auditor asked; I can't prove anything."*
The EU AI Act is in force; enterprise procurement asks "where are the logs?"
and hears silence.
**Our move:** NTF traces as audit evidence, `nudgec audit` reports,
AI-Act-ready positioning.
**Unlock:** Nudge enters procurement checklists — and checklist technology is
never un-installed. *Audience: enterprise.*

### Door 5 — Improvement: *"My agent works; how does it get better?"*
Prompt tuning is artisanal — no measurement, no systematic gains.
**Our move:** the `optimize` block (budget-bounded, type-safe search over
prompts/models) + cross-run learning from historical traces + a published,
reproducible benchmark.
**Unlock:** Nudge agents compound in quality; others stay frozen.

### Door 6 — Lock-in: *"Everything depends on one vendor."*
Frameworks die or rewrite; model prices swing; vendors pivot.
**Our move:** ejectable codegen (shipped), provider-neutral adapters
(shipped), Agent Hub + A2A serving.
**Unlock:** adopting Nudge is not a risk — it is the insurance policy.

## The compounding logic

Debug → Test → Safe → Compliant → Improving → Inevitable.
Each door feeds the next: teams that debug want CI; teams with CI want safe
deploys; deployed systems need audits; audited systems invest in improvement;
long-lived systems fear lock-in. Indispensability compounds.

## Honest constraints

- We are a tiny team. Sequencing is the strategy: doors 1–2 are nearly
  unlocked; door 3 is the next major engineering thesis; doors 4–6 ride on
  the standard and the community built along the way.
- No SaaS, no token, no lock-in of our own. Nudge wins as a standard and a
  toolchain — MIT, forever.
