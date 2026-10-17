# Confidence-Aware Types and Gradual Verification (Roadmap v1.5 - Door 3: Safe)

## 1. Executive Summary

In traditional software, a value of type `T` is a guaranteed, deterministic instance of `T`. In AI-native agent applications, outputs returned by Large Language Models and external tools are probabilistic, heuristic, and inherently unverified. Passing raw probabilistic outputs directly into critical sinks (file systems, database migrations, shell commands, financial transactions, or UI computer control) is the central vulnerability of modern AI agents.

Nudge addresses this problem at the language level rather than through ad-hoc runtime checks or prompt pleas. Under the "Door 3: Safe" roadmap commitment, Nudge introduces **Confidence-Aware Types** and **Gradual Verification**:
- Values track a strict verification progression: `unverified -> validated -> verified`.
- Calibrated confidence qualifications allow fine-grained thresholds (e.g. `@confidence(0.85)`).
- A native `refine x until <condition>` loop construct orchestrates bounded retry and improvement loops while respecting effect declarations (`uses LLM`), trace replay, and budget walls.
- Statically proven subtyping prevents unverified data from reaching sensitive function sinks without proper promotion.

---

## 2. The Verification Continuum

Nudge defines three formal verification stages for any value:

```
+---------------+      Schema repair / assertions      +---------------+      Deterministic check / human      +---------------+
|  unverified   | -----------------------------------> |   validated   | ------------------------------------> |   verified    |
| (probabilistic|        decide {} confidence          | (structurally |        sandbox test / auth proof      | (deterministic|
|  raw output)  |                                      |  sound data)  |                                       |  guarantee)   |
+---------------+                                      +---------------+                                       +---------------+
```

### 2.1 Unverified (`unverified`)
- **Semantics**: Untrusted or probabilistic output. The data exists in memory, but its semantics, correctness, and safety invariants have not been proved.
- **Origin**: Raw output of `llm"""..."""` calls, unauthenticated tool responses, web scraping results, and external user input.
- **Restrictions**: Cannot be passed to sensitive functions marked as requiring `validated` or `verified` arguments.

### 2.2 Validated (`validated`)
- **Semantics**: Conforms to structural schema rules and semantic assertions.
- **Origin**: Output of an LLM call with `schema: T, retry: 3 with repair` after successful JSON-schema validation, or output that has passed a calibrated decision gate (`decide {}`) meeting a confidence threshold.
- **Restrictions**: Safe for domain logic, reading, and non-destructive transformations. Inadmissible for irreversible or privileged operations that require deterministic proof.

### 2.3 Verified (`verified`)
- **Semantics**: Deterministically proved to satisfy core invariants or formally authorized by an external ground truth.
- **Origin**: Output verified by a pure deterministic validator (such as a compiler pass, a unit test suite executed in a sandbox, a cryptographic signature check, or a database foreign-key proof) or approved via an interactive human auth gate.
- **Restrictions**: Permitted at all sinks, including privileged system operations (`run`, destructive tool writes, privileged financial/auth effects).

---

## 3. Surface Syntax

Nudge provides two complementary syntactic representations: refinement qualifiers and generic stage constructors.

### 3.1 Refinement Qualifiers (`@confidence` and `@stage`)

Refinements attach directly to base types or aliases via the `@` symbol:

```nudge
// Numeric confidence threshold (between 0.0 and 1.0)
type HighConfidenceScore = float @confidence(0.9)
type CandidatePlan = Plan @confidence(0.8)

// Explicit verification stage
type RawPlan = Plan @stage(unverified)
type CheckedPlan = Plan @stage(validated)
type ApprovedPlan = Plan @stage(verified)
```

For convenience, shorthand stage qualifiers are supported:
- `Plan @unverified`
- `Plan @validated`
- `Plan @verified`

### 3.2 Generic Stage Constructors

Generic syntax offers an expressive functional representation:

```nudge
Unverified[T]  // Equivalent to T @stage(unverified)
Validated[T]   // Equivalent to T @stage(validated)
Verified[T]    // Equivalent to T @stage(verified)
Confidence[T]  // Value paired with its calibrated confidence score
```

Example usage in type declarations and function signatures:

```nudge
type Migration = {
    sql: string,
    destructive: bool,
}

// Function requires proven safety before execution
fn execute_migration(m: Verified[Migration]) -> () uses IO, Tool {
    ...
}
```

---

## 4. Subtyping and Type Promotion Rules

### 4.1 Subtyping Lattice

Verification stages form a subtyping lattice where higher verification levels can always satisfy requirements for lower levels:

$$\text{Verified}[T] \ <: \ \text{Validated}[T] \ <: \ \text{Unverified}[T]$$

In words:
- Every `Verified[T]` is sound to use where `Validated[T]` or `Unverified[T]` is expected.
- Every `Validated[T]` is sound to use where `Unverified[T]` is expected.
- An `Unverified[T]` is **NOT** assignable to a parameter expecting `Validated[T]` or `Verified[T]`.
- A `Validated[T]` is **NOT** assignable to a parameter expecting `Verified[T]`.

For numeric confidence qualifications:
- `T @confidence(c1)` is a subtype of `T @confidence(c2)` if and only if $c1 \ge c2$.
- Example: `Plan @confidence(0.95)` is assignable to `Plan @confidence(0.80)`.
- Example: `Plan @confidence(0.70)` passed to `Plan @confidence(0.85)` yields a compile-time type mismatch error.

### 4.2 Promotion Mechanisms

A value transitions between stages through verifiable promotion operations:

#### 1. Schema Repair Promotion (`unverified -> validated`)
When an LLM call specifies a schema with repair:
```nudge
let plan: Validated[Plan] = llm"""...""" with {
    schema: Plan,
    retry: 3 with repair,
}
```
If the model output conforms to the schema (or successfully repairs), the result is automatically promoted from `Unverified[Plan]` to `Validated[Plan]`.

#### 2. Decision Model Gate (`unverified -> validated`)
Calibrated decision questions evaluate the candidate:
```nudge
let audit = decide {
    safe: "Is this SQL plan non-destructive?" choose ["yes", "no"]
} on plan with { model: "jev:decision-v1" }

if audit.safe.winner == "yes" and audit.safe.confidence >= 0.85 {
    // Inside this branch, plan is flow-promoted to Validated[Plan]
    // or Plan @confidence(0.85)
}
```

#### 3. Deterministic Verification Gate (`validated -> verified`)
Deterministic checks verify execution invariants:
```nudge
fn test_in_sandbox(m: Validated[Migration]) -> bool uses Tool {
    // Pure or sandboxed verification tool
    ...
}

if test_in_sandbox(m) {
    // Inside this branch, m is promoted to Verified[Migration]
    execute_migration(m) // Type-checks cleanly!
}
```

---

## 5. Iterative Refinement: `refine x until <condition>`

In autonomous agents, generating high-confidence output rarely succeeds in a single zero-shot step. Agents need an iterative feedback loop: draft, critique, adjust, repeat until confidence or safety thresholds are reached.

### 5.1 Syntax

```nudge
refine candidate until confidence >= 0.85 {
    let critique = decide {
        clarity: "Does this proposal answer all user requirements?" choose ["yes", "no"],
        safety: "Are there security risks?" choose ["yes", "no"]
    } on candidate

    confidence = critique.clarity.confidence

    if confidence < 0.85 {
        candidate = llm"""
            Revise proposal {candidate} based on critique:
            Clarity score: {critique.clarity.confidence}
            Safety verdict: {critique.safety.winner}
        """ with {
            schema: Proposal,
            retry: 2 with repair
        }
    }
} max_rounds: 4 fallback: default_proposal
```

### 5.2 Effect Interaction

The `refine` construct is transparent with respect to effects:
- If the body of the `refine` loop issues `llm"""..."""` calls, the enclosing function **MUST** declare `uses LLM`.
- If the body issues `decide { ... }` calls, the enclosing function **MUST** declare `uses Decision`.
- Omitting these effects produces the standard Nudge effect errors (`E0301` or `E0302`).

### 5.3 Budget Walls and Termination Guarantees

AI retry loops must never turn into infinite money-burning cycles:
1. **Explicit Round Cap**: The `max_rounds: N` clause provides an absolute static bound (defaults to 3 if omitted).
2. **Cumulative Budget Wall**: Every round charges against the call-site budget and run-level budget. If the budget exhausts during round 2, the runtime raises `BudgetExceeded` immediately.
3. **Fallback Handling**: If `max_rounds` is exhausted without reaching the `until` condition, the loop assigns the optional `fallback` expression. If no fallback is declared, runtime raises `RefinementFailure`.

### 5.4 Tracing and Deterministic Replay

Under Nudge's replay contract:
- Every iteration of the `refine` loop logs a trace record with `refine_round: n`, the evaluated condition expression, and the confidence score.
- During `NUDGE_REPLAY`, the loop replays from recorded history with zero model cost and identical deterministic state transitions.

---

## 6. Compile-Time Diagnostics

When an unverified or insufficient-confidence value is passed to a sink requiring a higher verification level, the compiler emits clear diagnostics:

### 6.1 Diagnostic Codes

- **E0201 (Type Mismatch - Verification Level Deficit)**:
```text
error[E0201]: verification level deficit
  --> src/agent.ndg:42:24
   |
42 |     execute_migration(draft_plan)
   |                       ^^^^^^^^^^ expected 'Verified[Migration]', found 'Unverified[Migration]'
   |
   = help: value is unverified LLM output; promote it using a deterministic validator, or refine it:
           'if verify_migration(draft_plan) { execute_migration(draft_plan) }'
```

- **E0201 (Confidence Threshold Deficit)**:
```text
error[E0201]: confidence requirement not satisfied
  --> src/agent.ndg:58:19
   |
58 |     deploy_config(candidate)
   |                   ^^^^^^^^^ expected '@confidence(>= 0.85)', but 'candidate' has '@confidence(0.70)'
   |
   = help: confidence is below function threshold; refine until confidence >= 0.85
```

- **E0202 (Malformed Confidence Refinement)**:
```text
error[E0202]: @confidence expects a numeric bound between 0.0 and 1.0, e.g. @confidence(0.8) - got 1.5
  --> src/types.ndg:12:21
   |
12 | type BadPlan = Plan @confidence(1.5)
   |                     ^^^^^^^^^^^^^^^
```

---

## 7. Concrete End-to-End Example

```nudge
type ActionProposal = {
    action_type: string,
    target: string,
    payload: string,
}

// Low-privilege drafting agent
fn draft_action(goal: string) -> Unverified[ActionProposal] uses LLM {
    llm"""Draft action for goal: {goal}""" with {
        model: "openai:gpt-4.5-mini",
        schema: ActionProposal,
    }
}

// Deterministic safety validator
fn check_target_whitelist(action: Validated[ActionProposal]) -> bool {
    action.target == "staging-server" or action.target == "dev-db"
}

// High-privilege executor sink
fn execute_action(action: Verified[ActionProposal]) -> () uses Computer, IO {
    computer.observe("terminal")
    // Safe to execute: compiler has proved action is verified
}

fn run_pipeline(goal: string) -> () uses LLM, Decision, Computer, IO {
    let mut proposal = draft_action(goal)
    let mut conf = 0.0

    // Iterative refinement until decision confidence threshold is met
    refine proposal until conf >= 0.90 {
        let eval = decide {
            safe: "Is this action safe and aligned with policy?" choose ["yes", "no"]
        } on proposal

        conf = eval.safe.confidence
        if conf < 0.90 {
            proposal = llm"""Fix proposal {proposal} to satisfy safety criteria""" with {
                schema: ActionProposal,
                retry: 2 with repair
            }
        }
    } max_rounds: 3

    // Proposal is now Validated[ActionProposal] with confidence >= 0.90
    if check_target_whitelist(proposal) {
        // Promoted to Verified[ActionProposal]
        execute_action(proposal)
    }
}
```

---

## 8. Compiler Foundation & Roadmap Phasing

1. **Phase 1 (Foundation - v1.5 milestone)**:
   - Recognize `@confidence(min)` in `crates/nudgec/src/check.rs` validation.
   - Recognize generic verification stages `Unverified[T]`, `Validated[T]`, `Verified[T]`, `Confidence[T]`.
   - Implement subtyping and assignability checks in `assignable()` in `check.rs`.
   - Prevent invalid confidence ranges (e.g. `< 0.0` or `> 1.0`).

2. **Phase 2 (Flow-sensitive Promotion)**:
   - Branch narrowing on `if` conditions testing confidence comparisons or validator function outcomes.

3. **Phase 3 (Language Syntax & Lowering)**:
   - Grammar support for `refine ... until` loop statement in `crates/nudgec/src/parser.rs`.
   - Runtime tracing and replay integration in `nudge-runtime`.
