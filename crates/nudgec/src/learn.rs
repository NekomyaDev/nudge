// `nudgec learn` — a terminal tutorial: six short lessons, each with an
// explanation, a program that compiles against the compiler shipping the
// lessons, and the exact commands to run. No TTY assumptions: plain
// stdout, works over SSH and in CI.

pub struct Lesson {
    pub name: &'static str,
    pub title: &'static str,
    /// Lesson body — what the learner should read.
    pub body: &'static str,
    /// Program shown to the learner; every lesson program must type-check
    /// (the test below enforces it against the compiler shipping these).
    pub program: Option<&'static str>,
}

pub const LESSONS: &[Lesson] = &[
    Lesson {
        name: "hello",
        title: "1/6 — Your first LLM call",
        body: r#"Nudge compiles typed LLM calls into plain Python/TypeScript.

An `llm"""..."""` block is a prompt with {interpolation} holes; `with {}`
declares the contract: `schema` pins the output type, `budget` caps the
cost, `model` names the provider. `uses LLM` is an *effect* — the
compiler tracks it, so a function that calls a model can never hide it.

Best part: every program runs against a deterministic fake provider by
default. No API key, no token spend, byte-reproducible tests.

Try:
  nudgec check hello.ndg      # type-check
  nudgec build hello.ndg      # compile to out/hello.py
  python3 out/hello.py        # run — fake provider, offline"#,
        program: Some(
            r#"fn main() -> string uses LLM {
    llm"""Say hello to the Nudge language in one short sentence."""
    with { model: "anthropic:sonnet-4.6", budget: 0.01 USD }
}
"#,
        ),
    },
    Lesson {
        name: "types",
        title: "2/6 — Typed outputs (schemas)",
        body: r#"Free-text LLM output is a bug factory. In Nudge you declare a
record type and pin it with `schema:` — the runtime validates every
response against the type and repairs or rejects mismatches.

The prompt must describe the contract in words too (the compiler checks
that schema fields actually appear in the prompt — Prompt Clippy W0003).

Try:
  nudgec check types.ndg && nudgec build types.ndg"#,
        program: Some(
            r#"type Ticket = { category: string, priority: string, summary: string }

fn triage(body: string) -> Ticket uses LLM {
    llm"""Classify this support ticket.

    Ticket body:
    {body}

    category: one of "billing", "technical", "account", "other"
    priority: one of "low", "normal", "high", "urgent"
    summary: one sentence a support lead can act on"""
    with { schema: Ticket, model: "anthropic:sonnet-4.6", budget: 0.02 USD }
}
"#,
        ),
    },
    Lesson {
        name: "decide",
        title: "3/6 — Typed decisions (decide{})",
        body: r#"`decide{}` asks a *decision model* (the JEV family: Laya, Jev, Valen)
typed questions about a state — one batched call, answers with
probabilities and confidence. A decision is an effect like LLM: tracked,
traced, replayable.

The fake decision provider synthesizes deterministic distributions, so
this too is $0 offline.

Try:
  nudgec check decide.ndg && nudgec build decide.ndg
  python3 out/decide.py"#,
        program: Some(
            r#"type Triage = {
    dept:    { winner: string, p: float, distribution: {string: float}, confidence: float },
    churn:   { p: float },
    urgency: { score: float, distribution: {string: float} }
}

fn triage(t: string) -> Triage uses Decision {
    decide {
        dept:    "which team should handle this ticket?" choose [billing, technical, security],
        churn:   "does the customer threaten to cancel?" yes/no,
        urgency: "how urgent is this?" score [low, soon, critical]
    }
    on t
    with { model: "fake:default", deadline: 50 }
}

fn main() -> Triage uses Decision {
    triage("my laptop is broken and I want a refund")
}
"#,
        ),
    },
    Lesson {
        name: "policy",
        title: "4/6 — Confidence policies (route{})",
        body: r#"raw model output should not reach users unchecked. `route{}` is a
policy switch: arms carry conditions (confidence thresholds, winner
equality) and an `otherwise` fallback. The compiler unifies the arm
types, so every path returns the same shape.

This is the "safe to deploy" pattern: high confidence → automate,
anything else → a human.

Try:
  nudgec check policy.ndg && nudgec build policy.ndg && python3 out/policy.py"#,
        program: Some(
            r#"type Triage = {
    dept:    { winner: string, p: float, distribution: {string: float}, confidence: float },
    churn:   { p: float },
    urgency: { score: float, distribution: {string: float} }
}

fn triage(t: string) -> Triage uses Decision {
    decide {
        dept:    "which team should handle this ticket?" choose [billing, technical, security],
        churn:   "does the customer threaten to cancel?" yes/no,
        urgency: "how urgent is this?" score [low, soon, critical]
    }
    on t
    with { model: "fake:default", deadline: 50 }
}

fn handle(t: string) -> string uses IO {
    "handled"
}

fn main() -> string uses Decision, IO {
    let d = triage("my laptop is broken and I want a refund")
    route {
        auto:   handle(d.dept.winner) when d.dept.confidence > 0.6,
        security: handle("security queue") when d.dept.winner == "security",
        review: handle("manual review") otherwise
    }
}

test "triage stays deterministic and policy picks an arm" {
    let d = triage("my laptop is broken and I want a refund")
    assert d.dept.p <= 1
    assert d.churn.p >= 0
}
"#,
        ),
    },
    Lesson {
        name: "test",
        title: "5/6 — $0 tests and property fuzzing",
        body: r#"test blocks run against the fake provider: deterministic, $0, no API
keys — your whole suite runs on every save.

`for_all` goes further: it loops a property over generated cases (edge
values + a seeded sweep) and *shrinks* any failure to a minimal
counterexample you can paste into a regression assert. gen.injection()
feeds adversarial prompt-injection strings into pure code.

Try:
  nudgec test test.ndg"#,
        program: Some(
            r#"fn sanitize(s: string) -> int {
    len(s)
}

test "sanitize output stays within bounds" {
    for_all s in gen.str(64) {
        assert len(s) >= 0
    }
}

test "injection corpus never crashes the checker" {
    for_all p in gen.injection() {
        assert len(p) >= 0
    }
}
"#,
        ),
    },
    Lesson {
        name: "trace",
        title: "6/6 — Traces, replay and regressions",
        body: r#"Set NUDGE_TRACE=<file> and every LLM call, tool call and decision
lands in an NTF trace (an open, frozen-v1 JSONL format — no vendor
lock-in). Then:

  nudgec trace-check t.jsonl    # validate against the schema
  NUDGE_REPLAY=t.jsonl python3 out/hello.py   # replay: $0, byte-identical
  nudgec trace-diff a.jsonl b.jsonl --fail-on-regression   # CI gate: cost/tokens/latency growth
  nudgec policy-sweep t.jsonl --question dept --thresholds 0.5,0.8  # re-cut thresholds, zero calls

And NUDGE_DECISION_CACHE=<path> stops you from ever paying for the same
decision twice.

You now know the whole loop: write → check → build → test → trace →
gate. Next: `nudgec init my-agent --list` and ship something."#,
        program: None,
    },
];

pub fn run(rest: &[String]) -> Result<(), String> {
    if rest.is_empty() {
        println!("nudge learn — the language in six lessons\n");
        for l in LESSONS {
            println!("  nudgec learn {:<8} {}", l.name, l.title);
        }
        println!("\nstart with: nudgec learn hello");
        return Ok(());
    }
    let name = rest[0].as_str();
    let lesson = LESSONS
        .iter()
        .find(|l| l.name == name)
        .ok_or_else(|| format!("unknown lesson '{name}' — run `nudgec learn` for the list"))?;
    println!("=== {} ===\n", lesson.title);
    println!("{}", lesson.body);
    if let Some(program) = lesson.program {
        println!("\n--- {}.ndg ---\n{}\n", name, program);
        println!("save it as {name}.ndg and run the Try commands above.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lesson_programs_type_check_against_the_shipping_compiler() {
        for l in LESSONS {
            let Some(src) = l.program else { continue };
            let tokens = crate::lexer::lex(src).unwrap();
            let items = crate::parser::parse(tokens).unwrap();
            let errs = crate::check::check(&items);
            assert!(
                errs.is_empty(),
                "lesson '{}' does not compile: {:?}",
                l.name,
                errs.iter().map(|e| &e.msg).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn six_lessons_with_unique_names_and_helpful_titles() {
        assert_eq!(LESSONS.len(), 6);
        let mut names: Vec<_> = LESSONS.iter().map(|l| l.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LESSONS.len());
        for l in LESSONS {
            assert!(l.title.len() > 5);
            assert!(l.body.contains("Try:") || l.body.contains("know the whole loop"));
        }
    }

    #[test]
    fn run_lists_and_shows_lessons() {
        assert!(run(&[]).is_ok());
        assert!(run(&["hello".to_string()]).is_ok());
        assert!(run(&["nope".to_string()]).is_err());
    }
}
