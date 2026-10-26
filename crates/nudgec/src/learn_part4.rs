//! `nudgec learn` curriculum, Part 4: Agents, UI, A2A, replay, and testing.
//!
//! Every displayed program is intentionally kept compatible with the shipping
//! lexer, parser, and type checker. `learn.rs` owns the original quickstart;
//! this module holds the detailed lessons 48 through 60.

use crate::learn::Lesson;

pub const LESSONS_PART4: &[Lesson] = &[
    Lesson {
        name: "agent_state",
        title: "48/60 - Agent state and durable mutations",
        body: r#"An `agent` groups durable state with the operations allowed to mutate it.
The `state { ... }` declaration gives every field both a type and a deterministic
initial value. Inside agent methods, `state.field = value` and `+=` are tracked
state writes, so the runtime can checkpoint them for recovery and replay.

Try:
  nudgec check agent_state.ndg
  nudgec test agent_state.ndg"#,
        program: Some(r#"agent Counter {
    state {
        count: int = 0
        label: string = "ready"
    }

    fn increment() {
        state.count += 1
        state.label = "counted"
    }
}

test "agent state mutates through its methods" {
    Counter.increment()
    assert Counter.state.count == 1
    assert Counter.state.label == "counted"
}
"#),
    },
    Lesson {
        name: "agent_actions",
        title: "49/60 - Agent actions and effect labels",
        body: r#"An agent method is written exactly like a function, but it can read and
mutate that agent's state. Its `uses` list remains part of the public contract:
`UI` covers interface actions, `LLM` covers model calls, and other effects are
similarly visible to the compiler and callers.

Try:
  nudgec check agent_actions.ndg
  nudgec build agent_actions.ndg"#,
        program: Some(r#"agent DraftAgent {
    state {
        draft: string = "empty"
    }

    fn edit(next: string) uses UI {
        state.draft = next
    }

    fn summarize() -> string uses LLM {
        llm"""Summarize this draft in one short sentence: {state.draft}"""
        with { model: "fake:default", budget: 0.01 USD }
    }
}

fn main() -> string uses UI, LLM {
    DraftAgent.edit("A typed agent keeps actions explicit.")
    DraftAgent.summarize()
}
"#),
    },
    Lesson {
        name: "ui_views",
        title: "50/60 - Typed views with columns, rows, and controls",
        body: r#"A `view` returns `View` and is composed from typed primitives. `column` and
`row` organize a tree; `text`, `button`, and `input` carry visible content,
stable IDs, and event-handler names. Keeping UI in Nudge means its state and
handlers share the same types as the rest of the agent.

Try:
  nudgec check ui_views.ndg
  nudgec build ui_views.ndg"#,
        program: Some(r#"view Greeting(name: string) -> View {
    column(padding = 16, gap = 8) [
        text("Welcome, " + name, size = 18, bold = true),
        row(gap = 6) [
            input(name, id = "name_input", on_change = update_name),
            button("Save", id = "save_button", on_click = save_name)
        ]
    ]
}

agent Welcome {
    state {
        name: string = "Nudge"
    }

    fn update_name(value: string) uses UI {
        state.name = value
    }

    fn save_name() uses UI {
        state.name = state.name
    }

    window MainWindow {
        title: "Welcome",
        width: 360,
        height: 180,
        content: Greeting(state.name)
    }
}

fn main() uses UI {
    Welcome.MainWindow.open()
}
"#),
    },
    Lesson {
        name: "ui_headless_test",
        title: "51/60 - Headless UI tests",
        body: r#"UI tests do not need a desktop or browser. `mount_headless()` renders a
window in memory, `find(...).exists()` checks stable element IDs, and
`dispatch_click(...)` invokes a handler. This gives ordinary CI a deterministic
way to test UI state transitions.

Try:
  nudgec check ui_headless_test.ndg
  nudgec test ui_headless_test.ndg"#,
        program: Some(r#"view CounterView(count: int) -> View {
    column(gap = 8) [
        text("Count: " + count),
        button("Increment", id = "increment", on_click = increment)
    ]
}

agent CounterUI {
    state {
        count: int = 0
    }

    fn increment() uses UI {
        state.count += 1
    }

    window MainWindow {
        title: "Counter",
        content: CounterView(state.count)
    }
}

test "headless mount finds and clicks controls" {
    let app = CounterUI.MainWindow.mount_headless()
    assert app.find("increment").exists()
    app.dispatch_click("increment")
    assert CounterUI.state.count == 1
}
"#),
    },
    Lesson {
        name: "mcp_tools",
        title: "52/60 - Publish functions through MCP",
        body: r#"`nudgec mcp program.ndg` exposes selected Nudge functions as Model Context
Protocol tools over stdio. Define a `tool` declaration for external MCP calls,
or expose ordinary typed functions with `--fns name1,name2`. Types become the
tool input schema and function results become structured tool output.

Try:
  nudgec check mcp_tools.ndg
  nudgec mcp mcp_tools.ndg --fns add"#,
        program: Some(r#"tool lookup_city(name: string) -> string {
    impl: mcp("directory").lookup(name)
    side_effects: none
}

fn add(left: int, right: int) -> int {
    left + right
}

fn main() -> int {
    add(20, 22)
}
"#),
    },
    Lesson {
        name: "a2a_agent_card",
        title: "53/60 - A2A Agent Cards, capabilities, and skills",
        body: r#"`nudgec a2a agent.ndg` derives an `agent.json` card from your agent
structure. The card advertises identity, capabilities, stateful skills, and
the typed methods peers may invoke. Keep exposed methods narrow and typed so
other agents can discover precisely what is safe to call.

Try:
  nudgec check a2a_agent_card.ndg
  nudgec a2a a2a_agent_card.ndg"#,
        program: Some(r#"agent WeatherPeer {
    state {
        requests: int = 0
    }

    fn forecast(city: string) -> string {
        state.requests += 1
        "Forecast requested for " + city
    }

    fn capability() -> string {
        "Provides deterministic city forecast summaries"
    }
}

fn main() {
}
"#),
    },
    Lesson {
        name: "a2a_peer_serving",
        title: "54/60 - Serve an A2A peer over REST and JSON-RPC",
        body: r#"Run `nudgec a2a-serve peer.ndg --port 9000` to turn an agent into an
A2A peer. It serves the Agent Card at `/.well-known/agent.json`, health at
`/health`, typed skills at `/skills/<name>`, and A2A JSON-RPC 2.0 at `/rpc`.
The same program remains unit-testable without opening a port.

Try:
  nudgec check a2a_peer_serving.ndg
  nudgec a2a-serve a2a_peer_serving.ndg --port 9000"#,
        program: Some(r#"agent EchoPeer {
    state {
        calls: int = 0
    }

    fn echo(message: string) -> string {
        state.calls += 1
        "echo: " + message
    }
}

fn main() {
}

test "peer skill is callable locally" {
    assert EchoPeer.echo("hello") == "echo: hello"
    assert EchoPeer.state.calls == 1
}
"#),
    },
    Lesson {
        name: "deterministic_fakes",
        title: "55/60 - Deterministic fakes: offline and zero cost",
        body: r#"Use `model: "fake:default"` when developing and testing. The deterministic
provider gives repeatable responses without API keys, network access, or spend.
It is the default testing foundation for fast local and CI feedback.

Try:
  nudgec check deterministic_fakes.ndg
  nudgec test deterministic_fakes.ndg"#,
        program: Some(r#"fn classify(ticket: string) -> string uses LLM {
    llm"""Classify this support ticket in one word: {ticket}"""
    with { model: "fake:default", budget: 0.00 USD }
}

test "fake provider runs offline" {
    let answer = classify("Please reset my account password")
    assert len(answer) >= 0
}
"#),
    },
    Lesson {
        name: "property_fuzzing",
        title: "56/60 - Property fuzzing and shrinking",
        body: r#"`for_all` executes a property over generated input. `gen.str(64)` supplies
strings up to 64 characters, including useful edge cases. If an assertion
fails, Nudge shrinks the original input toward a smallest counterexample, which
makes a failing case practical to add as a regression test.

Try:
  nudgec check property_fuzzing.ndg
  nudgec test property_fuzzing.ndg"#,
        program: Some(r#"fn normalized_length(value: string) -> int {
    len(value)
}

test "all generated strings have a non-negative length" {
    for_all x in gen.str(64) {
        assert normalized_length(x) >= 0
        assert normalized_length(x) <= 64
    }
}
"#),
    },
    Lesson {
        name: "ntf_traces",
        title: "57/60 - Open NTF traces",
        body: r#"Set `NUDGE_TRACE=<path>` before running generated code to record calls and
outcomes as NTF JSONL. NTF is an open, versioned trace schema: inspect it with
ordinary tooling, validate it with `nudgec trace-check`, and retain it as a
portable test artifact.

Try:
  nudgec check ntf_traces.ndg
  nudgec build ntf_traces.ndg
  NUDGE_TRACE=run.ntf.jsonl python3 out/ntf_traces.py
  nudgec trace-check run.ntf.jsonl"#,
        program: Some(r#"fn greet(person: string) -> string uses LLM {
    llm"""Greet {person} in one short sentence."""
    with { model: "fake:default", budget: 0.00 USD }
}

fn main() -> string uses LLM {
    greet("trace reader")
}
"#),
    },
    Lesson {
        name: "trace_replay",
        title: "58/60 - Byte-identical trace replay",
        body: r#"A captured NTF trace can replace live provider calls. Set
`NUDGE_REPLAY=<trace-path>` when executing the same compiled program and the
runtime replays recorded events at $0. That makes a previously observed run
repeatable, including its exact model-facing inputs and outputs.

Try:
  nudgec check trace_replay.ndg
  nudgec build trace_replay.ndg
  NUDGE_TRACE=first.ntf.jsonl python3 out/trace_replay.py
  NUDGE_REPLAY=first.ntf.jsonl python3 out/trace_replay.py"#,
        program: Some(r#"fn answer(question: string) -> string uses LLM {
    llm"""Answer this question briefly: {question}"""
    with { model: "fake:default", budget: 0.00 USD }
}

fn main() -> string uses LLM {
    answer("What does replay protect?")
}
"#),
    },
    Lesson {
        name: "trace_diff_ci",
        title: "59/60 - Trace diffs as CI regression gates",
        body: r#"Compare a trusted baseline trace to a candidate trace with
`nudgec trace-diff baseline.ntf.jsonl candidate.ntf.jsonl --fail-on-regression`.
The command turns growth in cost, token use, latency, or failures into a
non-zero exit code, so CI can reject an agent regression before deployment.

Try:
  nudgec check trace_diff_ci.ndg
  nudgec trace-diff baseline.ntf.jsonl candidate.ntf.jsonl --fail-on-regression"#,
        program: Some(r#"fn concise_summary(note: string) -> string uses LLM {
    llm"""Summarize this note in one concise sentence: {note}"""
    with { model: "fake:default", budget: 0.00 USD }
}

fn main() -> string uses LLM {
    concise_summary("Trace comparison makes resource regressions visible.")
}
"#),
    },
    Lesson {
        name: "compilation_targets",
        title: "60/60 - Python, TypeScript, and standalone Rust targets",
        body: r#"The same checked Nudge source can target several deployment shapes:
`nudgec build program.ndg` emits Python, `nudgec build-ts program.ndg` emits
TypeScript, and `nudgec build program.ndg --standalone` packages a native Rust
agent binary. Type-check first, then select the runtime ecosystem that matches
your deployment.

Try:
  nudgec check compilation_targets.ndg
  nudgec build compilation_targets.ndg
  nudgec build-ts compilation_targets.ndg
  nudgec build compilation_targets.ndg --standalone"#,
        program: Some(r#"fn double(value: int) -> int {
    value * 2
}

fn main() -> int {
    double(21)
}
"#),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part4_programs_type_check_against_the_shipping_compiler() {
        for lesson in LESSONS_PART4 {
            let Some(src) = lesson.program else { continue };
            let tokens = crate::lexer::lex(src).unwrap();
            let items = crate::parser::parse(tokens).unwrap();
            let errors = crate::check::check(&items);
            assert!(
                errors.is_empty(),
                "lesson '{}' does not compile: {:?}",
                lesson.name,
                errors.iter().map(|error| &error.msg).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn part4_has_all_thirteen_unique_lessons() {
        assert_eq!(LESSONS_PART4.len(), 13);
        let mut names: Vec<_> = LESSONS_PART4.iter().map(|lesson| lesson.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LESSONS_PART4.len());
        assert!(LESSONS_PART4.iter().all(|lesson| lesson.body.contains("Try:")));
    }
}
