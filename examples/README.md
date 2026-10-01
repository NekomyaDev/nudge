# Nudge Examples

> [!NOTE]
> **Testing & Preview Stage / 测试与技术预览阶段**: The examples in this directory serve as integration test fixtures, compiler validation suites, and technical previews. They are actively tested across CI matrices and continue to evolve alongside the Nudge language specification.


## Examples

### [AI Chatbot](chatbot/)
A conversational AI agent with memory. Demonstrates:
- Conversation history management
- Schema-validated responses
- Budget-controlled API calls

### [Code Reviewer](code-reviewer/)
An AI agent that reviews code for quality and security. Demonstrates:
- Structured output with line numbers
- Severity-based issue classification
- Quality scoring

### [Research Agent](research-agent/)
An AI agent that researches topics and produces findings. Demonstrates:
- Multi-source research
- Confidence scoring
- Research gap identification

### [Data Analyzer](data-analyzer/)
An AI agent that analyzes data and provides insights. Demonstrates:
- Pattern recognition
- Statistical analysis
- Actionable recommendations

### [Translator](translator/)
An AI agent that translates text between languages. Demonstrates:
- Multi-language support
- Quality scoring
- Parallel translation (par map)

### [Support Ticket Classifier](classifier/)
Classifies a batch of tickets with per-call model routing. Demonstrates:
- `route{}` cost-aware model routing (cheap vs strong per ticket)
- `par map` over a batch
- Typed categories with repair

### [RAG Agent](rag-agent/)
A retrieval-augmented agent that answers only from retrieved context. Demonstrates:
- MCP tool retrieval (`impl: mcp("kb").retrieve`)
- Grounded answers with citations — and explicit refusal when evidence is thin
- Budget-controlled synthesis with repair

### [Dungeon Crawler RPG](dungeon-crawler/)
A turn-based RPG adventure game engine written purely in Nudge. Demonstrates:
- Pure Nudge game logic and turn-based combat mechanics
- Stateful hero progression (HP, ATK, DEF, Potions, Gold, XP, Level)
- Value-level policy switching with `route{}` (armor mitigation, critical hits, branching outcomes)
- Merchant trading, weapon & armor forging, and shrine restoration

## More Examples

Compact single-file programs at the top of this directory, used by the compiler's CI and smoke tests:

- [`hello_llm.ndg`](hello_llm.ndg) — the smallest possible LLM program
- [`checkpoint_agent.ndg`](checkpoint_agent.ndg) — resume a crashed run from its checkpoint (`nudgec resume`)
- [`smoke_provider.ndg`](smoke_provider.ndg) — exercises provider plumbing (also runnable against live providers via the smoke workflows)
- [`research_agent.ndg`](research_agent.ndg) — the compiler's own acceptance fixture: every `cargo test` run compiles and type-checks this file

```sh
nudgec test examples/hello_llm.ndg
```

Their replay fixtures live in [`traces/`](traces/).

## Quick Start

```sh
# Run any example
cd examples/chatbot
nudgec check chatbot.ndg
nudgec build chatbot.ndg
python3 out/chatbot.py
```

## Features Demonstrated

| Example | Typed LLM | Replay | Budget | Parallel | Effects |
|:---|:---:|:---:|:---:|:---:|:---:|
| Chatbot | ✅ | ✅ | ✅ | - | LLM |
| Code Reviewer | ✅ | ✅ | ✅ | - | LLM |
| Research Agent | ✅ | ✅ | ✅ | - | LLM |
| Data Analyzer | ✅ | ✅ | ✅ | - | LLM |
| Translator | ✅ | ✅ | ✅ | ✅ | LLM |
| Classifier | ✅ | ✅ | ✅ | ✅ | LLM |
| RAG Agent | ✅ | ✅ | ✅ | - | LLM, Tool |
| Dungeon Crawler | - | - | - | - | Pure |

## Contributing

Want to add your own example? See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.
