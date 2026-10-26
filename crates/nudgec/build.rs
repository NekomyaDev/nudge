use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../../docs/learn");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("learn_curriculum.rs");

    // Exact curriculum sequence (60 lessons)
    let lesson_stems = [
        "hello",
        "types",
        "decide",
        "policy",
        "test",
        "trace",
        "ui",
        "a2a",
        "variables_mut",
        "primitives",
        "functions",
        "operators",
        "string_ops",
        "records",
        "lists",
        "options_results",
        "pattern_matching",
        "loops",
        "purity_effects",
        "modules_imports",
        "assertions_contracts",
        "comments_docs",
        "prompt_syntax",
        "prompt_interpolation",
        "structured_schemas",
        "model_budget",
        "model_routing",
        "retry_repair",
        "fallback_models",
        "few_shot",
        "prompt_clippy",
        "temperature_params",
        "streaming",
        "system_prompts",
        "caching_cost",
        "decide_syntax",
        "choose_questions",
        "yesno_questions",
        "score_questions",
        "multi_question_batch",
        "confidence_thresholds",
        "route_syntax",
        "human_in_the_loop",
        "policy_sweep",
        "injection_guard",
        "decision_caching",
        "explain_reports",
        "agent_state",
        "agent_actions",
        "ui_views",
        "ui_headless_test",
        "mcp_tools",
        "a2a_agent_card",
        "a2a_peer_serving",
        "deterministic_fakes",
        "property_fuzzing",
        "ntf_traces",
        "trace_replay",
        "trace_diff_ci",
        "compilation_targets",
    ];

    let mut generated = String::new();
    generated.push_str("pub const LESSONS: &[Lesson] = &[\n");

    for stem in &lesson_stems {
        generated.push_str(&format!(
            "    Lesson::from_source({:?}, include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../../docs/learn/{}.ndg\"))),\n",
            stem, stem
        ));
    }
    generated.push_str("];\n");

    fs::write(&dest_path, generated).unwrap();
}
