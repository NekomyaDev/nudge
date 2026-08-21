# AI Chatbot

A simple AI chatbot built with Nudge. Maintains conversation memory and responds naturally.

## Features

- Conversation memory (remembers previous messages)
- Natural language responses
- Budget-controlled API calls
- Trace-based testing

## Code

```nudge
type Message = { role: string, content: string }
type Conversation = { messages: [Message], summary: string }

fn chat(history: [Message], user_input: string) -> Conversation uses LLM {
    llm"""Continue this conversation naturally.

    Conversation history:
    {history}

    User: {user_input}

    Respond as a helpful assistant. Return the updated conversation with all messages and a brief summary."""
    with { schema: Conversation, model: "anthropic:sonnet-4.6", budget: 0.02 USD, retry: 2 with repair }
}

fn main() -> Conversation uses LLM {
    let initial: [Message] = [
        { role: "user", content: "Hello! What can you help me with?" }
    ]
    chat(initial, "Tell me about Nudge programming language")
}

test "chat maintains conversation history" {
    let t = replay("traces/chat.jsonl")
    assert len(t.output.messages) >= 2
    assert t.output.summary != ""
    assert t.cost_usd < 0.05
}
```

## Run

```sh
nudgec check chatbot.ndg
nudgec build chatbot.ndg
python3 out/chatbot.py
```

## How it works

1. Takes conversation history and new user input
2. Sends to LLM with schema validation
3. Returns updated conversation with summary
4. Budget enforced per-call ($0.02 USD)
