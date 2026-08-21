# Nudge Examples

Real-world examples built with Nudge. Each example demonstrates different features and use cases.

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

## Contributing

Want to add your own example? See [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines.
