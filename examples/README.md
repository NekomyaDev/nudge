# Nudge examples

This directory contains the supported Nudge examples and their replay fixtures.
Every `.ndg` source file listed below is checked in CI with:

```sh
cargo run -p nudgec -- check examples/<path>.ndg
```

The examples compile to Python with `nudgec build`. Replay-backed tests run
without API keys. Live LLM, MCP, or computer-automation runs require the
corresponding provider or service configuration described below.

## Supported examples

| Example | Path | Demonstrates | External service for live runs |
| --- | --- | --- | --- |
| Hello LLM | [`hello_llm.ndg`](hello_llm.ndg) | Minimal typed LLM call and budget | LLM provider, or fake provider |
| Chatbot | [`chatbot/`](chatbot/) | Conversation memory, schema output, replay | LLM provider |
| Code reviewer | [`code-reviewer/`](code-reviewer/) | Structured review findings and scoring | LLM provider |
| Support classifier | [`classifier/`](classifier/) | `route{}`, `par map`, typed classification | LLM provider |
| Data analyzer | [`data-analyzer/`](data-analyzer/) | Structured insights and recommendations | LLM provider |
| Translator | [`translator/`](translator/) | Typed translation and parallel mapping | LLM provider |
| RAG agent | [`rag-agent/`](rag-agent/) | MCP retrieval, grounded answers, refusal | MCP knowledge-base server and LLM provider |
| Research agent | [`research_agent.ndg`](research_agent.ndg) | MCP search, parallel research, replay | MCP search server and LLM provider |
| Checkpoint agent | [`checkpoint_agent.ndg`](checkpoint_agent.ndg) | Stateful agents and checkpoint/resume | LLM provider for live runs |
| Triage agent | [`triage-agent/`](triage-agent/) | `decide{}`, confidence policy routing | None for fake-provider tests |
| Computer automation | [`computer-automation/`](computer-automation/) | Observe, decide, act, replay, drift checks | Fake computer provider or configured desktop |
| Dungeon crawler | [`dungeon-crawler/`](dungeon-crawler/) | Pure Nudge state, combat, routing, and tests | None |
| Property fuzz | [`property-fuzz/`](property-fuzz/) | `for_all`, generators, and shrinking | None |
| Provider smoke | [`smoke_provider.ndg`](smoke_provider.ndg) | Real-provider adapter smoke test | Configured provider |

## Check and test examples

From the repository root:

```sh
cargo run -p nudgec -- check examples/hello_llm.ndg
cargo run -p nudgec -- check examples/chatbot/chatbot.ndg
cargo run -p nudgec -- test examples/property-fuzz/property-fuzz.ndg
```

To build an example from its directory:

```sh
cd examples/chatbot
nudgec check chatbot.ndg
nudgec build chatbot.ndg
python3 out/chatbot.py
```

For replay-backed examples, `nudgec test` uses the committed trace fixture and
does not contact a live provider. See each example's README for its specific
run command and configuration.

## Special cases

- `examples/research_agent.ndg` is the canonical research acceptance fixture.
  The former `examples/research-agent/` copy was removed because it duplicated
  the same concept with a separate trace and README.
- `examples/notebooks/` was removed. The notebooks installed an external
  `nudge-runtime` package, depended on machine-local Windows paths, and included
  captured output from one developer's environment. They were not portable or
  reproducible examples.
- `examples/dungeon-crawler/index.html` and `play.py` are optional presentation
  helpers. The source of truth remains `dungeon-crawler.ndg` and its compiler
  checks.

## Replay fixtures

Recorded traces are kept next to the examples that use them. The shared
[`traces/`](traces/) directory contains the acceptance fixture for
`research_agent.ndg` and the small top-level demo trace.

## Contributing

New examples should be self-contained, use portable paths, include a README
when they need configuration, and pass `nudgec check` from a clean checkout.
See [`CONTRIBUTING.md`](../CONTRIBUTING.md) for project contribution guidance.
