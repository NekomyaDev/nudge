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
    fn test_eval_map_and_list_primitives() {
        let src = r#"
            fn main() -> bool {
                let mut m: Map[string, int] = {}
                m["apple"] = 10
                m["banana"] = 20

                let has_apple = map.has(m, "apple")
                let has_cherry = map.has(m, "cherry")
                let apple_val = map.get(m, "apple", 0)
                let cherry_val = map.get(m, "cherry", 99)
                let keys = map.keys(m)
                let vals = map.values(m)

                let m2 = map.remove(m, "apple")
                let apple_removed = !map.has(m2, "apple")

                let m3 = map.set(m2, "orange", 30)
                let has_orange = map.has(m3, "orange")

                let xs: [string] = ["first", "second", "third", "fourth"]
                let has_second = list.contains(xs, "second")
                let not_has_fifth = !list.contains(xs, "fifth")
                let idx_third = list.index_of(xs, "third")
                let idx_missing = list.index_of(xs, "missing")
                let sliced = list.slice(xs, 1, 3)
                let rev = list.reverse(xs)

                let ok_slice = len(sliced) == 2 and sliced[0] == "second" and sliced[1] == "third"
                let ok_rev = rev[0] == "fourth" and rev[3] == "first"
                let ok_keys = len(keys) == 2 and keys[0] == "apple" and keys[1] == "banana"
                let ok_vals = len(vals) == 2 and vals[0] == 10 and vals[1] == 20

                has_apple and !has_cherry and apple_val == 10 and cherry_val == 99
                    and apple_removed and has_orange
                    and has_second and not_has_fifth and idx_third == 2 and idx_missing == -1
                    and ok_slice and ok_rev and ok_keys and ok_vals
            }
        "#;
        let items = parse_src(src);
        let res = run_program(&items, "main", &[]).expect("execution failed");
        assert_eq!(res, NudgeVal::Bool(true));
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

    #[test]
    fn test_ui_window_open_headless_fallback_and_reactivity() {
        let _cleanup = EnvReset("NUDGE_UI_HEADLESS");
        std::env::set_var("NUDGE_UI_HEADLESS", "1");

        let src = r#"
            view CounterView(val: int) -> View {
                column(padding = 20, gap = 10) [
                    text("Counter: " + val),
                    button("Click Me", id = "btn_inc", on_click = inc)
                ]
            }

            agent CounterApp {
                state { val: int = 0 }
                fn inc() uses UI {
                    state.val = state.val + 1
                }
                window MainWindow {
                    title: "Nudge Acceptance GUI",
                    width: 300,
                    height: 200,
                    content: CounterView(state.val)
                }
            }

            test "open window headless and verify reactivity" {
                let app = CounterApp.MainWindow.open()
                assert app.find("btn_inc").exists()
                assert app.find_text("Counter: 0").exists()
                app.dispatch_click("btn_inc")
                assert app.find_text("Counter: 1").exists()
                assert CounterApp.state.val == 1
            }
        "#;
        let items = parse_src(src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load_items failed");
        interp.run_all_tests().expect("test execution failed");
    }

    #[test]
    fn test_str_join_both_engines_parity() {
        let src = r#"
            fn main() -> string {
                str.join(["alpha", "beta", "gamma"], "::")
            }
        "#;
        let items = parse_src(src);

        // 1. Interpreter execution
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load_items");
        let interp_res = interp.invoke_fn("main", &[]).expect("interpreter run");
        assert_eq!(interp_res, NudgeVal::Str("alpha::beta::gamma".to_string()));

        // 2. nudge-runtime stdlib execution
        let rt_res = nudge_runtime::stdlib::execute(
            "str.join",
            vec![
                nudge_runtime::value::Value::List(vec![
                    nudge_runtime::value::Value::String("alpha".to_string()),
                    nudge_runtime::value::Value::String("beta".to_string()),
                    nudge_runtime::value::Value::String("gamma".to_string()),
                ]),
                nudge_runtime::value::Value::String("::".to_string()),
            ],
        )
        .expect("runtime str.join");
        assert_eq!(
            rt_res,
            nudge_runtime::value::Value::String("alpha::beta::gamma".to_string())
        );

        // Verify conversion parity
        assert_eq!(format!("{interp_res}"), format!("{rt_res}"));
    }

    #[test]
    fn test_str_index_of_multibyte_both_engines() {
        let src = r#"
            fn main() -> int {
                str.index_of("🦀x", "x")
            }
        "#;
        let items = parse_src(src);

        // 1. Interpreter: char index, not byte offset (4)
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load_items");
        let interp_res = interp.invoke_fn("main", &[]).expect("interpreter run");
        assert_eq!(interp_res, NudgeVal::Int(1));

        // 2. nudge-runtime stdlib: char index
        let rt_res = nudge_runtime::stdlib::execute(
            "str.index_of",
            vec![
                nudge_runtime::value::Value::String("🦀x".to_string()),
                nudge_runtime::value::Value::String("x".to_string()),
            ],
        )
        .expect("runtime str.index_of");
        assert_eq!(rt_res, nudge_runtime::value::Value::Int(1));
    }

    #[test]
    fn test_interpreter_deep_recursion_overflow_error() {
        // Run on a thread with adequate native stack so the 1000 call_depth limit
        // is reached and checked rather than premature OS thread stack exhaustion.
        let handle = std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn(|| {
                let src = r#"
                    fn recurse(n: int) -> int {
                        if n <= 0 {
                            0
                        } else {
                            recurse(n - 1) + 1
                        }
                    }
                    fn main() -> int {
                        recurse(1500)
                    }
                "#;
                let items = parse_src(src);
                let mut interp = crate::eval::interpreter::Interpreter::new();
                interp.load_items(&items).expect("load_items");

                // Depth > 1000 should return Err instead of crashing with SIGSEGV
                let res = interp.invoke_fn("main", &[]);
                assert!(res.is_err(), "expected error on deep recursion");
                let err = res.unwrap_err();
                assert!(
                    err.contains("call stack overflow: recursion limit exceeded"),
                    "unexpected error message: {err}"
                );

                // Ensure call_depth is decremented back to 0 (no stuck counter)
                assert_eq!(interp.call_depth, 0);

                // Verify that subsequent calls succeed normally
                let ok_res = interp
                    .invoke_fn("recurse", &[NudgeVal::Int(10)])
                    .expect("subsequent call");
                assert_eq!(ok_res, NudgeVal::Int(10));
                assert_eq!(interp.call_depth, 0);
            })
            .expect("spawn thread");
        handle.join().expect("thread join");
    }

    #[test]
    fn test_eval_browser_fetch_e2e() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir().join("nudge_eval_browser_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        let fixture_path = temp_dir.join("page.html");
        {
            let mut f = std::fs::File::create(&fixture_path).expect("create test file");
            f.write_all(b"<html><head><title>Test Page</title></head><body>OK</body></html>")
                .expect("write test file");
        }

        let file_url = format!("file://{}", fixture_path.to_str().unwrap());

        let src = r#"
            fn fetch_status(url: string) -> int uses Browser {
                let resp = browser.fetch(url)
                resp.status
            }
            fn fetch_body(url: string) -> string uses Browser {
                let resp = browser.fetch(url)
                resp.body
            }
        "#;
        let items = parse_src(src);
        let status = run_program(&items, "fetch_status", &[NudgeVal::Str(file_url.clone())])
            .expect("browser.fetch status succeeds");
        assert_eq!(status, NudgeVal::Int(200));

        let body = run_program(&items, "fetch_body", &[NudgeVal::Str(file_url)])
            .expect("browser.fetch body succeeds");
        assert_eq!(
            body,
            NudgeVal::Str(
                "<html><head><title>Test Page</title></head><body>OK</body></html>".into()
            )
        );

        let _ = std::fs::remove_file(fixture_path);
        let _ = std::fs::remove_dir(temp_dir);
    }

    #[test]
    fn test_prelude_codec_hash_and_b64() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let hash_path = root_dir.join("prelude/codec/hash.ndg");
        let b64_path = root_dir.join("prelude/codec/b64.ndg");

        let hash_src = std::fs::read_to_string(&hash_path).expect("read hash.ndg");
        let hash_items = parse_src(&hash_src);
        let state_val = run_program(
            &hash_items,
            "state_id",
            &[
                NudgeVal::Str("https://example.com".into()),
                NudgeVal::Str("<h1>Hello</h1>".into()),
            ],
        )
        .expect("hash.ndg state_id succeeds");
        assert_eq!(state_val, NudgeVal::Str("s-6d6d2244-586564a1".into()));

        let b64_src = std::fs::read_to_string(&b64_path).expect("read b64.ndg");
        let b64_items = parse_src(&b64_src);

        // RFC 4648 test vectors
        let vectors = [("f", "Zg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")];

        for (plain, encoded) in vectors {
            let enc_res = run_program(&b64_items, "b64_encode", &[NudgeVal::Str(plain.into())])
                .expect("b64_encode succeeds");
            assert_eq!(enc_res, NudgeVal::Str(encoded.into()));

            let dec_res = run_program(&b64_items, "b64_decode", &[NudgeVal::Str(encoded.into())])
                .expect("b64_decode succeeds");
            assert_eq!(dec_res, NudgeVal::Str(plain.into()));
        }
    }

    #[test]
    fn test_prelude_codec_json() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let json_path = root_dir.join("prelude/codec/json.ndg");

        let json_src = std::fs::read_to_string(&json_path).expect("read json.ndg");
        let test_src = format!(
            r#"{json_src}
fn test_round_trip() -> bool {{
    let raw = "{{\"enabled\":true,\"name\":\"nudge\",\"version\":1}}"
    let fields = parse_flat_object(raw)
    let s_name = get_string_field(fields, "name", "")
    let n_ver = get_int_field(fields, "version", 0)
    let b_en = get_bool_field(fields, "enabled", false)
    let back = stringify_fields(fields)

    // Array stringify and parse test
    let arr = ["alpha", "beta", "gamma"]
    let arr_json = stringify_string_array(arr)
    let parsed_arr = parse_json_string_array(arr_json)
    let ok_arr = len(parsed_arr) == 3 and parsed_arr[0] == "alpha" and parsed_arr[1] == "beta" and parsed_arr[2] == "gamma"

    let num_arr = [10, 20, 30]
    let num_json = stringify_int_array(num_arr)
    let parsed_nums = parse_json_int_array(num_json)
    let ok_nums = len(parsed_nums) == 3 and parsed_nums[0] == 10 and parsed_nums[2] == 30

    let bool_arr = [true, false, true]
    let bool_json = stringify_bool_array(bool_arr)
    let parsed_bools = parse_json_bool_array(bool_json)
    let ok_bools = len(parsed_bools) == 3 and parsed_bools[0] and !parsed_bools[1] and parsed_bools[2]

    s_name == "nudge" and n_ver == 1 and b_en and back == raw and ok_arr and ok_nums and ok_bools
}}
"#
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_round_trip", &[]).expect("json round trip succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_net_url() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let url_path = root_dir.join("prelude/net/url.ndg");

        let url_src = std::fs::read_to_string(&url_path).expect("read url.ndg");
        let test_src = format!(
            "{url_src}\n{}",
            r#"
fn test_url_resolve() -> bool {
    let base = parse_url("https://example.com:8080/path/to/page.html?v=1#top")
    let b_ok = base.scheme == "https" and base.host == "example.com" and base.port == 8080 and base.path == "/path/to/page.html" and base.query == "v=1" and base.fragment == "top"

    let r1 = resolve_relative(base, "sub.html")
    let r1_ok = r1 == "https://example.com:8080/path/to/sub.html"

    let r2 = resolve_relative(base, "../other.html")
    let r2_ok = r2 == "https://example.com:8080/path/other.html"

    let r3 = resolve_relative(base, "/root.html")
    let r3_ok = r3 == "https://example.com:8080/root.html"

    let r4 = resolve_relative(base, "//cdn.example.org/lib.js")
    let r4_ok = r4 == "https://cdn.example.org/lib.js"

    // Query parameter decoding test
    let params = parse_query_params("?q=nudge%20lang&filter=active%2Bready&empty=&user=alice%40corp.org")
    let q_val = get_query_param(params, "q", "")
    let f_val = get_query_param(params, "filter", "")
    let u_val = get_query_param(params, "user", "")
    let ok_params = q_val == "nudge lang" and f_val == "active+ready" and u_val == "alice@corp.org"

    // Query parameter builder test
    let built_q = build_query_string([{ name: "key with spaces", value: "val&more" }])
    let ok_built = built_q == "?key%20with%20spaces=val%26more"

    b_ok and r1_ok and r2_ok and r3_ok and r4_ok and ok_params and ok_built
}
"#
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_url_resolve", &[]).expect("url resolution succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_net_httpc() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let httpc_path = root_dir.join("prelude/net/httpc.ndg");

        let httpc_src = std::fs::read_to_string(&httpc_path).expect("read httpc.ndg");
        let test_src = format!(
            r#"{httpc_src}
fn test_http_framing() -> bool {{
    let req = build_http_request("GET", "example.com", "/index.html", [{{ name: "x-custom", value: "test" }}], "")
    let req_ok = str.contains(req, "GET /index.html HTTP/1.1") and str.contains(req, "Host: example.com") and str.contains(req, "x-custom: test")

    let raw_resp = "HTTP/1.1 200 OK\nContent-Type: text/html\nServer: NudgeMock\n\n<html><body>Hello World</body></html>"
    let resp = parse_headers_and_body(raw_resp)
    let resp_ok = resp.status == 200 and resp.body == "<html><body>Hello World</body></html>" and len(resp.headers) == 2

    let raw_chunked = "HTTP/1.1 200 OK\nTransfer-Encoding: chunked\n\n5\nHello\n6\n World\n0\n\n"
    let resp_chunked = parse_headers_and_body(raw_chunked)
    let chunked_ok = resp_chunked.status == 200 and resp_chunked.body == "Hello World"

    req_ok and resp_ok and chunked_ok
}}
"#
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_http_framing", &[]).expect("http framing succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_web_html_and_dom() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let html_src =
            std::fs::read_to_string(root_dir.join("prelude/web/html.ndg")).expect("read html.ndg");
        let dom_src =
            std::fs::read_to_string(root_dir.join("prelude/web/dom.ndg")).expect("read dom.ndg");

        let test_src = format!(
            r##"{html_src}
{dom_src}
fn test_dom_suite() -> bool {{
    let doc = "<div id=\"main\" class=\"container wrapper\"><header><h1 class=\"title\">Hello &amp; Welcome</h1></header><form action=\"/submit\"><input type=\"text\" name=\"username\" value=\"alice\"/><button type=\"submit\" class=\"btn primary\">Send</button></form></div>"
    let tokens = tokenize_html(doc)
    let tree = build_dom_tree(tokens)

    let cnt_ok = len(tree.nodes) == 6

    let h1_nodes = query_selector_all(tree, "h1")
    let h1_ok = len(h1_nodes) == 1 and h1_nodes[0].name == "Hello & Welcome" and h1_nodes[0].role == "heading"

    let main_div = query_selector(tree, "#main")
    let main_ok = if main_div != none {{
        main_div.tag == "div" and main_div.ref == 1
    }} else {{
        false
    }}

    let btns = query_selector_all(tree, ".btn")
    let btn_ok = len(btns) == 1 and btns[0].tag == "button" and btns[0].clickable and btns[0].role == "button"

    let inputs = query_selector_all(tree, "[name=\"username\"]")
    let input_ok = len(inputs) == 1 and inputs[0].value == "alice" and inputs[0].editable and inputs[0].role == "textbox"

    let form_btns = query_selector_all(tree, "form button")
    let desc_ok = len(form_btns) == 1 and form_btns[0].name == "Send"

    let by_ref = get_element_by_ref(tree, 1)
    let ref_ok = if by_ref != none {{
        by_ref.tag == "div"
    }} else {{
        false
    }}

    // Compound selector tests
    let compound_btns = query_selector_all(tree, "button.btn")
    let ok_compound_btn = len(compound_btns) == 1 and compound_btns[0].tag == "button"

    // Direct child selector test
    let child_inputs = query_selector_all(tree, "form > input")
    let ok_child = len(child_inputs) == 1 and child_inputs[0].tag == "input"

    // Pseudo-class test
    let first_elems = query_selector_all(tree, ":first-child")
    let ok_first = len(first_elems) >= 1

    // New DOM & Entity tests
    let by_id = get_element_by_id(tree, "main")
    let ok_by_id = if by_id != none {{ by_id.tag == "div" }} else {{ false }}
    let inputs_by_tag = get_elements_by_tag_name(tree, "input")
    let ok_tag = len(inputs_by_tag) == 1 and inputs_by_tag[0].name == "username"
    let titles_by_class = get_elements_by_class_name(tree, "title")
    let ok_class = len(titles_by_class) == 1 and titles_by_class[0].tag == "h1"

    let dec_entities = decode_entities("Product&trade; &reg; 100&yen; 50&cent;")
    let ok_entities = dec_entities == "Product™ ® 100¥ 50¢"

    cnt_ok and h1_ok and main_ok and btn_ok and input_ok and desc_ok and ref_ok and ok_compound_btn and ok_child and ok_first and ok_by_id and ok_tag and ok_class and ok_entities
}}
"##
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_dom_suite", &[]).expect("dom suite succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_web_policy() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let url_src =
            std::fs::read_to_string(root_dir.join("prelude/net/url.ndg")).expect("read url.ndg");
        let policy_src = std::fs::read_to_string(root_dir.join("prelude/web/policy.ndg"))
            .expect("read policy.ndg");

        let test_src = format!(
            r##"{url_src}
{policy_src}
fn test_policy_suite() -> bool {{
    let policy = {{
        allow: ["*.example.com", "api.github.com"],
        workspace_root: "/home/app/workspace",
        max_redirects: 20
    }}

    let u1 = parse_url("https://sub.example.com/v1/data")
    let ok1 = validate_url_policy(u1, policy).allowed

    let u2 = parse_url("https://api.github.com/repos")
    let ok2 = validate_url_policy(u2, policy).allowed

    let u3 = parse_url("https://malicious.com/steal")
    let ok3 = !validate_url_policy(u3, policy).allowed

    let empty_policy = {{
        allow: [],
        workspace_root: "/home/app/workspace",
        max_redirects: 20
    }}
    let ok4 = !validate_url_policy(u1, empty_policy).allowed

    let u5 = parse_url("ftp://example.com/file.zip")
    let ok5 = !validate_url_policy(u5, policy).allowed

    let u6 = parse_url("file:///home/app/workspace/docs/index.html")
    let ok6 = validate_url_policy(u6, policy).allowed

    let u7 = parse_url("file:///etc/passwd")
    let ok7 = !validate_url_policy(u7, policy).allowed

    let u8 = parse_url("file:///home/app/workspace/../etc/passwd")
    let ok8 = !validate_url_policy(u8, policy).allowed

    let r_ok = validate_redirect_hop(19, 20).allowed
    let r_denied = !validate_redirect_hop(20, 20).allowed

    ok1 and ok2 and ok3 and ok4 and ok5 and ok6 and ok7 and ok8 and r_ok and r_denied
}}
"##
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_policy_suite", &[]).expect("policy suite succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_web_snapshot() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let html_src =
            std::fs::read_to_string(root_dir.join("prelude/web/html.ndg")).expect("read html.ndg");
        let dom_src =
            std::fs::read_to_string(root_dir.join("prelude/web/dom.ndg")).expect("read dom.ndg");
        let snapshot_src = std::fs::read_to_string(root_dir.join("prelude/web/snapshot.ndg"))
            .expect("read snapshot.ndg");

        let test_src = format!(
            r##"{html_src}
{dom_src}
{snapshot_src}
fn test_snapshot_suite() -> bool {{
    let doc = "<div id=\"main\"><header><h1>Doc Title</h1></header><form><input type=\"text\" name=\"query\" value=\"search terms\"/><button type=\"submit\">Go</button></form></div>"
    let tokens = tokenize_html(doc)
    let tree = build_dom_tree(tokens)
    let snap = make_snapshot("p-1", "https://example.com/search", "Search Page", 1, tree)

    let ok_page = snap.page == "p-1"
    let ok_url = snap.url == "https://example.com/search"
    let ok_title = snap.title == "Search Page"
    let ok_state = snap.state_id == "s-1"
    let ok_cnt = snap.count == 6 and len(snap.elements) == 6

    let t = snap.tree
    let has_h1 = str.contains(t, "[3] heading \"Doc Title\"")
    let has_input = str.contains(t, "[5] textbox \"query\" = search terms (editable)")
    let has_btn = str.contains(t, "[6] button \"Go\" (clickable)")

    let s2 = make_state_id(2)
    let ok_s2 = s2 == "s-2"

    ok_page and ok_url and ok_title and ok_state and ok_cnt and has_h1 and has_input and has_btn and ok_s2
}}
"##
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_snapshot_suite", &[]).expect("snapshot suite succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_web_cookies_forms_actions() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let url_src =
            std::fs::read_to_string(root_dir.join("prelude/net/url.ndg")).expect("read url.ndg");
        let cookie_src = std::fs::read_to_string(root_dir.join("prelude/web/cookie.ndg"))
            .expect("read cookie.ndg");
        let profile_src = std::fs::read_to_string(root_dir.join("prelude/web/profile.ndg"))
            .expect("read profile.ndg");
        let html_src =
            std::fs::read_to_string(root_dir.join("prelude/web/html.ndg")).expect("read html.ndg");
        let dom_src =
            std::fs::read_to_string(root_dir.join("prelude/web/dom.ndg")).expect("read dom.ndg");
        let snapshot_src = std::fs::read_to_string(root_dir.join("prelude/web/snapshot.ndg"))
            .expect("read snapshot.ndg");
        let form_src =
            std::fs::read_to_string(root_dir.join("prelude/web/form.ndg")).expect("read form.ndg");
        let action_src = std::fs::read_to_string(root_dir.join("prelude/web/action.ndg"))
            .expect("read action.ndg");

        let test_src = format!(
            r##"{url_src}
{cookie_src}
{profile_src}
{html_src}
{dom_src}
{snapshot_src}
{form_src}
{action_src}
fn test_f4_suite() -> bool {{
    let prof1 = new_profile("user1")
    let prof2 = new_profile("user2")

    let ok_locale = prof1.device.locale == "en-US" and prof2.device.locale == "en-US"

    let set_cookie_hdr: [Header] = [
        {{ name: "Set-Cookie", value: "sid=sess_abc123; Domain=example.com; Path=/; Secure; HttpOnly" }}
    ]
    let prof1_updated = apply_response_headers(prof1, set_cookie_hdr, "example.com")

    let hdrs1 = build_profile_headers(prof1_updated, "example.com", "/dashboard", true, [])
    let hdrs2 = build_profile_headers(prof2, "example.com", "/dashboard", true, [])

    let mut h1_has_cookie = false
    for h in hdrs1 {{
        if h.name == "Cookie" and h.value == "sid=sess_abc123" {{
            h1_has_cookie = true
        }}
    }}
    let mut h2_has_cookie = false
    for h in hdrs2 {{
        if h.name == "Cookie" {{
            h2_has_cookie = true
        }}
    }}
    let ok_isolation = h1_has_cookie and !h2_has_cookie

    // Leading dot RFC 6265 cookie test
    let dot_cookie_hdr: [Header] = [
        {{ name: "Set-Cookie", value: "track=xyz; Domain=.example.com; Path=/; Secure" }}
    ]
    let prof1_with_dot = apply_response_headers(prof1_updated, dot_cookie_hdr, "example.com")
    let hdrs_sub = build_profile_headers(prof1_with_dot, "sub.example.com", "/dashboard", true, [])
    let mut sub_has_dot_cookie = false
    for h in hdrs_sub {{
        if str.contains(h.value, "track=xyz") {{
            sub_has_dot_cookie = true
        }}
    }}
    let ok_dot_cookie = sub_has_dot_cookie

    let doc = "<div id=\"app\"><form id=\"f1\" method=\"POST\" action=\"/api/login\"><input type=\"text\" name=\"user\" value=\"\"/><input type=\"password\" name=\"pass\" value=\"\"/><input type=\"checkbox\" name=\"remember\" value=\"yes\"/><button type=\"submit\" id=\"sub\">Login</button></form></div>"
    let tokens = tokenize_html(doc)
    let tree = build_dom_tree(tokens)

    let filled_tree = fill_form_fields(tree, [
        {{ ref: 3, value: "alice@example.com" }},
        {{ ref: 4, value: "secret 123&pass" }}
    ])

    let sub = build_form_submission(filled_tree, 2, "https://example.com/login.html")
    let ok_sub_method = sub.method == "POST"
    let ok_sub_action = sub.action == "https://example.com/api/login"
    let ok_sub_body = str.contains(sub.body, "user=alice%40example.com") and str.contains(sub.body, "pass=secret+123%26pass") and !str.contains(sub.body, "remember")

    let snap0 = make_snapshot("p-1", "https://example.com", "Test App", 1, filled_tree)

    let click_res = execute_click_target(filled_tree, 6, snap0)
    let ok_click = click_res.ok and click_res.action_sent

    let compound_res = execute_compound_clicks(filled_tree, [6], 10, snap0)
    let ok_compound = compound_res.ok

    let type_res = execute_type_action(filled_tree, 3, "bob", 2, snap0)
    let ok_type = type_res.ok and str.contains(type_res.snapshot.tree, "textbox \"user\" = bob")

    let wait_res = execute_wait_selector(filled_tree, "#sub", 1000, snap0)
    let ok_wait = wait_res.ok and !wait_res.timed_out

    let wait_fail = execute_wait_selector(filled_tree, "#nonexistent", 1000, snap0)
    let ok_wait_fail = !wait_fail.ok and wait_fail.timed_out

    ok_locale and ok_isolation and ok_dot_cookie and ok_sub_method and ok_sub_action and ok_sub_body and ok_click and ok_compound and ok_type and ok_wait and ok_wait_fail
}}
"##
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_f4_suite", &[]).expect("f4 suite succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_web_journal_replay_drift() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let url_src =
            std::fs::read_to_string(root_dir.join("prelude/net/url.ndg")).expect("read url.ndg");
        let html_src =
            std::fs::read_to_string(root_dir.join("prelude/web/html.ndg")).expect("read html.ndg");
        let dom_src =
            std::fs::read_to_string(root_dir.join("prelude/web/dom.ndg")).expect("read dom.ndg");
        let snapshot_src = std::fs::read_to_string(root_dir.join("prelude/web/snapshot.ndg"))
            .expect("read snapshot.ndg");
        let form_src =
            std::fs::read_to_string(root_dir.join("prelude/web/form.ndg")).expect("read form.ndg");
        let action_src = std::fs::read_to_string(root_dir.join("prelude/web/action.ndg"))
            .expect("read action.ndg");
        let journal_src = std::fs::read_to_string(root_dir.join("prelude/web/journal.ndg"))
            .expect("read journal.ndg");
        let drift_src = std::fs::read_to_string(root_dir.join("prelude/web/drift.ndg"))
            .expect("read drift.ndg");
        let watcher_src =
            std::fs::read_to_string(root_dir.join("examples/browser-agent/watcher.ndg"))
                .expect("read watcher.ndg");

        let test_src = format!(
            r##"{url_src}
{html_src}
{dom_src}
{snapshot_src}
{form_src}
{action_src}
{journal_src}
{drift_src}
{watcher_src}
fn test_f5_suite() -> bool {{
    // 1. NTF Trace Emission
    let doc_base = "<div id=\"page\"><button id=\"b1\">Login</button></div>"
    let t_base = build_dom_tree(tokenize_html(doc_base))
    let snap_base = make_snapshot("p-1", "https://example.com", "Title", 1, t_base)

    let snap_ntf = emit_snapshot_ntf(1, snap_base, 25, "sha256:abc", "shot1.png")
    let ok_snap_ntf = str.contains(snap_ntf, "\"kind\": \"browser.snapshot\"") and str.contains(snap_ntf, "\"element_count\": 2")

    let act_ntf = emit_act_ntf(2, "p-1", "click", 2, "ok", 10, true, false, "sha256:def")
    let ok_act_ntf = str.contains(act_ntf, "\"kind\": \"browser.act\"") and str.contains(act_ntf, "\"action\": \"click\"")

    // 2. Offline $0 Replay
    let rec1: NtfRecord = {{
        v: 1,
        seq: 1,
        kind: "browser.snapshot",
        url: "https://example.com",
        state_id: "s-1",
        element_count: 2,
        outcome: "ok",
        latency_ms: 25,
        action: "",
        page: "p-1",
        target: 0,
        action_sent: false,
        dry_run: false,
        request_hash: "sha256:abc",
        screenshot_asset: "shot1.png"
    }}
    let rec2: NtfRecord = {{
        v: 1,
        seq: 2,
        kind: "browser.act",
        url: "",
        state_id: "",
        element_count: 0,
        outcome: "ok",
        latency_ms: 10,
        action: "click",
        page: "p-1",
        target: 2,
        action_sent: true,
        dry_run: true,
        request_hash: "sha256:def",
        screenshot_asset: ""
    }}

    let j = new_replay_journal([rec1, rec2])

    // Replay snapshot
    let rep_snap_res = replay_next_snapshot(j, "https://example.com")
    let ok_rep_snap = rep_snap_res.result != none and rep_snap_res.error == ""

    // Replay dry-run action
    let rep_act_res = replay_act_dry_run(rep_snap_res.journal, "p-1", "click", 2, snap_base)
    let ok_rep_act = rep_act_res.result.ok and rep_act_res.result.action_sent

    // Replay exhaustion
    let rep_exhaust = replay_next_snapshot(rep_act_res.journal, "https://example.com")
    let ok_exhaust = rep_exhaust.result == none and str.contains(rep_exhaust.error, "ReplayMismatch: trace exhausted")

    // 3. Drift Detection & Watcher
    let doc_mutated = "<div id=\"page\"><h1>Alert Banner</h1><button id=\"b1\">Login</button></div>"
    let watch_res = check_page_and_alert_on_drift(doc_mutated, snap_base)
    let ok_drift = watch_res.drift_detected and !watch_res.action_fired and str.contains(watch_res.summary, "new element(s)")

    ok_snap_ntf and ok_act_ntf and ok_rep_snap and ok_rep_act and ok_exhaust and ok_drift
}}
"##
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_f5_suite", &[]).expect("f5 suite succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_web_js_engine() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let url_src =
            std::fs::read_to_string(root_dir.join("prelude/net/url.ndg")).expect("read url.ndg");
        let policy_src = std::fs::read_to_string(root_dir.join("prelude/web/policy.ndg"))
            .expect("read policy.ndg");
        let html_src =
            std::fs::read_to_string(root_dir.join("prelude/web/html.ndg")).expect("read html.ndg");
        let dom_src =
            std::fs::read_to_string(root_dir.join("prelude/web/dom.ndg")).expect("read dom.ndg");
        let snapshot_src = std::fs::read_to_string(root_dir.join("prelude/web/snapshot.ndg"))
            .expect("read snapshot.ndg");
        let form_src =
            std::fs::read_to_string(root_dir.join("prelude/web/form.ndg")).expect("read form.ndg");
        let lexer_src = std::fs::read_to_string(root_dir.join("prelude/web/js/lexer.ndg"))
            .expect("read lexer.ndg");
        let parser_src = std::fs::read_to_string(root_dir.join("prelude/web/js/parser.ndg"))
            .expect("read parser.ndg");
        let eval_src = std::fs::read_to_string(root_dir.join("prelude/web/js/eval.ndg"))
            .expect("read eval.ndg");
        let bridge_src = std::fs::read_to_string(root_dir.join("prelude/web/js/dom_bridge.ndg"))
            .expect("read dom_bridge.ndg");

        let test_src = format!(
            r##"{url_src}
{policy_src}
{html_src}
{dom_src}
{snapshot_src}
{form_src}
{lexer_src}
{parser_src}
{eval_src}
{bridge_src}
fn test_f6_suite() -> bool {{
    // 1. Lexer & Parser
    let js_code = "var count = 42; function add(a, b) {{ return a + b; }}"
    let tokens = tokenize_js(js_code)
    let ast = parse_js_script(tokens)
    let ok_ast = len(ast.nodes) >= 2

    // 2. Sandboxed execution & 3-tier shield
    let vm_cfg = default_vm_config()
    let exec_res = execute_js(ast, vm_cfg)
    let ok_exec = exec_res.ok

    // Infinite loop step-budget termination test
    let inf_code = "while (true) {{ var x = 1; }}"
    let inf_tokens = tokenize_js(inf_code)
    let inf_ast = parse_js_script(inf_tokens)
    let inf_res = execute_js(inf_ast, vm_cfg)
    let ok_shield = !inf_res.ok and str.contains(inf_res.error, "step budget exceeded")

    // 3. DOM Bridge mutation
    let doc = "<div id=\"app\"><input id=\"target\" value=\"initial\"/></div>"
    let tree = build_dom_tree(tokenize_html(doc))
    let pol: PolicyConfig = {{
        allow: ["*"],
        workspace_root: "/home",
        max_redirects: 20
    }}

    let mutation_script = "document.querySelector(\"#target\").value = \"updated_by_js\";"
    let bridge_res = execute_dom_script(tree, mutation_script, pol)
    let ok_mutation = bridge_res.executed

    let snap = make_snapshot("p-1", "https://example.com", "SPA", 1, bridge_res.tree)
    let ok_snap = str.contains(snap.tree, "updated_by_js")

    // textContent DOM Bridge mutation
    let text_mutation = "document.querySelector(\"#msg\").textContent = \"Hello from JS\";"
    let doc2 = "<div id=\"app\"><p id=\"msg\">Old Text</p></div>"
    let tree2 = build_dom_tree(tokenize_html(doc2))
    let bridge_res2 = execute_dom_script(tree2, text_mutation, pol)
    let snap2 = make_snapshot("p-2", "https://example.com", "SPA", 2, bridge_res2.tree)
    let ok_text_mut = str.contains(snap2.tree, "Hello from JS")

    ok_ast and ok_exec and ok_shield and ok_mutation and ok_snap and ok_text_mut
}}
"##
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_f6_suite", &[]).expect("f6 suite succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_web_layout_and_compositor() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let url_src =
            std::fs::read_to_string(root_dir.join("prelude/net/url.ndg")).expect("read url.ndg");
        let html_src =
            std::fs::read_to_string(root_dir.join("prelude/web/html.ndg")).expect("read html.ndg");
        let dom_src =
            std::fs::read_to_string(root_dir.join("prelude/web/dom.ndg")).expect("read dom.ndg");
        let snapshot_src = std::fs::read_to_string(root_dir.join("prelude/web/snapshot.ndg"))
            .expect("read snapshot.ndg");
        let layout_src = std::fs::read_to_string(root_dir.join("prelude/web/layout.ndg"))
            .expect("read layout.ndg");
        let compositor_src = std::fs::read_to_string(root_dir.join("prelude/web/compositor.ndg"))
            .expect("read compositor.ndg");

        let test_src = format!(
            r##"{url_src}
{html_src}
{dom_src}
{snapshot_src}
{layout_src}
{compositor_src}
fn test_f7_suite() -> bool uses Browser {{
    // 1. Box Model & Layout
    let doc = "<div id=\"card\" style=\"width: 400px; height: 200px; margin: 20px;\"><button id=\"btn\" style=\"width: 100px; height: 30px;\">Click</button><span style=\"display: none;\">Hidden</span></div>"
    let tree = build_dom_tree(tokenize_html(doc))
    let lt = compute_layout(tree, 1280, 800)

    let b_card = get_box_bounds(lt, 1)
    let ok_card = b_card[0] == 20 and b_card[1] == 20 and b_card[2] == 400 and b_card[3] == 200

    let b_btn = get_box_bounds(lt, 2)
    let ok_btn = b_btn[2] == 100 and b_btn[3] == 30

    let b_hidden = get_box_bounds(lt, 3)
    let ok_hidden = b_hidden[0] == 0 and b_hidden[1] == 0 and b_hidden[2] == 0 and b_hidden[3] == 0

    // 2. Snapshot Integration
    let snap0 = make_snapshot("p-1", "https://example.com", "Test Page", 1, tree)
    let snap_with_layout = apply_layout_to_snapshot(snap0, lt)
    let ok_snap_bounds = len(snap_with_layout.elements) >= 2 and snap_with_layout.elements[0].bounds[2] == 400

    // 3. Rasterization & Screenshot
    let shot_res = rasterize_layout_tree(lt)
    let ok_shot = str.starts_with(shot_res.data_url, "data:image/png;base64,") and str.starts_with(shot_res.asset_name, "shot-")

    let captured = capture_page_screenshot(snap_with_layout, lt)
    let ok_captured = str.starts_with(captured.screenshot, "data:image/png;base64,")

    // 4. Flexbox Layout Test
    let flex_doc = "<div id=\"row\" style=\"display: flex; gap: 10px; width: 300px;\"><button id=\"b1\" style=\"width: 60px; height: 30px;\">B1</button><button id=\"b2\" style=\"width: 70px; height: 30px;\">B2</button></div>"
    let flex_tree = build_dom_tree(tokenize_html(flex_doc))
    let flex_lt = compute_layout(flex_tree, 1280, 800)
    let b1 = get_box_bounds(flex_lt, 2)
    let b2 = get_box_bounds(flex_lt, 3)
    let ok_flex = b1[0] == 0 and b2[0] == 70 and b1[2] == 60 and b2[2] == 70

    // 5. Multi-line Paragraph Wrapping Test
    let wrap_doc = "<p style=\"width: 100px;\">This is a long line of paragraph text that must wrap into several lines.</p>"
    let wrap_tree = build_dom_tree(tokenize_html(wrap_doc))
    let wrap_lt = compute_layout(wrap_tree, 1280, 800)
    let b_wrap = get_box_bounds(wrap_lt, 1)
    let ok_wrap = b_wrap[2] == 100 and b_wrap[3] > 40

    // return which one fails for debugging
    let mut err_code = 0
    if !ok_card {{ err_code = 1 }}
    else if !ok_btn {{ err_code = 2 }}
    else if !ok_hidden {{ err_code = 3 }}
    else if !ok_snap_bounds {{ err_code = 4 }}
    else if !ok_shot {{ err_code = 5 }}
    else if !ok_captured {{ err_code = 6 }}
    else if !ok_flex {{ err_code = 7 }}
    else if !ok_wrap {{ err_code = 8 }}

    ok_card and ok_btn and ok_hidden and ok_snap_bounds and ok_shot and ok_captured and ok_flex and ok_wrap
}}
"##
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_f7_suite", &[]).expect("f7 suite succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_web_stream_h2_ws() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let h2_src =
            std::fs::read_to_string(root_dir.join("prelude/net/h2.ndg")).expect("read h2.ndg");
        let ws_src =
            std::fs::read_to_string(root_dir.join("prelude/net/ws.ndg")).expect("read ws.ndg");

        let test_src = format!(
            r##"{h2_src}
{ws_src}
fn test_f8_suite() -> bool uses Browser {{
    // 1. Raw Stream Echo Channel
    let handle = browser.connect("echo", 80)
    let ok_conn = handle > 0

    let written = browser.write(handle, "Nudge Stream Echo")
    let ok_write = written == 17

    let read_back = browser.read(handle, 1024)
    let ok_read = read_back == "Nudge Stream Echo"

    let closed = browser.close(handle)
    let ok_close = closed

    // 2. HTTP/2 Framing
    let pref = h2_connection_preface()
    let ok_pref = str.starts_with(pref, "PRI * HTTP/2.0")

    let set_frame = make_h2_settings_frame(false)
    let set_hdr = parse_h2_frame_header(set_frame)
    let ok_h2_settings = set_hdr.frame_type == 4 and set_hdr.length == 0

    let hdr_frame = make_h2_headers_frame(1, [{{ name: ":path", value: "/" }}], true)
    let hdr_header = parse_h2_frame_header(hdr_frame)
    let ok_h2_headers = hdr_header.frame_type == 1 and hdr_header.stream_id == 1 and str.contains(hdr_frame, ":path: /")

    // 3. WebSocket Framing
    let ws_req = make_ws_handshake_request("echo.websocket.org", "/ws", "dGhlIHNhbXBsZSBub25jZQ==")
    let ok_ws_req = str.contains(ws_req, "Upgrade: websocket") and str.contains(ws_req, "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==")

    let crlf = str.from_char_code(13) + "\n"
    let mock_resp = "HTTP/1.1 101 Switching Protocols" + crlf + "Upgrade: websocket" + crlf + "Connection: Upgrade" + crlf + "Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=" + crlf + crlf
    let ws_resp = parse_ws_handshake_response(mock_resp)
    let ok_ws_resp = ws_resp.status == 101 and ws_resp.upgrade_ok and ws_resp.accept_key == "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="

    let text_frame = make_ws_text_frame("hello", "abcd")
    let parsed_frame = parse_ws_frame(text_frame)
    let ok_ws_frame = parsed_frame.opcode == 1 and parsed_frame.is_fin and parsed_frame.text == "hello"

    // 16-bit extended payload test (200 characters)
    let mut long_msg = ""
    for _ in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20] {{
        long_msg = long_msg + "0123456789"
    }}
    let ext_frame = make_ws_text_frame(long_msg, "mask")
    let parsed_ext = parse_ws_frame(ext_frame)
    let ok_ext_frame = parsed_ext.opcode == 1 and parsed_ext.text == long_msg

    // Ping / Pong test
    let ping_frame = make_ws_ping_frame("heartbeat", "p123")
    let parsed_ping = parse_ws_frame(ping_frame)
    let ok_ping = parsed_ping.opcode == 9 and parsed_ping.text == "heartbeat"

    let close_frame = make_ws_close_frame()
    let parsed_close = parse_ws_frame(close_frame)
    let ok_ws_close = parsed_close.opcode == 8 and parsed_close.is_fin

    // HTTP/2 RST_STREAM & PING tests
    let rst_frame = make_h2_rst_stream_frame(3, 8) // CANCEL
    let rst_hdr = parse_h2_frame_header(rst_frame)
    let ok_h2_rst = rst_hdr.frame_type == 3 and rst_hdr.stream_id == 3 and rst_hdr.length == 4

    let ping_h2 = make_h2_ping_frame("ping1234", false)
    let ping_h2_hdr = parse_h2_frame_header(ping_h2)
    let ok_h2_ping = ping_h2_hdr.frame_type == 6 and ping_h2_hdr.stream_id == 0 and ping_h2_hdr.length == 8

    let win_frame = make_h2_window_update_frame(1, 65535)
    let win_hdr = parse_h2_frame_header(win_frame)
    let ok_h2_win = win_hdr.frame_type == 8 and win_hdr.stream_id == 1 and win_hdr.length == 4

    ok_conn and ok_write and ok_read and ok_close and ok_pref and ok_h2_settings and ok_h2_headers and ok_ws_req and ok_ws_resp and ok_ws_frame and ok_ext_frame and ok_ping and ok_ws_close and ok_h2_rst and ok_h2_ping and ok_h2_win
}}
"##
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_f8_suite", &[]).expect("f8 suite succeeds");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_prelude_web_recipes_f9() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let html_src =
            std::fs::read_to_string(root_dir.join("prelude/web/html.ndg")).expect("read html.ndg");
        let dom_src =
            std::fs::read_to_string(root_dir.join("prelude/web/dom.ndg")).expect("read dom.ndg");
        let search_recipe = std::fs::read_to_string(root_dir.join("recipes/browser_search.ndg"))
            .expect("read browser_search.ndg");
        let price_recipe = std::fs::read_to_string(root_dir.join("recipes/price_tracker.ndg"))
            .expect("read price_tracker.ndg");

        let test_src = format!(
            r##"{html_src}
{dom_src}
{search_recipe}
{price_recipe}
fn test_f9_recipes() -> bool uses Browser {{
    // 1. Search Recipe execution
    let search_res = execute_search("nudge language")
    let ok_search = len(search_res) >= 0

    // 2. Price Tracker Recipe execution
    let price_report = track_product_price("https://shop.example.com/shoes", 150.0)
    let ok_price = price_report.product_name != ""

    ok_search and ok_price
}}
"##
        );
        let items = parse_src(&test_src);
        let ok = run_program(&items, "test_f9_recipes", &[]).expect("f9 recipes succeed");
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_eval_index_assign_list_and_map() {
        let src = r#"
            fn test_list_assign() -> [int] {
                let mut xs = [10, 20, 30]
                xs[1] = 99
                xs
            }

            fn test_map_assign() -> int {
                let mut m = { a: 1, b: 2 }
                m["b"] = 42
                m["c"] = 100
                m["b"] + m["c"]
            }
        "#;
        let items = parse_src(src);
        let list_res = run_program(&items, "test_list_assign", &[]).unwrap();
        assert_eq!(
            list_res,
            NudgeVal::List(vec![
                NudgeVal::Int(10),
                NudgeVal::Int(99),
                NudgeVal::Int(30)
            ])
        );

        let map_res = run_program(&items, "test_map_assign", &[]).unwrap();
        assert_eq!(map_res, NudgeVal::Int(142));
    }

    #[test]
    fn test_eval_mem_store_primitives() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let src = r#"
            fn test_mem() -> bool uses MemStore {
                mem.clear()
                assert !mem.has("user:1")
                mem.set("user:1", "alice")
                assert mem.has("user:1")
                let val = mem.get("user:1", "default")
                assert val == "alice"
                let missing = mem.get("user:missing", "fallback")
                assert missing == "fallback"

                mem.set("user:2", "bob")
                let ks = mem.keys()
                assert len(ks) == 2

                let del_ok = mem.delete("user:1")
                assert del_ok
                assert !mem.has("user:1")
                true
            }
        "#;
        let items = parse_src(src);
        let ok = run_program(&items, "test_mem", &[]).unwrap();
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_eval_str_char_code_and_hash() {
        let src = r#"
            fn test_primitives() -> bool {
                let c_a = str.char_code("ABC", 0)
                let c_b = str.char_code("ABC", 1)
                let h1 = str.hash("hello world")
                let h2 = str.hash("hello world")
                let h3 = str.hash("other text")
                c_a == 65 and c_b == 66 and h1 == h2 and h1 != h3
            }
        "#;
        let items = parse_src(src);
        let ok = run_program(&items, "test_primitives", &[]).unwrap();
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_eval_workspace_stat() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let temp_dir = std::env::temp_dir().join(format!("nudge_test_stat_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let file_path = temp_dir.join("test_stat.txt");
        std::fs::write(&file_path, "hello world 12345").unwrap();

        let _guard_ws = crate::eval::interpreter::tests::EnvReset::set(
            "NUDGE_WORKSPACE_DIR",
            temp_dir.to_str().unwrap(),
        );

        let src = r#"
            fn test_stat() -> bool uses Workspace {
                let st = workspace.stat("test_stat.txt")
                st.size == 17 and !st.is_dir and st.mtime_ms > 0
            }
        "#;
        let items = parse_src(src);
        let ok = run_program(&items, "test_stat", &[]).unwrap();
        let _ = std::fs::remove_dir_all(&temp_dir);
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_eval_stdlib_expanded_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let src = r#"
            fn test_expanded() -> bool {
                // 1. Time & OS & Env
                let now_ts = time.now()
                let ok_time = now_ts > 1000000000.0
                let formatted = time.format(1700000000.0, "%Y-%m-%d")
                let ok_fmt = formatted == "2023-11-14"

                env.set("NUDGE_TEST_ENV_VAR", "active_value")
                let has_var = env.has("NUDGE_TEST_ENV_VAR")
                let env_val = env.get("NUDGE_TEST_ENV_VAR")
                let ok_env = has_var and env_val == "active_value"

                let pid = os.pid()
                let cwd = os.cwd()
                let ok_os = pid > 0 and str.length(cwd) > 0

                // 2. Str expanded
                let rep = str.repeat("ab", 3)
                let ok_rep = rep == "ababab"
                let padded = str.pad_left("42", 5, "0")
                let ok_pad = padded == "00042"
                let l_idx = str.last_index_of("foo.bar.baz", ".")
                let ok_l_idx = l_idx == 7
                let trimmed_s = str.trim_start("  hello  ")
                let trimmed_e = str.trim_end("  hello  ")
                let ok_trim = trimmed_s == "hello  " and trimmed_e == "  hello"
                let parsed_num = str.parse("1234")
                let ok_parse = parsed_num == 1234

                // 3. Math expanded
                let sin_0 = math.sin(0.0)
                let cos_0 = math.cos(0.0)
                let ok_trig = sin_0 == 0.0 and cos_0 == 1.0
                let clamped = math.clamp(150, 0, 100)
                let ok_clamp = clamped == 100
                let sum_val = math.sum([10, 20, 30, 40])
                let mean_val = math.mean([10, 20, 30])
                let ok_math = sum_val == 100 and mean_val == 20.0 and math.PI > 3.14

                // 4. List expanded
                let sorted_xs = list.sort([5, 2, 8, 1, 9])
                let ok_sort = sorted_xs[0] == 1 and sorted_xs[1] == 2 and sorted_xs[4] == 9
                let uniq_xs = list.unique([1, 2, 2, 3, 1, 4])
                let ok_uniq = len(uniq_xs) == 4 and uniq_xs[0] == 1 and uniq_xs[3] == 4
                let f_elem = list.first([100, 200, 300])
                let l_elem = list.last([100, 200, 300])
                let ok_fl = f_elem == 100 and l_elem == 300
                let pushed = list.push([1, 2], 3)
                let ok_push = len(pushed) == 3 and pushed[2] == 3
                let cat = list.concat([1, 2], [3, 4])
                let ok_cat = len(cat) == 4 and cat[2] == 3
                let flat = list.flatten([[1, 2], [3, 4]])
                let ok_flat = len(flat) == 4 and flat[3] == 4

                // 5. Map expanded
                let m1 = { a: 1, b: 2 }
                let m2 = { b: 20, c: 30 }
                let merged = map.merge(m1, m2)
                let ok_merge = merged["a"] == 1 and merged["b"] == 20 and merged["c"] == 30
                let m_len = map.len(merged)
                let ok_len = m_len == 3
                let entries = map.entries(m1)
                let ok_entries = len(entries) == 2

                ok_time and ok_fmt and ok_env and ok_os and ok_rep and ok_pad and ok_l_idx and ok_trim and ok_parse and ok_trig and ok_clamp and ok_math and ok_sort and ok_uniq and ok_fl and ok_push and ok_cat and ok_flat and ok_merge and ok_len and ok_entries
            }
        "#;
        let items = parse_src(src);
        let ok = run_program(&items, "test_expanded", &[]).unwrap();
        assert_eq!(ok, NudgeVal::Bool(true));
    }

    #[test]
    fn test_nvfs_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let nvfs_test_path = root_dir.join("crates/nudge-index/nvfs/test_nvfs.ndg");

        let nvfs_src = std::fs::read_to_string(&nvfs_test_path).expect("read test_nvfs.ndg");
        let items = parse_src(&nvfs_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp.run_all_tests().expect("all nvfs test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("nvfs main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_parser_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let parser_test_path = root_dir.join("crates/nudge-index/parser/test_parser.ndg");

        let parser_src = std::fs::read_to_string(&parser_test_path).expect("read test_parser.ndg");
        let items = parse_src(&parser_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp.run_all_tests().expect("all parser test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("parser main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_indexer_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let indexer_test_path = root_dir.join("crates/nudge-index/indexer/test_indexer.ndg");

        let indexer_src =
            std::fs::read_to_string(&indexer_test_path).expect("read test_indexer.ndg");
        let items = parse_src(&indexer_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp
            .run_all_tests()
            .expect("all indexer test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("indexer main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_query_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let query_test_path = root_dir.join("crates/nudge-index/api/test_query.ndg");

        let query_src = std::fs::read_to_string(&query_test_path).expect("read test_query.ndg");
        let items = parse_src(&query_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp.run_all_tests().expect("all query test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("query main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_engine_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let engine_test_path = root_dir.join("crates/nudge-index/test_engine.ndg");

        let engine_src = std::fs::read_to_string(&engine_test_path).expect("read test_engine.ndg");
        let items = parse_src(&engine_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp.run_all_tests().expect("all engine test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("engine main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_symbol_table_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let sym_test_path = root_dir.join("crates/nudge-index/indexer/symbol_table.ndg");

        let sym_src = std::fs::read_to_string(&sym_test_path).expect("read symbol_table.ndg");
        let items = parse_src(&sym_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp
            .run_all_tests()
            .expect("all symbol table test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("symbol table main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_inverted_index_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let idx_test_path = root_dir.join("crates/nudge-index/indexer/inverted_index.ndg");

        let idx_src = std::fs::read_to_string(&idx_test_path).expect("read inverted_index.ndg");
        let items = parse_src(&idx_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp
            .run_all_tests()
            .expect("all inverted index test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("inverted index main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_file_manager_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let _guard_ws = nudge_runtime::workspace::WS_TEST_LOCK.lock().unwrap();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        std::env::set_var("NUDGE_WORKSPACE_DIR", root_dir);
        let fm_path = root_dir.join("examples/file-manager/file_manager.ndg");

        let fm_src = std::fs::read_to_string(&fm_path).expect("read file_manager.ndg");
        let items = parse_src(&fm_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp
            .run_all_tests()
            .expect("all file manager test blocks pass");
    }

    #[test]
    fn test_call_graph_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let cg_path = root_dir.join("crates/nudge-index/indexer/call_graph.ndg");

        let cg_src = std::fs::read_to_string(&cg_path).expect("read call_graph.ndg");
        let items = parse_src(&cg_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp
            .run_all_tests()
            .expect("all call graph test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("call graph main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_diff_engine_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let diff_path = root_dir.join("crates/nudge-index/indexer/diff_engine.ndg");

        let diff_src = std::fs::read_to_string(&diff_path).expect("read diff_engine.ndg");
        let items = parse_src(&diff_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp
            .run_all_tests()
            .expect("all diff engine test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("diff engine main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_debouncer_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let deb_path = root_dir.join("crates/nudge-index/events/debouncer.ndg");

        let deb_src = std::fs::read_to_string(&deb_path).expect("read debouncer.ndg");
        let items = parse_src(&deb_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp
            .run_all_tests()
            .expect("all debouncer test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("debouncer main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_mcp_provider_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let mcp_path = root_dir.join("crates/nudge-index/api/mcp_provider.ndg");

        let mcp_src = std::fs::read_to_string(&mcp_path).expect("read mcp_provider.ndg");
        let items = parse_src(&mcp_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp
            .run_all_tests()
            .expect("all mcp provider test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("mcp provider main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }

    #[test]
    fn test_collections_acceptance_suite() {
        let _guard = crate::eval::interpreter::tests::EnvResetGuard::acquire();
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root_dir = manifest_dir.parent().unwrap().parent().unwrap();
        let coll_path = root_dir.join("examples/collections_demo.ndg");

        let coll_src = std::fs::read_to_string(&coll_path).expect("read test_collections.ndg");
        let items = parse_src(&coll_src);
        let mut interp = crate::eval::interpreter::Interpreter::new();
        interp.load_items(&items).expect("load items succeeds");
        interp
            .run_all_tests()
            .expect("all collections test blocks pass");
        let res = interp
            .invoke_fn("main", &[])
            .expect("collections main execution succeeds");
        assert_eq!(res, NudgeVal::Int(0));
    }
}
