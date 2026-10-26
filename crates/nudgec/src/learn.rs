// `nudgec learn` - a terminal tutorial: sixty comprehensive lessons.
// Lessons and programs live cleanly in `docs/learn/*.md` and `docs/learn/*.ndg`
// without code duplication, type-checked by test against the shipping compiler.

pub struct Lesson {
    pub name: &'static str,
    pub title: &'static str,
    /// Lesson markdown body loaded from docs/learn.
    pub doc: &'static str,
    /// Lesson program loaded from docs/learn (if any).
    pub program: Option<&'static str>,
}

impl Lesson {
    /// Body text to show the learner (stripping the markdown H1 title line).
    pub fn body(&self) -> &'static str {
        if let Some(pos) = self.doc.find("\n\n") {
            &self.doc[pos + 2..]
        } else {
            self.doc
        }
    }
}

pub const LESSONS: &[Lesson] = &[
    Lesson {
        name: "hello",
        title: "1/60 - Your first LLM call",
        doc: include_str!("../../../docs/learn/01_hello.md"),
        program: Some(include_str!("../../../docs/learn/01_hello.ndg")),
    },
    Lesson {
        name: "types",
        title: "2/60 - Typed outputs (schemas)",
        doc: include_str!("../../../docs/learn/02_types.md"),
        program: Some(include_str!("../../../docs/learn/02_types.ndg")),
    },
    Lesson {
        name: "decide",
        title: "3/60 - Typed decisions (decide{})",
        doc: include_str!("../../../docs/learn/03_decide.md"),
        program: Some(include_str!("../../../docs/learn/03_decide.ndg")),
    },
    Lesson {
        name: "policy",
        title: "4/60 - Confidence policies (route{})",
        doc: include_str!("../../../docs/learn/04_policy.md"),
        program: Some(include_str!("../../../docs/learn/04_policy.ndg")),
    },
    Lesson {
        name: "test",
        title: "5/60 - $0 tests and property fuzzing",
        doc: include_str!("../../../docs/learn/05_test.md"),
        program: Some(include_str!("../../../docs/learn/05_test.ndg")),
    },
    Lesson {
        name: "trace",
        title: "6/60 - Traces, replay and regressions",
        doc: include_str!("../../../docs/learn/06_trace.md"),
        program: None,
    },
    Lesson {
        name: "ui",
        title: "7/60 - Interactive UI & Agents",
        doc: include_str!("../../../docs/learn/07_ui.md"),
        program: Some(include_str!("../../../docs/learn/07_ui.ndg")),
    },
    Lesson {
        name: "a2a",
        title: "8/60 - Agent-to-Agent (A2A) Peer Serving",
        doc: include_str!("../../../docs/learn/08_a2a.md"),
        program: Some(include_str!("../../../docs/learn/08_a2a.ndg")),
    },
    Lesson {
        name: "variables_mut",
        title: "9/60 - Variables and mutable state",
        doc: include_str!("../../../docs/learn/09_variables_mut.md"),
        program: Some(include_str!("../../../docs/learn/09_variables_mut.ndg")),
    },
    Lesson {
        name: "primitives",
        title: "10/60 - Primitive types",
        doc: include_str!("../../../docs/learn/10_primitives.md"),
        program: Some(include_str!("../../../docs/learn/10_primitives.ndg")),
    },
    Lesson {
        name: "functions",
        title: "11/60 - Functions, parameters, and return types",
        doc: include_str!("../../../docs/learn/11_functions.md"),
        program: Some(include_str!("../../../docs/learn/11_functions.ndg")),
    },
    Lesson {
        name: "operators",
        title: "12/60 - Arithmetic, logical, and comparison operators",
        doc: include_str!("../../../docs/learn/12_operators.md"),
        program: Some(include_str!("../../../docs/learn/12_operators.ndg")),
    },
    Lesson {
        name: "string_ops",
        title: "13/60 - String operations",
        doc: include_str!("../../../docs/learn/13_string_ops.md"),
        program: Some(include_str!("../../../docs/learn/13_string_ops.ndg")),
    },
    Lesson {
        name: "records",
        title: "14/60 - Records and type aliases",
        doc: include_str!("../../../docs/learn/14_records.md"),
        program: Some(include_str!("../../../docs/learn/14_records.ndg")),
    },
    Lesson {
        name: "lists",
        title: "15/60 - Lists, indexing, and collection loops",
        doc: include_str!("../../../docs/learn/15_lists.md"),
        program: Some(include_str!("../../../docs/learn/15_lists.ndg")),
    },
    Lesson {
        name: "options_results",
        title: "16/60 - Optional values and error-shaped data",
        doc: include_str!("../../../docs/learn/16_options_results.md"),
        program: Some(include_str!("../../../docs/learn/16_options_results.ndg")),
    },
    Lesson {
        name: "pattern_matching",
        title: "17/60 - Branching before match expressions",
        doc: include_str!("../../../docs/learn/17_pattern_matching.md"),
        program: Some(include_str!("../../../docs/learn/17_pattern_matching.ndg")),
    },
    Lesson {
        name: "loops",
        title: "18/60 - Collection loops",
        doc: include_str!("../../../docs/learn/18_loops.md"),
        program: Some(include_str!("../../../docs/learn/18_loops.ndg")),
    },
    Lesson {
        name: "purity_effects",
        title: "19/60 - Purity and tracked effects",
        doc: include_str!("../../../docs/learn/19_purity_effects.md"),
        program: Some(include_str!("../../../docs/learn/19_purity_effects.ndg")),
    },
    Lesson {
        name: "modules_imports",
        title: "20/60 - Organizing modules",
        doc: include_str!("../../../docs/learn/20_modules_imports.md"),
        program: Some(include_str!("../../../docs/learn/20_modules_imports.ndg")),
    },
    Lesson {
        name: "assertions_contracts",
        title: "21/60 - Assertions and lightweight contracts",
        doc: include_str!("../../../docs/learn/21_assertions_contracts.md"),
        program: Some(include_str!(
            "../../../docs/learn/21_assertions_contracts.ndg"
        )),
    },
    Lesson {
        name: "comments_docs",
        title: "22/60 - Comments and documentation",
        doc: include_str!("../../../docs/learn/22_comments_docs.md"),
        program: Some(include_str!("../../../docs/learn/22_comments_docs.ndg")),
    },
    Lesson {
        name: "prompt_syntax",
        title: "23/60 - Prompt syntax and multi-line blocks",
        doc: include_str!("../../../docs/learn/23_prompt_syntax.md"),
        program: Some(include_str!("../../../docs/learn/23_prompt_syntax.ndg")),
    },
    Lesson {
        name: "prompt_interpolation",
        title: "24/60 - Prompt interpolation and formatting",
        doc: include_str!("../../../docs/learn/24_prompt_interpolation.md"),
        program: Some(include_str!(
            "../../../docs/learn/24_prompt_interpolation.ndg"
        )),
    },
    Lesson {
        name: "structured_schemas",
        title: "25/60 - Nested structured-output schemas",
        doc: include_str!("../../../docs/learn/25_structured_schemas.md"),
        program: Some(include_str!(
            "../../../docs/learn/25_structured_schemas.ndg"
        )),
    },
    Lesson {
        name: "model_budget",
        title: "26/60 - Budgets, token caps, and hard limits",
        doc: include_str!("../../../docs/learn/26_model_budget.md"),
        program: Some(include_str!("../../../docs/learn/26_model_budget.ndg")),
    },
    Lesson {
        name: "model_routing",
        title: "27/60 - Selecting and routing models",
        doc: include_str!("../../../docs/learn/27_model_routing.md"),
        program: Some(include_str!("../../../docs/learn/27_model_routing.ndg")),
    },
    Lesson {
        name: "retry_repair",
        title: "28/60 - Retry and automatic JSON repair",
        doc: include_str!("../../../docs/learn/28_retry_repair.md"),
        program: Some(include_str!("../../../docs/learn/28_retry_repair.ndg")),
    },
    Lesson {
        name: "fallback_models",
        title: "29/60 - Fallback models for failures and timeouts",
        doc: include_str!("../../../docs/learn/29_fallback_models.md"),
        program: Some(include_str!("../../../docs/learn/29_fallback_models.ndg")),
    },
    Lesson {
        name: "few_shot",
        title: "30/60 - Few-shot prompting and context conditioning",
        doc: include_str!("../../../docs/learn/30_few_shot.md"),
        program: Some(include_str!("../../../docs/learn/30_few_shot.ndg")),
    },
    Lesson {
        name: "prompt_clippy",
        title: "31/60 - Prompt Clippy reliability warnings",
        doc: include_str!("../../../docs/learn/31_prompt_clippy.md"),
        program: Some(include_str!("../../../docs/learn/31_prompt_clippy.ndg")),
    },
    Lesson {
        name: "temperature_params",
        title: "32/60 - Temperature, top_p, and seed parameters",
        doc: include_str!("../../../docs/learn/32_temperature_params.md"),
        program: Some(include_str!(
            "../../../docs/learn/32_temperature_params.ndg"
        )),
    },
    Lesson {
        name: "streaming",
        title: "33/60 - Streaming responses and token chunks",
        doc: include_str!("../../../docs/learn/33_streaming.md"),
        program: Some(include_str!("../../../docs/learn/33_streaming.ndg")),
    },
    Lesson {
        name: "system_prompts",
        title: "34/60 - System prompts and role management",
        doc: include_str!("../../../docs/learn/34_system_prompts.md"),
        program: Some(include_str!("../../../docs/learn/34_system_prompts.ndg")),
    },
    Lesson {
        name: "caching_cost",
        title: "35/60 - Prompt caching and cost optimization",
        doc: include_str!("../../../docs/learn/35_caching_cost.md"),
        program: Some(include_str!("../../../docs/learn/35_caching_cost.ndg")),
    },
    Lesson {
        name: "decide_syntax",
        title: "36/60 - decide { ... } on state with { ... }",
        doc: include_str!("../../../docs/learn/36_decide_syntax.md"),
        program: Some(include_str!("../../../docs/learn/36_decide_syntax.ndg")),
    },
    Lesson {
        name: "choose_questions",
        title: "37/60 - choose [a, b, c]: classifying with a winner",
        doc: include_str!("../../../docs/learn/37_choose_questions.md"),
        program: Some(include_str!("../../../docs/learn/37_choose_questions.ndg")),
    },
    Lesson {
        name: "yesno_questions",
        title: "38/60 - yes/no questions and probability p",
        doc: include_str!("../../../docs/learn/38_yesno_questions.md"),
        program: Some(include_str!("../../../docs/learn/38_yesno_questions.ndg")),
    },
    Lesson {
        name: "score_questions",
        title: "39/60 - score [low, medium, high] and weighted scores",
        doc: include_str!("../../../docs/learn/39_score_questions.md"),
        program: Some(include_str!("../../../docs/learn/39_score_questions.ndg")),
    },
    Lesson {
        name: "multi_question_batch",
        title: "40/60 - batch three questions in one decision call",
        doc: include_str!("../../../docs/learn/40_multi_question_batch.md"),
        program: Some(include_str!(
            "../../../docs/learn/40_multi_question_batch.ndg"
        )),
    },
    Lesson {
        name: "confidence_thresholds",
        title: "41/60 - filter with p and confidence thresholds",
        doc: include_str!("../../../docs/learn/41_confidence_thresholds.md"),
        program: Some(include_str!(
            "../../../docs/learn/41_confidence_thresholds.ndg"
        )),
    },
    Lesson {
        name: "route_syntax",
        title: "42/60 - route { arm: expr when cond, otherwise: fallback }",
        doc: include_str!("../../../docs/learn/42_route_syntax.md"),
        program: Some(include_str!("../../../docs/learn/42_route_syntax.ndg")),
    },
    Lesson {
        name: "human_in_the_loop",
        title: "43/60 - escalate low-confidence decisions to a human",
        doc: include_str!("../../../docs/learn/43_human_in_the_loop.md"),
        program: Some(include_str!("../../../docs/learn/43_human_in_the_loop.ndg")),
    },
    Lesson {
        name: "policy_sweep",
        title: "44/60 - calibrate thresholds offline with policy-sweep",
        doc: include_str!("../../../docs/learn/44_policy_sweep.md"),
        program: Some(include_str!("../../../docs/learn/44_policy_sweep.ndg")),
    },
    Lesson {
        name: "injection_guard",
        title: "45/60 - gen.injection() and prompt-injection defenses",
        doc: include_str!("../../../docs/learn/45_injection_guard.md"),
        program: Some(include_str!("../../../docs/learn/45_injection_guard.ndg")),
    },
    Lesson {
        name: "decision_caching",
        title: "46/60 - cache repeated decisions with NUDGE_DECISION_CACHE",
        doc: include_str!("../../../docs/learn/46_decision_caching.md"),
        program: Some(include_str!("../../../docs/learn/46_decision_caching.ndg")),
    },
    Lesson {
        name: "explain_reports",
        title: "47/60 - audit decisions with nudgec explain",
        doc: include_str!("../../../docs/learn/47_explain_reports.md"),
        program: Some(include_str!("../../../docs/learn/47_explain_reports.ndg")),
    },
    Lesson {
        name: "agent_state",
        title: "48/60 - Agent state and durable mutations",
        doc: include_str!("../../../docs/learn/48_agent_state.md"),
        program: Some(include_str!("../../../docs/learn/48_agent_state.ndg")),
    },
    Lesson {
        name: "agent_actions",
        title: "49/60 - Agent actions and effect labels",
        doc: include_str!("../../../docs/learn/49_agent_actions.md"),
        program: Some(include_str!("../../../docs/learn/49_agent_actions.ndg")),
    },
    Lesson {
        name: "ui_views",
        title: "50/60 - Typed views with columns, rows, and controls",
        doc: include_str!("../../../docs/learn/50_ui_views.md"),
        program: Some(include_str!("../../../docs/learn/50_ui_views.ndg")),
    },
    Lesson {
        name: "ui_headless_test",
        title: "51/60 - Headless UI tests",
        doc: include_str!("../../../docs/learn/51_ui_headless_test.md"),
        program: Some(include_str!("../../../docs/learn/51_ui_headless_test.ndg")),
    },
    Lesson {
        name: "mcp_tools",
        title: "52/60 - Publish functions through MCP",
        doc: include_str!("../../../docs/learn/52_mcp_tools.md"),
        program: Some(include_str!("../../../docs/learn/52_mcp_tools.ndg")),
    },
    Lesson {
        name: "a2a_agent_card",
        title: "53/60 - A2A Agent Cards, capabilities, and skills",
        doc: include_str!("../../../docs/learn/53_a2a_agent_card.md"),
        program: Some(include_str!("../../../docs/learn/53_a2a_agent_card.ndg")),
    },
    Lesson {
        name: "a2a_peer_serving",
        title: "54/60 - Serve an A2A peer over REST and JSON-RPC",
        doc: include_str!("../../../docs/learn/54_a2a_peer_serving.md"),
        program: Some(include_str!("../../../docs/learn/54_a2a_peer_serving.ndg")),
    },
    Lesson {
        name: "deterministic_fakes",
        title: "55/60 - Deterministic fakes: offline and zero cost",
        doc: include_str!("../../../docs/learn/55_deterministic_fakes.md"),
        program: Some(include_str!(
            "../../../docs/learn/55_deterministic_fakes.ndg"
        )),
    },
    Lesson {
        name: "property_fuzzing",
        title: "56/60 - Property fuzzing and shrinking",
        doc: include_str!("../../../docs/learn/56_property_fuzzing.md"),
        program: Some(include_str!("../../../docs/learn/56_property_fuzzing.ndg")),
    },
    Lesson {
        name: "ntf_traces",
        title: "57/60 - Open NTF traces",
        doc: include_str!("../../../docs/learn/57_ntf_traces.md"),
        program: Some(include_str!("../../../docs/learn/57_ntf_traces.ndg")),
    },
    Lesson {
        name: "trace_replay",
        title: "58/60 - Byte-identical trace replay",
        doc: include_str!("../../../docs/learn/58_trace_replay.md"),
        program: Some(include_str!("../../../docs/learn/58_trace_replay.ndg")),
    },
    Lesson {
        name: "trace_diff_ci",
        title: "59/60 - Trace diffs as CI regression gates",
        doc: include_str!("../../../docs/learn/59_trace_diff_ci.md"),
        program: Some(include_str!("../../../docs/learn/59_trace_diff_ci.ndg")),
    },
    Lesson {
        name: "compilation_targets",
        title: "60/60 - Python, TypeScript, and standalone Rust targets",
        doc: include_str!("../../../docs/learn/60_compilation_targets.md"),
        program: Some(include_str!(
            "../../../docs/learn/60_compilation_targets.ndg"
        )),
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
    println!("{}", lesson.body().trim_end());
    if let Some(program) = lesson.program {
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
            let Some(src) = l.program else { continue };
            let tokens = crate::lexer::lex(src)
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
            assert!(l.doc.contains("Try:") || l.doc.contains("know the whole loop"));
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
