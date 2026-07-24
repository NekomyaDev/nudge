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

## North Stars — the giant outcomes (5–10 year)

The doors make Nudge indispensable; these are what indispensability can
compound into. All six grow from the same root — the frozen trace, the
proofs, the standard.

1. **The Machine-Verifiable Language** 🤖 — most software will soon be
   written by AI, and the winning language is the one whose output AI can
   *prove things about*. Types + effects + capabilities + budget proofs make
   Nudge the verifiable compilation target of the AI era — the JVM of
   AI-generated software. Door 3 grown to full scale.
2. **The agent internet's native tongue** 🌐 — when the A2A/agent-web
   standards solidify, Nudge agents are native citizens: cards, serving,
   Hub. The JavaScript of the agent web.
3. **The certified-agent industry** 📜 — behavioral proofs beyond
   capabilities ("this agent can never do X") become what insurers and
   regulators demand. Nudge is the notary: proof format + verifier, the
   technical reference implementation for AI-Act-style regulation.
4. **The agent economy** 💸 — budgets are already a language construct;
   the next step is agents hiring agents with compiler-proven spend limits
   as escrow. Nudge's budget system becomes the money protocol of the agent
   economy.
5. **The classroom** 🎓 — The Nudge Book + playground + open curriculum:
   a generation of developers learns agents in Nudge first. Permanence's
   deepest form.
6. **The foundation** 🏛️ — the endgame: an independent Nudge Foundation,
   corporate sponsors, NTF on an ISO/W3C standards track. Nudge outgrows
   its author — the truest definition of success.

*Sequencing honesty: every star hangs on the door chain — trace freeze →
standard → proofs → economy → foundation. The first link already sits in
the repo.*

## The idea backlog (accepted, awaiting a slot)

Ideas ratified into the strategy; each lands in a version when its door
opens.

- **Open Trace Commons** 🌍 — the "ImageNet of agent behavior": opt-in,
  anonymized NTF traces donated to a public dataset. Researchers write
  papers on it (citations flow back), benchmarks feed from it, and the NTF
  standard entrenches itself by use. Community + academia + standard in one
  move. *(Feeds Doors 2–4; pairs with the web playground.)*
- **Prompt Clippy** 🔍 — the compiler lints `llm"""` blocks: vague
  instruction, missing output contract, schema fields never mentioned in the
  prompt, prompt too long for the declared budget. Prompt engineering as a
  compiler concern — nobody does this; small build, huge visibility.
  *(Door 1; an early v1.2/v1.3 candidate.)*
- **VCR-world simulation** ✈️ — record tool responses once (the trace
  already does), replay the whole world: develop agents offline,
  deterministically, at zero cost, with always-green CI. "VCR for the web."
  *(Doors 1–2; a natural extension of the replay engine.)*
- **`nudgec evolve`** 🧬 — semantic diffs between agent *versions*, not just
  runs: "v3 shortened the prompt, moved to the strong model, cost −40%,
  accuracy flat." Behavioral change-tracking beyond version control.
  *(Door 5's visible face.)*
