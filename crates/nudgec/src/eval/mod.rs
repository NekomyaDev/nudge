pub mod env;
pub mod interpreter;
pub mod val;

pub use interpreter::run_program;
#[allow(unused_imports)]
pub use interpreter::run_program_with_args;
pub use val::{parse_cli_arg, NudgeVal};

#[cfg(all(test, feature = "compiler"))]
mod tests {
    use super::*;
    use crate::lexer;
    use crate::parser;

    fn parse_src(src: &str) -> Vec<crate::ast::Item> {
        let tokens = lexer::lex(src).expect("lexing failed");
        parser::parse(tokens).expect("parsing failed")
    }

    #[test]
    fn test_eval_simple_add() {
        let src = r#"
            fn add(a: int, b: int) -> int {
                a + b
            }
            fn main() -> int {
                add(10, 32)
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(res, NudgeVal::Int(42));
    }

    #[test]
    fn test_eval_route() {
        let src = r#"
            fn choose(x: int) -> string {
                route {
                    first: "small" when x < 5,
                    second: "medium" when x <= 10,
                    last: "large" otherwise
                }
            }
            fn main() -> string {
                choose(7)
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(res, NudgeVal::Str("medium".to_string()));
    }

    #[test]
    fn test_eval_agent_state() {
        let src = r#"
            agent Counter {
                state {
                    count: int = 0,
                    history: [string] = []
                }
                fn inc(step: int) -> int {
                    state.count += step
                    state.history += ["incremented"]
                    state.count
                }
                fn get() -> int {
                    state.count
                }
                fn main() -> int {
                    let c1 = inc(5)
                    let c2 = inc(3)
                    get()
                }
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(res, NudgeVal::Int(8));
    }

    #[test]
    fn test_eval_decide_and_confidence() {
        let src = r#"
            fn triage(t: string) -> string {
                let d = decide {
                    dept: "which team should handle?" choose [billing, tech, security]
                }
                on t
                with { model: "fake:default" }

                route {
                    high: d.dept.winner when d.dept.confidence > 0.0,
                    fallback: "manual" otherwise
                }
            }
            fn main() -> string {
                triage("refund needed")
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert!(matches!(res, NudgeVal::Str(_)));
    }

    #[test]
    fn test_eval_merge() {
        let src = r#"
            fn main() -> [int] {
                let a = [1, 2, 3]
                let b = [2, 3, 4, 5]
                a | merge b
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(
            res,
            NudgeVal::List(vec![
                NudgeVal::Int(1),
                NudgeVal::Int(2),
                NudgeVal::Int(3),
                NudgeVal::Int(4),
                NudgeVal::Int(5),
            ])
        );
    }

    #[test]
    fn test_eval_zip_and_len() {
        let src = r#"
            fn main() -> int {
                let a = [10, 20]
                let b = [30, 40]
                let z = zip(a, b)
                len(z)
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(res, NudgeVal::Int(2));
    }

    #[test]
    fn test_eval_clean_diagnostics_unsupported() {
        let _g = eval_guard();
        let src_stream = r#"
            fn main() -> int {
                stream let x = 42
                x
            }
        "#;
        let items = parse_src(src_stream);
        let err = run_program(&items, "main", &[]).unwrap_err();
        assert!(err.contains("E-native-0100"), "got {err}");

        let src_tool = r#"
            tool test_tool() -> int {
                impl: mcp("kb").test_tool()
                side_effects: none
            }
            fn main() -> int {
                test_tool()
            }
        "#;
        let items = parse_src(src_tool);
        std::env::set_var("NUDGE_PROVIDER", "openai");
        let err = run_program(&items, "main", &[]).unwrap_err();
        std::env::set_var("NUDGE_PROVIDER", "fake");
        assert!(err.contains("E-native-0201"), "got {err}");
    }

    #[test]
    fn test_eval_llm_call_fake_offline() {
        let _g = eval_guard();
        std::env::set_var("NUDGE_PROVIDER", "fake");
        let src = r#"
            fn main() -> string uses LLM {
                llm"""Hello Nudge Language!""" with { model: "fake:default" }
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(
            res,
            NudgeVal::Str("fake: Hello Nudge Language!".to_string())
        );
    }

    #[test]
    fn test_eval_llm_structured_output_fake() {
        let _g = eval_guard();
        std::env::set_var("NUDGE_PROVIDER", "fake");
        let src = r#"
            type Smoke = { title: string, confidence: float }
            fn main() -> string uses LLM {
                let r = llm"""Get title""" with { schema: Smoke, model: "fake:default" }
                r.title
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(res, NudgeVal::Str("smoke test".to_string()));
    }

    #[test]
    fn test_list_schema_representation() {
        use crate::ast::{ExprKind, Item, StmtKind};
        use crate::eval::interpreter::schema_expr_to_string;

        let src = r#"
            type Finding = { claim: string }
            fn f() -> [Finding] uses LLM {
                llm"""extract""" with { schema: [Finding] }
            }
            fn g() -> [[Finding]] uses LLM {
                llm"""nested""" with { schema: [[Finding]] }
            }
            fn h() -> [string] uses LLM {
                llm"""strings""" with { schema: [string] }
            }
            fn single() -> Finding uses LLM {
                llm"""single""" with { schema: Finding }
            }
        "#;
        let items = parse_src(src);

        // schema: [Finding]
        let Item::Fn { body, .. } = &items[1] else {
            panic!("expected fn f")
        };
        let StmtKind::ExprStmt(expr) = &body[0].kind else {
            panic!("expected expr")
        };
        let ExprKind::LlmCall { options, .. } = &expr.kind else {
            panic!("expected llm call")
        };
        let (_, s_expr) = options
            .iter()
            .find(|(k, _)| k == "schema")
            .expect("schema option");
        assert_eq!(schema_expr_to_string(s_expr), Some("[Finding]".to_string()));

        // schema: [[Finding]]
        let Item::Fn { body, .. } = &items[2] else {
            panic!("expected fn g")
        };
        let StmtKind::ExprStmt(expr) = &body[0].kind else {
            panic!("expected expr")
        };
        let ExprKind::LlmCall { options, .. } = &expr.kind else {
            panic!("expected llm call")
        };
        let (_, s_expr) = options
            .iter()
            .find(|(k, _)| k == "schema")
            .expect("schema option");
        assert_eq!(
            schema_expr_to_string(s_expr),
            Some("[[Finding]]".to_string())
        );

        // schema: [string]
        let Item::Fn { body, .. } = &items[3] else {
            panic!("expected fn h")
        };
        let StmtKind::ExprStmt(expr) = &body[0].kind else {
            panic!("expected expr")
        };
        let ExprKind::LlmCall { options, .. } = &expr.kind else {
            panic!("expected llm call")
        };
        let (_, s_expr) = options
            .iter()
            .find(|(k, _)| k == "schema")
            .expect("schema option");
        assert_eq!(schema_expr_to_string(s_expr), Some("[string]".to_string()));

        // schema: Finding
        let Item::Fn { body, .. } = &items[4] else {
            panic!("expected fn single")
        };
        let StmtKind::ExprStmt(expr) = &body[0].kind else {
            panic!("expected expr")
        };
        let ExprKind::LlmCall { options, .. } = &expr.kind else {
            panic!("expected llm call")
        };
        let (_, s_expr) = options
            .iter()
            .find(|(k, _)| k == "schema")
            .expect("schema option");
        assert_eq!(schema_expr_to_string(s_expr), Some("Finding".to_string()));
    }

    #[test]
    fn test_eval_llm_structured_list_schema_output_fake() {
        let _g = eval_guard();
        std::env::set_var("NUDGE_PROVIDER", "fake");
        let src = r#"
            type Finding = { claim: string, source: string, confidence: float }
            fn main() -> [Finding] uses LLM {
                llm"""Find things""" with { schema: [Finding], model: "fake:default" }
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        let NudgeVal::List(list) = res else {
            panic!("expected list result")
        };
        assert_eq!(list.len(), 3);
        let NudgeVal::Map(first) = &list[0] else {
            panic!("expected map element")
        };
        assert!(first.contains_key("claim"));
        assert!(first.contains_key("source"));
        assert!(first.contains_key("confidence"));
    }

    #[test]
    fn test_eval_par_map() {
        let src = r#"
            fn double(x: int) -> int {
                x * 2
            }
            fn main() -> [int] {
                let nums = [1, 2, 3]
                par map(nums) |x| -> double(x)
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(
            res,
            NudgeVal::List(vec![NudgeVal::Int(2), NudgeVal::Int(4), NudgeVal::Int(6)])
        );
    }

    #[test]
    fn test_eval_fake_tool() {
        let _g = eval_guard();
        std::env::set_var("NUDGE_PROVIDER", "fake");
        let src = r#"
            type Chunk = { doc: string, text: string, score: float }
            tool retrieve(q: string, k: int) -> [Chunk] {
                impl: mcp("kb").retrieve(q)
                side_effects: none
            }
            fn main() -> int uses Tool {
                let chunks = retrieve("test query", 2)
                len(chunks)
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(res, NudgeVal::Int(1));
    }

    #[test]
    fn test_eval_with_args_success() {
        let src = r#"
            fn handle(t: string) -> string {
                t
            }
        "#;
        let items = parse_src(src);
        let args = vec![NudgeVal::Str("urgent: double charged".into())];
        let res = run_program_with_args(&items, "handle", &args).expect("execution failed");
        assert_eq!(res, NudgeVal::Str("urgent: double charged".into()));
    }

    #[test]
    fn test_eval_with_args_multiple() {
        let src = r#"
            fn add_two(a: int, b: int) -> int {
                a + b
            }
        "#;
        let items = parse_src(src);
        let args = vec![NudgeVal::Int(10), NudgeVal::Int(32)];
        let res = run_program_with_args(&items, "add_two", &args).expect("execution failed");
        assert_eq!(res, NudgeVal::Int(42));
    }

    #[test]
    fn test_eval_arity_mismatch_too_few() {
        let src = r#"
            fn handle(t: string) -> string {
                t
            }
        "#;
        let items = parse_src(src);
        let err = run_program_with_args(&items, "handle", &[]).unwrap_err();
        assert_eq!(err, "entry 'handle' expects 1 argument (t: string), got 0");
    }

    #[test]
    fn test_eval_arity_mismatch_too_many() {
        let src = r#"
            fn main() -> int {
                42
            }
        "#;
        let items = parse_src(src);
        let args = vec![NudgeVal::Int(1)];
        let err = run_program_with_args(&items, "main", &args).unwrap_err();
        assert_eq!(err, "entry 'main' expects 0 arguments, got 1");
    }

    #[test]
    fn test_eval_arity_mismatch_multiple_params() {
        let src = r#"
            fn pair(a: int, b: string) -> string {
                b
            }
        "#;
        let items = parse_src(src);
        let err = run_program_with_args(&items, "pair", &[]).unwrap_err();
        assert_eq!(
            err,
            "entry 'pair' expects 2 arguments (a: int, b: string), got 0"
        );
    }

    // ── computer bridge (v1.5): the native interpreter speaks the same
    //    computer-use surface as the Python/TS runtimes and the VM ────

    /// The computer statics (last-observation authority, replay cursors),
    /// the NUDGE_* bridge env vars and the trace/replay state are
    /// process-global — serialize the bridge + LLM trace/replay tests.
    fn computer_guard() -> std::sync::MutexGuard<'static, ()> {
        interpreter::tests::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Same lock for every test that runs LLM calls or touches the
    /// NUDGE_PROVIDER / NUDGE_REPLAY / NUDGE_TRACE bridge env.
    fn eval_guard() -> std::sync::MutexGuard<'static, ()> {
        computer_guard()
    }

    /// Removes an env var on drop so a panicking test never leaks it into
    /// sibling tests (or their python subprocesses).
    struct EnvReset(&'static str);
    impl Drop for EnvReset {
        fn drop(&mut self) {
            std::env::remove_var(self.0);
        }
    }

    #[test]
    fn test_eval_computer_fake_desktop_offline() {
        let _g = computer_guard();
        nudge_runtime::stdlib::computer::reset_authority_for_tests();
        // no env needed: the deterministic fake desktop is the default
        // provider — observe/click work fully offline
        let src = r#"
            fn look(app: string) -> string uses Computer {
                let obs = computer.observe(app, allow = [app], deadline = 5000)
                obs.tree
            }
            fn press() -> bool uses Computer {
                let obs = computer.observe("FakeApp")
                let r = computer.click(1)
                let t = computer.type("hello")
                r.ok and t.ok
            }
        "#;
        let items = parse_src(src);
        let tree = run_program(&items, "look", &[NudgeVal::Str("FakeApp".into())])
            .expect("native computer.observe works");
        // the zcode tree rendered from the fake scene (byte-parity with
        // the Python/TS runtimes)
        assert_eq!(
            tree,
            NudgeVal::Str(
                "[0] window \"FakeApp\"\n[1] button \"OK\" actions=['press'] (pressable)\n[2] button \"Cancel\" actions=['press'] (pressable)\n[3] textfield \"Search\" (focused,editable)"
                    .into()
            )
        );
        let ok = run_program(&items, "press", &[]).expect("native computer.click works");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_eval_computer_errors_surface() {
        let _g = computer_guard();
        nudge_runtime::stdlib::computer::reset_authority_for_tests();
        // an action without a prior observe is a clear E-native failure —
        // the stdlib error propagates, never a silent success
        let src = r#"
            fn main() -> bool uses Computer {
                let r = computer.click(1)
                r.ok
            }
        "#;
        let items = parse_src(src);
        let err = run_program(&items, "main", &[]).unwrap_err();
        assert!(err.contains("E-native-0202"), "{err}");
        assert!(err.contains("computer.click"), "{err}");
        assert!(err.contains("observe"), "{err}");
        // unknown options fail loudly instead of passing through
        let src = r#"
            fn main() -> bool uses Computer {
                let r = computer.click(1, mode = "yolo")
                r.ok
            }
        "#;
        let items = parse_src(src);
        let err = run_program(&items, "main", &[]).unwrap_err();
        assert!(err.contains("E-native-0203"), "{err}");
        assert!(err.contains("mode"), "{err}");
    }

    #[test]
    fn test_eval_computer_replay_dry_run() {
        let _g = computer_guard();
        nudge_runtime::stdlib::computer::reset_authority_for_tests();
        let _cleanup = EnvReset("NUDGE_REPLAY");
        let trace = std::env::temp_dir().join(format!(
            "nudge-eval-computer-replay-{}.jsonl",
            std::process::id()
        ));
        std::fs::write(
            &trace,
            concat!(
                "{\"kind\":\"computer.observe\",\"app\":\"FakeApp\",\"state_id\":\"s-9\",\"elements\":[]}\n",
                "{\"kind\":\"computer.act\",\"action\":\"click\",\"app\":\"FakeApp\",\"target\":{\"index\":1},\"ok\":true,\"outcome\":\"ok\",\"latency_ms\":3}\n",
            ),
        )
        .expect("write trace");
        let src = r#"
            fn main() -> bool uses Computer {
                let obs = computer.observe("FakeApp", allow = ["FakeApp"])
                let r = computer.click(1)
                r.ok
            }
        "#;
        let items = parse_src(src);
        std::env::set_var("NUDGE_REPLAY", &trace);
        let res = run_program(&items, "main", &[]);
        assert_eq!(
            res.expect("replayed observe + dry-run click"),
            NudgeVal::Bool(true)
        );

        // a program that DIVERGES from the trace raises ReplayMismatch —
        // wrapped as a clear E-native error, never a silent replay
        nudge_runtime::stdlib::computer::reset_authority_for_tests();
        let diverging = r#"
            fn main() -> bool uses Computer {
                let obs = computer.observe("FakeApp")
                let r = computer.key("Return")
                r.ok
            }
        "#;
        let items = parse_src(diverging);
        let err = run_program(&items, "main", &[]).unwrap_err();
        assert!(err.contains("E-native-0202"), "{err}");
        assert!(err.contains("ReplayMismatch"), "{err}");
        assert!(err.contains("computer.key"), "{err}");
    }

    // ── NTF v1 trace writing + deterministic LLM replay ──────────────

    /// Unique temp trace path per test (process id + label).
    fn trace_tmp(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("nudge-eval-{label}-{}.jsonl", std::process::id()))
    }

    #[test]
    fn test_eval_llm_trace_recorded_and_validates() {
        let _g = eval_guard();
        std::env::set_var("NUDGE_PROVIDER", "fake");
        let path = trace_tmp("trace");
        let _ = std::fs::remove_file(&path);
        let _scope = nudge_runtime::trace::testing::scope(Some(path.clone()), None);
        let src = r#"
            fn main() -> string uses LLM {
                llm"""Hello trace""" with { model: "fake:default" }
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(res, NudgeVal::Str("fake: Hello trace".to_string()));

        let text = std::fs::read_to_string(&path).expect("trace file written");
        // every record MUST conform to the frozen v1 schema
        assert_eq!(crate::tracecheck::validate(&text), Vec::<String>::new());
        let mut lines = text.lines();
        let rec: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(rec["v"], 1);
        assert_eq!(rec["seq"], 1);
        assert_eq!(rec["kind"], "llm.call");
        assert_eq!(rec["model"], "fake:default");
        assert_eq!(rec["input"], "Hello trace");
        assert_eq!(rec["output"], "fake: Hello trace");
        assert_eq!(rec["params"]["temperature"], 0);
        assert_eq!(rec["outcome"], "ok");
        assert_eq!(rec["repair_round"], 0);
        assert_eq!(rec["provider"], "fake");
        assert!(rec["cost_usd"].is_number());
        assert!(rec["tokens"]["in"].is_number());
        assert!(rec["tokens"]["out"].is_number());
        let hash = rec["request_hash"].as_str().expect("request_hash");
        assert!(hash.starts_with("sha256:"), "{hash}");
        // the traced function boundary lands as `fn.return` (parity with
        // the Python runtime's @effectful wrapper)
        let fret: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(fret["kind"], "fn.return");
        assert_eq!(fret["fn"], "main");
        assert_eq!(fret["output"], "fake: Hello trace");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_eval_llm_replay_round_trip() {
        let _g = eval_guard();
        std::env::set_var("NUDGE_PROVIDER", "fake");
        let src = r#"
            fn main() -> string uses LLM {
                llm"""round trip""" with { model: "fake:default" }
            }
        "#;
        let path = trace_tmp("roundtrip");
        let _ = std::fs::remove_file(&path);
        // record
        {
            let _scope = nudge_runtime::trace::testing::scope(Some(path.clone()), None);
            let items = parse_src(src);
            let live = run_program(&items, "main", &[]).expect("record run");
            assert_eq!(live, NudgeVal::Str("fake: round trip".to_string()));
        }
        // replay: the recorded answer is served verbatim (never a provider)
        {
            let _scope = nudge_runtime::trace::testing::scope(None, Some(path.clone()));
            let items = parse_src(src);
            let replayed = run_program(&items, "main", &[]).expect("replay run");
            assert_eq!(replayed, NudgeVal::Str("fake: round trip".to_string()));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_eval_llm_replay_mismatch_is_loud() {
        let _g = eval_guard();
        std::env::set_var("NUDGE_PROVIDER", "fake");
        let recorded = r#"
            fn main() -> string uses LLM {
                llm"""alpha""" with { model: "fake:default" }
            }
        "#;
        let changed = r#"
            fn main() -> string uses LLM {
                llm"""beta""" with { model: "fake:default" }
            }
        "#;
        let path = trace_tmp("mismatch");
        let _ = std::fs::remove_file(&path);
        {
            let _scope = nudge_runtime::trace::testing::scope(Some(path.clone()), None);
            let items = parse_src(recorded);
            run_program(&items, "main", &[]).expect("record run");
        }
        {
            // the program changed its prompt: no recorded llm.call may be
            // consumed — a loud ReplayMismatch, never a silent provider call
            let _scope = nudge_runtime::trace::testing::scope(None, Some(path.clone()));
            let items = parse_src(changed);
            let err = run_program(&items, "main", &[]).unwrap_err();
            assert!(err.contains("E-native-0302"), "{err}");
            assert!(err.contains("ReplayMismatch"), "{err}");
            assert!(err.contains("request_hash"), "{err}");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_eval_llm_replay_exhaustion_is_loud() {
        let _g = eval_guard();
        std::env::set_var("NUDGE_PROVIDER", "fake");
        let one_call = r#"
            fn main() -> string uses LLM {
                llm"""alpha""" with { model: "fake:default" }
            }
        "#;
        let two_calls = r#"
            fn main() -> string uses LLM {
                let a = llm"""alpha""" with { model: "fake:default" }
                let b = llm"""alpha""" with { model: "fake:default" }
                a
            }
        "#;
        let path = trace_tmp("exhaust");
        let _ = std::fs::remove_file(&path);
        {
            let _scope = nudge_runtime::trace::testing::scope(Some(path.clone()), None);
            let items = parse_src(one_call);
            run_program(&items, "main", &[]).expect("record run");
        }
        {
            let _scope = nudge_runtime::trace::testing::scope(None, Some(path.clone()));
            let items = parse_src(two_calls);
            let err = run_program(&items, "main", &[]).unwrap_err();
            assert!(err.contains("E-native-0302"), "{err}");
            assert!(err.contains("more llm calls than the trace"), "{err}");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_eval_computer_trace_records() {
        let _g = eval_guard();
        nudge_runtime::stdlib::computer::reset_authority_for_tests();
        let path = trace_tmp("computer");
        let _ = std::fs::remove_file(&path);
        let _scope = nudge_runtime::trace::testing::scope(Some(path.clone()), None);
        let src = r#"
            fn main() -> bool uses Computer {
                let obs = computer.observe("FakeApp", allow = ["FakeApp"])
                let r = computer.click(1)
                r.ok
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("recorded computer run");
        assert_eq!(res, NudgeVal::Bool(true));

        let text = std::fs::read_to_string(&path).expect("computer trace written");
        assert_eq!(crate::tracecheck::validate(&text), Vec::<String>::new());
        assert!(text.contains("computer.observe"), "{text}");
        assert!(text.contains("computer.act"), "{text}");
        assert!(text.contains("\"action\": \"click\""), "{text}");
        assert!(text.contains("\"state_id\": \"s-1\""), "{text}");
        assert!(text.contains("\"element_count\": 4"), "{text}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_ui_headless_acceptance_and_reactivity() {
        let src = r#"
            view CounterView(count: int) -> View {
                column [
                    text("Sayı: " + count),
                    button("Artır", id = "btn_inc", on_click = inc)
                ]
            }

            agent CounterApp {
                state {
                    val: int = 0
                }

                fn inc() uses UI {
                    state.val = state.val + 1
                }

                window Win {
                    title: "Test Penceresi",
                    content: CounterView(state.val)
                }
            }

            test "pencere acilisi, girdi tetiklemesi ve gorunum guncellemesi" {
                // 1. Pencere baslatma (Headless sanal agac modunda)
                let app = CounterApp.Win.mount_headless()
                assert app.find("btn_inc").exists()
                assert app.find_text("Sayı: 0").exists()

                // 2. Sentetik olay tetikleme
                app.dispatch_click("btn_inc")

                // 3. Reaktif guncelleme dogrulamasi
                assert app.find_text("Sayı: 1").exists()
                assert CounterApp.state.val == 1
            }
        "#;
        let items = parse_src(src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load_items failed");

        // Run the test block as written in the specification
        interp.run_all_tests().expect("test execution failed");

        // Direct assertions on runtime state and view tree
        let final_val = interp
            .agent_store
            .get_field("CounterApp", "val")
            .expect("read CounterApp.state.val");
        assert_eq!(final_val, NudgeVal::Int(1));

        // Mount fresh headless instance to inspect initial -> click -> updated step by step
        let env = std::rc::Rc::new(std::cell::RefCell::new(crate::eval::env::Env::new()));
        let win_ref = NudgeVal::WindowRef {
            agent: Some("CounterApp".to_string()),
            name: "Win".to_string(),
        };
        env.borrow_mut().define("Win".to_string(), win_ref);
        let mount_call = crate::ast::Expr {
            kind: crate::ast::ExprKind::Call {
                func: Box::new(crate::ast::Expr {
                    kind: crate::ast::ExprKind::Field {
                        obj: Box::new(crate::ast::Expr {
                            kind: crate::ast::ExprKind::Ident("Win".to_string()),
                            span: crate::ast::Span { start: 0, end: 0 },
                        }),
                        name: "mount_headless".to_string(),
                    },
                    span: crate::ast::Span { start: 0, end: 0 },
                }),
                args: vec![],
                kwargs: vec![],
            },
            span: crate::ast::Span { start: 0, end: 0 },
        };
        let app_val = interp.eval_expr(&mount_call, &env).expect("mount headless");
        env.borrow_mut().define("app".to_string(), app_val);

        // Initial state
        let find_0 = crate::ast::Expr {
            kind: crate::ast::ExprKind::Call {
                func: Box::new(crate::ast::Expr {
                    kind: crate::ast::ExprKind::Field {
                        obj: Box::new(crate::ast::Expr {
                            kind: crate::ast::ExprKind::Ident("app".to_string()),
                            span: crate::ast::Span { start: 0, end: 0 },
                        }),
                        name: "find_text".to_string(),
                    },
                    span: crate::ast::Span { start: 0, end: 0 },
                }),
                args: vec![crate::ast::Expr {
                    kind: crate::ast::ExprKind::Str("Sayı: 1".to_string()),
                    span: crate::ast::Span { start: 0, end: 0 },
                }],
                kwargs: vec![],
            },
            span: crate::ast::Span { start: 0, end: 0 },
        };
        let _res = interp.eval_expr(&find_0, &env).expect("find text");
        let exists_call = crate::ast::Expr {
            kind: crate::ast::ExprKind::Call {
                func: Box::new(crate::ast::Expr {
                    kind: crate::ast::ExprKind::Field {
                        obj: Box::new(find_0.clone()),
                        name: "exists".to_string(),
                    },
                    span: crate::ast::Span { start: 0, end: 0 },
                }),
                args: vec![],
                kwargs: vec![],
            },
            span: crate::ast::Span { start: 0, end: 0 },
        };
        assert_eq!(
            interp.eval_expr(&exists_call, &env).unwrap(),
            NudgeVal::Bool(true)
        );
    }
}
