# Nudge Interactive Curriculum Cheat-Sheet

A concise one-page reference covering all **140 interactive lessons** available in `nudgec learn`.

---

## 1. Quick Syntax Index

### Basic Syntax & Types (Lessons 1–22)
```nudge
let immutable_val = 42
let mut mutable_counter = 0
mutable_counter = mutable_counter + 1

fn add(a: int, b: int) -> int {
    a + b
}

type Point = { x: int, y: int }
type Status = Success | Error(string)
```

### LLM Integration & Prompting (Lessons 23–35)
```nudge
fn ask_llm(question: string) -> string uses LLM {
    llm"""Answer concisely: {question}"""
    with {
        model: "anthropic:sonnet-4.6",
        budget: 0.02 USD,
        retry: 2 with repair
    }
}
```

### Typed Decisions & Confidence Policies (Lessons 36–47)
```nudge
fn classify(ticket: string) -> string uses Decision {
    let d = decide {
        dept: "Target department?" choose [billing, support, dev]
    } on ticket with { model: "laya:multilingual", deadline: 50 }

    route {
        d.dept.winner when d.dept.confidence >= 0.8,
        "human_escalation" otherwise
    }
}
```

### Reactive UI & Desktop Studio (Lessons 48–60, 101–110)
```nudge
agent CounterApp {
    state { count: int = 0 }

    fn increment() uses UI {
        state.count = state.count + 1
    }

    view MainWindow() -> View {
        column(padding = 16, gap = 8) [
            text(str.format("Count: {0}", state.count), size = 16, bold = true),
            button("Add", on_click = increment)
        ]
    }
}
```

### Distributed Agent Mesh & Persistence (Lessons 61–80)
- `a2a.serve`: Expose agent cards via `.well-known/agent.json` and JSON-RPC 2.0.
- `state.x = val`: Automatic durable mutation and transaction checkpointing.
- `mem.set` / `mem.get`: Fast in-memory virtual filesystem caching (`MemStore`).

### High Assurance, Bytecode VM & Security (Lessons 111–140)
- **Effects**: Declared explicitly via `uses <Effect>`. Undeclared effects fail closed (`E0301`).
- **Security Sandboxes**: Fail-closed process execution (`NUDGE_PROCESS_GRANTS`), workspace path containment (`E0910`, `E0922`), and prompt-injection fuzzing (`gen.injection()`).
- **Bytecode Virtual Machine**: Pure functions compile to `.nbc` stack bytecode for 70x faster execution than the AST interpreter.
EOF
