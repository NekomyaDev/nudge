// `nudgec learn` - interactive language tutorial: sixty comprehensive lessons.
// Every lesson is an executable, type-checked `.ndg` source file located in
// `docs/learn/<name>.ndg` containing its documentation in doc comments (`///`).

pub struct Lesson {
    pub name: &'static str,
    pub title: &'static str,
    /// Raw `.ndg` source containing doc comments and executable code.
    pub source: &'static str,
}

impl Lesson {
    /// Extracts the documentation body from doc comments (`///`).
    pub fn body(&self) -> String {
        let mut out = Vec::new();
        for line in self.source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("///") {
                let text = trimmed.strip_prefix("///").unwrap();
                out.push(text.strip_prefix(' ').unwrap_or(text));
            } else if !trimmed.is_empty() {
                break;
            }
        }
        out.join("\n")
    }

    /// Extracts the executable Nudge program portion (non-doc comment lines).
    pub fn program(&self) -> Option<String> {
        let mut out = Vec::new();
        let mut found_code = false;
        for line in self.source.lines() {
            let trimmed = line.trim();
            if !found_code {
                if trimmed.starts_with("///") || trimmed.is_empty() {
                    continue;
                }
                found_code = true;
            }
            out.push(line);
        }
        if found_code && !out.is_empty() {
            Some(out.join("\n"))
        } else {
            None
        }
    }
}

pub const LESSONS: &[Lesson] = &[
    Lesson {
        name: "hello",
        title: "1/60 - Your first LLM call",
        source: include_str!("../../../docs/learn/hello.ndg"),
    },
    Lesson {
        name: "types",
        title: "2/60 - Typed outputs (schemas)",
        source: include_str!("../../../docs/learn/types.ndg"),
    },
    Lesson {
        name: "decide",
        title: "3/60 - Typed decisions (decide{})",
        source: include_str!("../../../docs/learn/decide.ndg"),
    },
    Lesson {
        name: "policy",
        title: "4/60 - Confidence policies (route{})",
        source: include_str!("../../../docs/learn/policy.ndg"),
    },
    Lesson {
        name: "test",
        title: "5/60 - $0 tests and property fuzzing",
        source: include_str!("../../../docs/learn/test.ndg"),
    },
    Lesson {
        name: "trace",
        title: "6/60 - Traces, replay and regressions",
        source: include_str!("../../../docs/learn/trace.ndg"),
    },
    Lesson {
        name: "ui",
        title: "7/60 - Interactive UI & Agents",
        source: include_str!("../../../docs/learn/ui.ndg"),
    },
    Lesson {
        name: "a2a",
        title: "8/60 - Agent-to-Agent (A2A) Peer Serving",
        source: include_str!("../../../docs/learn/a2a.ndg"),
    },
    Lesson {
        name: "variables_mut",
        title: "9/60 - Variables and mutable state",
        source: include_str!("../../../docs/learn/variables_mut.ndg"),
    },
    Lesson {
        name: "primitives",
        title: "10/60 - Primitive types",
        source: include_str!("../../../docs/learn/primitives.ndg"),
    },
    Lesson {
        name: "functions",
        title: "11/60 - Functions, parameters, and return types",
        source: include_str!("../../../docs/learn/functions.ndg"),
    },
    Lesson {
        name: "operators",
        title: "12/60 - Arithmetic, logical, and comparison operators",
        source: include_str!("../../../docs/learn/operators.ndg"),
    },
    Lesson {
        name: "string_ops",
        title: "13/60 - String operations",
        source: include_str!("../../../docs/learn/string_ops.ndg"),
    },
    Lesson {
        name: "records",
        title: "14/60 - Records and type aliases",
        source: include_str!("../../../docs/learn/records.ndg"),
    },
    Lesson {
        name: "lists",
        title: "15/60 - Lists, indexing, and collection loops",
        source: include_str!("../../../docs/learn/lists.ndg"),
    },
    Lesson {
        name: "options_results",
        title: "16/60 - Optional values and error-shaped data",
        source: include_str!("../../../docs/learn/options_results.ndg"),
    },
    Lesson {
        name: "pattern_matching",
        title: "17/60 - Branching before match expressions",
        source: include_str!("../../../docs/learn/pattern_matching.ndg"),
    },
    Lesson {
        name: "loops",
        title: "18/60 - Collection loops",
        source: include_str!("../../../docs/learn/loops.ndg"),
    },
    Lesson {
        name: "purity_effects",
        title: "19/60 - Purity and tracked effects",
        source: include_str!("../../../docs/learn/purity_effects.ndg"),
    },
    Lesson {
        name: "modules_imports",
        title: "20/60 - Organizing modules",
        source: include_str!("../../../docs/learn/modules_imports.ndg"),
    },
    Lesson {
        name: "assertions_contracts",
        title: "21/60 - Assertions and lightweight contracts",
        source: include_str!("../../../docs/learn/assertions_contracts.ndg"),
    },
    Lesson {
        name: "comments_docs",
        title: "22/60 - Comments and documentation",
        source: include_str!("../../../docs/learn/comments_docs.ndg"),
    },
    Lesson {
        name: "prompt_syntax",
        title: "23/60 - Prompt syntax and multi-line blocks",
        source: include_str!("../../../docs/learn/prompt_syntax.ndg"),
    },
    Lesson {
        name: "prompt_interpolation",
        title: "24/60 - Prompt interpolation and formatting",
        source: include_str!("../../../docs/learn/prompt_interpolation.ndg"),
    },
    Lesson {
        name: "structured_schemas",
        title: "25/60 - Nested structured-output schemas",
        source: include_str!("../../../docs/learn/structured_schemas.ndg"),
    },
    Lesson {
        name: "model_budget",
        title: "26/60 - Budgets, token caps, and hard limits",
        source: include_str!("../../../docs/learn/model_budget.ndg"),
    },
    Lesson {
        name: "model_routing",
        title: "27/60 - Selecting and routing models",
        source: include_str!("../../../docs/learn/model_routing.ndg"),
    },
    Lesson {
        name: "retry_repair",
        title: "28/60 - Retry and automatic JSON repair",
        source: include_str!("../../../docs/learn/retry_repair.ndg"),
    },
    Lesson {
        name: "fallback_models",
        title: "29/60 - Fallback models for failures and timeouts",
        source: include_str!("../../../docs/learn/fallback_models.ndg"),
    },
    Lesson {
        name: "few_shot",
        title: "30/60 - Few-shot prompting and context conditioning",
        source: include_str!("../../../docs/learn/few_shot.ndg"),
    },
    Lesson {
        name: "prompt_clippy",
        title: "31/60 - Prompt Clippy reliability warnings",
        source: include_str!("../../../docs/learn/prompt_clippy.ndg"),
    },
    Lesson {
        name: "temperature_params",
        title: "32/60 - Temperature, top_p, and seed parameters",
        source: include_str!("../../../docs/learn/temperature_params.ndg"),
    },
    Lesson {
        name: "streaming",
        title: "33/60 - Streaming responses and token chunks",
        source: include_str!("../../../docs/learn/streaming.ndg"),
    },
    Lesson {
        name: "system_prompts",
        title: "34/60 - System prompts and role management",
        source: include_str!("../../../docs/learn/system_prompts.ndg"),
    },
    Lesson {
        name: "caching_cost",
        title: "35/60 - Prompt caching and cost optimization",
        source: include_str!("../../../docs/learn/caching_cost.ndg"),
    },
    Lesson {
        name: "decide_syntax",
        title: "36/60 - decide { ... } on state with { ... }",
        source: include_str!("../../../docs/learn/decide_syntax.ndg"),
    },
    Lesson {
        name: "choose_questions",
        title: "37/60 - choose [a, b, c]: classifying with a winner",
        source: include_str!("../../../docs/learn/choose_questions.ndg"),
    },
    Lesson {
        name: "yesno_questions",
        title: "38/60 - yes/no questions and probability p",
        source: include_str!("../../../docs/learn/yesno_questions.ndg"),
    },
    Lesson {
        name: "score_questions",
        title: "39/60 - score [low, medium, high] and weighted scores",
        source: include_str!("../../../docs/learn/score_questions.ndg"),
    },
    Lesson {
        name: "multi_question_batch",
        title: "40/60 - batch three questions in one decision call",
        source: include_str!("../../../docs/learn/multi_question_batch.ndg"),
    },
    Lesson {
        name: "confidence_thresholds",
        title: "41/60 - filter with p and confidence thresholds",
        source: include_str!("../../../docs/learn/confidence_thresholds.ndg"),
    },
    Lesson {
        name: "route_syntax",
        title: "42/60 - route { arm: expr when cond, otherwise: fallback }",
        source: include_str!("../../../docs/learn/route_syntax.ndg"),
    },
    Lesson {
        name: "human_in_the_loop",
        title: "43/60 - escalate low-confidence decisions to a human",
        source: include_str!("../../../docs/learn/human_in_the_loop.ndg"),
    },
    Lesson {
        name: "policy_sweep",
        title: "44/60 - calibrate thresholds offline with policy-sweep",
        source: include_str!("../../../docs/learn/policy_sweep.ndg"),
    },
    Lesson {
        name: "injection_guard",
        title: "45/60 - gen.injection() and prompt-injection defenses",
        source: include_str!("../../../docs/learn/injection_guard.ndg"),
    },
    Lesson {
        name: "decision_caching",
        title: "46/60 - cache repeated decisions with NUDGE_DECISION_CACHE",
        source: include_str!("../../../docs/learn/decision_caching.ndg"),
    },
    Lesson {
        name: "explain_reports",
        title: "47/60 - audit decisions with nudgec explain",
        source: include_str!("../../../docs/learn/explain_reports.ndg"),
    },
    Lesson {
        name: "agent_state",
        title: "48/60 - Agent state and durable mutations",
        source: include_str!("../../../docs/learn/agent_state.ndg"),
    },
    Lesson {
        name: "agent_actions",
        title: "49/60 - Agent actions and effect labels",
        source: include_str!("../../../docs/learn/agent_actions.ndg"),
    },
    Lesson {
        name: "ui_views",
        title: "50/60 - Typed views with columns, rows, and controls",
        source: include_str!("../../../docs/learn/ui_views.ndg"),
    },
    Lesson {
        name: "ui_headless_test",
        title: "51/60 - Headless UI tests",
        source: include_str!("../../../docs/learn/ui_headless_test.ndg"),
    },
    Lesson {
        name: "mcp_tools",
        title: "52/60 - Publish functions through MCP",
        source: include_str!("../../../docs/learn/mcp_tools.ndg"),
    },
    Lesson {
        name: "a2a_agent_card",
        title: "53/60 - A2A Agent Cards, capabilities, and skills",
        source: include_str!("../../../docs/learn/a2a_agent_card.ndg"),
    },
    Lesson {
        name: "a2a_peer_serving",
        title: "54/60 - Serve an A2A peer over REST and JSON-RPC",
        source: include_str!("../../../docs/learn/a2a_peer_serving.ndg"),
    },
    Lesson {
        name: "deterministic_fakes",
        title: "55/60 - Deterministic fakes: offline and zero cost",
        source: include_str!("../../../docs/learn/deterministic_fakes.ndg"),
    },
    Lesson {
        name: "property_fuzzing",
        title: "56/60 - Property fuzzing and shrinking",
        source: include_str!("../../../docs/learn/property_fuzzing.ndg"),
    },
    Lesson {
        name: "ntf_traces",
        title: "57/60 - Open NTF traces",
        source: include_str!("../../../docs/learn/ntf_traces.ndg"),
    },
    Lesson {
        name: "trace_replay",
        title: "58/60 - Byte-identical trace replay",
        source: include_str!("../../../docs/learn/trace_replay.ndg"),
    },
    Lesson {
        name: "trace_diff_ci",
        title: "59/60 - Trace diffs as CI regression gates",
        source: include_str!("../../../docs/learn/trace_diff_ci.ndg"),
    },
    Lesson {
        name: "compilation_targets",
        title: "60/60 - Python, TypeScript, and standalone Rust targets",
        source: include_str!("../../../docs/learn/compilation_targets.ndg"),
    },
];

pub fn all_lessons() -> &'static [Lesson] {
    LESSONS
}

pub fn run(rest: &[String]) -> Result<(), String> {
    let lessons = LESSONS;
    if rest.is_empty() {
        println!(
            "nudge learn - complete interactive language curriculum ({} lessons)\n",
            lessons.len()
        );
        for l in lessons {
            println!("  nudgec learn {:<22} {}", l.name, l.title);
        }
        println!("\nstart with: nudgec learn hello");
        return Ok(());
    }
    let name = rest[0].as_str();
    let lesson = lessons
        .iter()
        .find(|l| l.name == name)
        .ok_or_else(|| format!("unknown lesson '{name}' — run `nudgec learn` for the list"))?;
    println!("=== {} ===\n", lesson.title);
    let body = lesson.body();
    if !body.is_empty() {
        println!("{}", body.trim_end());
    }
    if let Some(program) = lesson.program() {
        println!("\n--- {}.ndg ---\n{}", name, program.trim_end());
        println!("\nsave it as {name}.ndg and run the Try commands above.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lesson_programs_type_check_against_the_shipping_compiler() {
        for l in LESSONS {
            // Check the entire .ndg source directly!
            let tokens = crate::lexer::lex(l.source)
                .unwrap_or_else(|e| panic!("lesson '{}' failed to lex: {e:?}", l.name));
            let items = crate::parser::parse(tokens)
                .unwrap_or_else(|e| panic!("lesson '{}' failed to parse: {e:?}", l.name));
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
    fn total_curriculum_contains_sixty_unique_lessons_and_helpful_titles() {
        assert_eq!(LESSONS.len(), 60);
        let mut names: Vec<_> = LESSONS.iter().map(|l| l.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 60, "every lesson name must be unique");
        for (i, l) in LESSONS.iter().enumerate() {
            let expected_prefix = format!("{}/60 - ", i + 1);
            assert!(
                l.title.starts_with(&expected_prefix),
                "lesson '{}' title '{}' should start with '{expected_prefix}'",
                l.name,
                l.title
            );
            let body = l.body();
            assert!(body.contains("Try:") || body.contains("know the whole loop"));
        }
    }

    #[test]
    fn run_lists_and_shows_lessons() {
        assert!(run(&[]).is_ok());
        assert!(run(&["hello".to_string()]).is_ok());
        assert!(run(&["primitives".to_string()]).is_ok());
        assert!(run(&["structured_schemas".to_string()]).is_ok());
        assert!(run(&["decide_syntax".to_string()]).is_ok());
        assert!(run(&["a2a_agent_card".to_string()]).is_ok());
        assert!(run(&["compilation_targets".to_string()]).is_ok());
        assert!(run(&["nope".to_string()]).is_err());
    }
}
