# Examples

## `research_agent.ndg` — the self-testing research agent (v0.1 acceptance)

The whole agent is **29 lines**. It plans search angles with an LLM call, fans
out web searches in parallel, analyzes each angle against its hits, and merges
everything into a typed `Report` — with a `test` block that replays a recorded
run at zero token cost.

```ndg
type Url = string @format(url)
type Finding = { claim: string, source: Url, confidence: float @range(0, 1) }
type SearchResult = { title: string, url: Url, snippet: string }
type Report = { title: string, findings: [Finding], gaps: [string] }

tool web_search(q: string) -> [SearchResult] { impl: mcp("search").web(q)  side_effects: none }

fn analyze(q: string, hits: [SearchResult]) -> [Finding] uses LLM {
    llm"""Extract verifiable findings about {q} from: {hits}"""
    with { schema: [Finding], model: "anthropic:sonnet-4.6", budget: 0.03 USD, retry: 2 with repair }
}

fn run(question: string) -> Report uses LLM, Tool {
    let angles: [string] = llm"""3 search angles for: {question}"""
        with { schema: [string], budget: 0.005 USD }
    let hits = par map angles |a| -> web_search(a)
    let found = par map(angles zip hits, concurrency = 3) |(a, h)| -> analyze(a, h)
    llm"""Merge these findings: {found}. Question: {question}. List remaining gaps."""
    with { schema: Report, model: "anthropic:sonnet-4.6", budget: 0.04 USD, retry: 3 with repair }
}

test "run stays within budget on recorded trace" {
    let t = replay("traces/demo_run.jsonl")
    assert t.cost_usd < 0.25
    assert len(t.output.findings) >= 3
}
```

### What the compiler proves before anything runs

- `schema: [Finding]` in `analyze` matches its declared `-> [Finding]` return type
  (mismatch → **E0201**).
- `uses LLM, Tool` on `run` covers everything its body does — effects are
  inferred transitively through `analyze` and `web_search` (missing clause →
  **E0301**, too-narrow clause → **E0302**).
- Every interpolated variable (`{question}`, `{found}`, …) exists (**E0101**).
- `budget: 0.03 USD` is in the only v0.1 currency (**E0501** otherwise).

### What the runtime guarantees while it runs

- **Repair loop:** model output that violates `schema` gets its validation
  errors fed back, up to `retry` rounds; exhaustion raises `SchemaFailure`.
- **Budget walls:** each `budget:` caps its call; `NUDGE_BUDGET=<usd>` caps the
  whole run — a counter shared across all `par` branches, so an in-flight
  branch raises `BudgetExceeded` the moment the wall is hit.
- **Trace:** every call and every effectful return lands in a JSONL trace
  (`v: 1` records), complete up to any crash point.

### The test block — traces as tests

`traces/demo_run.jsonl` is a real recorded run (5 LLM calls, $0.005 at the
deterministic fake price of $0.001/call). The test replays it and asserts on
cost and output shape. Replaying burns **zero** tokens — that is the CI story.

### Try it

```sh
# from the repo root — static verification first
nudgec check examples/research_agent.ndg      # type + effect verification
nudgec build examples/research_agent.ndg      # emit Python to out/

cd examples
PYTHONPATH=../runtime nudgec test research_agent.ndg   # replay the canned trace, run asserts

# drive a run yourself (fake provider, zero tokens)
PYTHONPATH=../runtime python3 -c "import sys; sys.path.insert(0, '../out')
import research_agent as r; print(r.run('urban heat islands'))"

# full replay of the committed trace — no provider calls at all
NUDGE_REPLAY=traces/demo_run.jsonl PYTHONPATH=../runtime python3 -c "import sys; sys.path.insert(0, '../out')
import research_agent as r; print(r.run('urban heat islands'))"

# the budget wall, live: 5 calls × $0.001 fake price vs a $0.0025 run budget
NUDGE_BUDGET=0.0025 PYTHONPATH=../runtime python3 -c "import sys; sys.path.insert(0, '../out')
import research_agent as r; print(r.run('urban heat islands'))"
```

All of the above runs against the deterministic **fake provider** (the default,
`NUDGE_PROVIDER=fake`): it synthesizes schema-conforming values — 3 items per
list, mid-range numbers — so every command works with no API key and no token
spend. Real providers land post-MVP.

### v0.1 acceptance criteria

| Criterion | Status |
|---|---|
| under 30 lines | ✅ 29 lines |
| zero manual JSON parsing | ✅ schemas do all the work |
| replay test passing at zero token cost | ✅ `nudgec test` replays the committed trace |

## `checkpoint_agent.ndg` — agent state that survives crashes (v0.2c)

An `agent` block declares typed `state` with defaults; every `state.x = v` /
`state.x += v` write is an **automatic checkpoint** to
`.nudge/runs/<run_id>/checkpoint.json`. After a crash, `nudge resume <run_id>`
re-executes the program replaying the recorded trace prefix (zero tokens for
work already done), then continues live — and the replayed state writes are
suppressed, so nothing is applied twice (design §7).

```ndg
agent Researcher {
    state {
        notes: [string] = [],
        round: int = 0,
    }

    fn step(q: string) -> string uses LLM {
        let note = llm"""One short research note about: {q}""" with { model: "anthropic:sonnet-4.6", budget: 0.01 USD }
        state.notes += [note]
        state.round += 1
        note
    }

    fn main() -> [string] uses LLM {
        step("urban heat islands")
        step("green roofs")
        state.notes
    }
}
```

### Try it

```sh
nudgec check examples/checkpoint_agent.ndg     # state writes verified (E0701 outside an agent)
nudgec build examples/checkpoint_agent.ndg

# a run that crashes mid-way: 2 calls × $0.001 fake price vs a $0.0015 budget
cd examples
NUDGE_RUN_ID=demo-1 NUDGE_BUDGET=0.0015 PYTHONPATH=../runtime python3 ../out/checkpoint_agent.py
# → BudgetExceeded; .nudge/runs/demo-1/checkpoint.json holds round: 1

# resume: replays the recorded call, then goes live — round ends at exactly 2
nudgec resume demo-1
```

## `hello_llm.ndg` — the smallest possible program

```ndg
fn main() -> string uses LLM {
    llm"""Say hello to the Nudge language in one short sentence."""
    with { model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}
```

```sh
nudgec build examples/hello_llm.ndg
PYTHONPATH=runtime python3 out/hello_llm.py
```
