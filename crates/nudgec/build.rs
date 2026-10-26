use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../../docs/learn");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("learn_curriculum.rs");

    // Exact curriculum sequence (100 lessons)
    let lesson_stems = [
        // Part 1: Core Essentials & Philosophy (1-8)
        "hello",
        "types",
        "decide",
        "policy",
        "test",
        "trace",
        "ui",
        "a2a",
        // Part 2: Language Fundamentals & Type System (9-22)
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
        // Part 3: LLM Integration & Prompt Engineering (23-35)
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
        // Part 4: JEV Decision Engine & Probabilistic Policies (36-47)
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
        // Part 5: Agent Architecture, UI & Tooling (48-60)
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
        // Part 6: Agent-to-Agent Mesh & Distributed Coordination (61-70)
        "a2a_task_lifecycle",
        "a2a_jsonrpc",
        "a2a_auth_bearer",
        "a2a_discovery",
        "a2a_streaming_chunks",
        "a2a_delegation",
        "a2a_rate_limits",
        "a2a_schema_negotiation",
        "a2a_circuit_breaker",
        "a2a_trace_context",
        // Part 7: Durable State & Enterprise Resumability (71-80)
        "state_persistence",
        "state_rollback",
        "checkpoint_resume",
        "concurrency_locks",
        "history_compaction",
        "ephemeral_vs_durable",
        "idempotent_actions",
        "time_expiration",
        "cache_invalidation",
        "state_migration",
        // Part 8: Tool Orchestration & Multi-Modal Pipelines (81-90)
        "mcp_resource_reading",
        "mcp_side_effects",
        "tool_pipeline_chaining",
        "tool_fallback_handling",
        "image_input_inspection",
        "audio_transcription",
        "dynamic_tool_registration",
        "tool_result_validation",
        "tool_timeout_interruption",
        "sandbox_isolation",
        // Part 9: Advanced Production, Governance & AI Act (91-100)
        "cost_governance",
        "audit_compliance_ai_act",
        "canary_deployments",
        "dataset_eval_scoring",
        "trace_diff_regressions",
        "secret_redaction",
        "multi_tenant_isolation",
        "zero_trust_compiler_gates",
        "standalone_native_packaging",
        "mastery_capstone",
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
