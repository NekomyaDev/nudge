pub mod env;
pub mod interpreter;
pub mod val;

pub use interpreter::run_program;
#[allow(unused_imports)]
pub use interpreter::run_program_with_args;
pub use val::{parse_cli_arg, NudgeVal};

#[cfg(test)]
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

    /// The computer statics (last-observation authority, replay cursors)
    /// and NUDGE_REPLAY are process-global — serialize the bridge tests.
    static COMPUTER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn computer_guard() -> std::sync::MutexGuard<'static, ()> {
        COMPUTER_LOCK.lock().unwrap_or_else(|e| e.into_inner())
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
}
