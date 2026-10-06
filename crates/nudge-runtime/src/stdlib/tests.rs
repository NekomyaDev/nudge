#[cfg(test)]
#[allow(clippy::module_inception)] // conventional inner test module
mod tests {
    use crate::stdlib;
    use crate::vm::Value;

    #[test]
    fn test_io_read_write() {
        let temp_file = "/tmp/nudge_test_io.txt";

        // Write
        let result = stdlib::io::execute(
            "io.write",
            vec![
                Value::String(temp_file.to_string()),
                Value::String("Hello, World!".to_string()),
            ],
        );
        assert!(result.is_ok());

        // Read
        let result = stdlib::io::execute("io.read", vec![Value::String(temp_file.to_string())]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::String("Hello, World!".to_string()));

        // Cleanup
        stdlib::io::execute("io.delete", vec![Value::String(temp_file.to_string())]).unwrap();
    }

    #[test]
    fn test_io_delete_dir_and_list_dir_sorted() {
        let temp_dir = std::env::temp_dir().join(format!("nudge_io_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let f2 = temp_dir.join("b.txt");
        let f1 = temp_dir.join("a.txt");
        std::fs::write(&f2, "b").unwrap();
        std::fs::write(&f1, "a").unwrap();

        let res = stdlib::io::execute(
            "io.list_dir",
            vec![Value::String(temp_dir.to_str().unwrap().to_string())],
        )
        .unwrap();
        assert_eq!(
            res,
            Value::List(vec![
                Value::String("a.txt".to_string()),
                Value::String("b.txt".to_string()),
            ])
        );

        stdlib::io::execute(
            "io.delete",
            vec![Value::String(f1.to_str().unwrap().to_string())],
        )
        .unwrap();
        // io.delete on a non-empty directory succeeds recursively
        let del_dir = stdlib::io::execute(
            "io.delete",
            vec![Value::String(temp_dir.to_str().unwrap().to_string())],
        );
        assert!(del_dir.is_ok());
        assert!(!temp_dir.exists());
    }

    #[test]
    fn test_io_exists() {
        let result = stdlib::io::execute("io.exists", vec![Value::String("/tmp".to_string())]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Bool(true));

        let result =
            stdlib::io::execute("io.exists", vec![Value::String("/nonexistent".to_string())]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_math_abs() {
        let result = stdlib::math::execute("math.abs", vec![Value::Int(-5)]);
        assert_eq!(result.unwrap(), Value::Int(5));

        let result = stdlib::math::execute("math.abs", vec![Value::Float(-3.15)]);
        assert_eq!(result.unwrap(), Value::Float(3.15));
    }

    #[test]
    fn test_math_min_max() {
        let result = stdlib::math::execute("math.min", vec![Value::Int(5), Value::Int(10)]);
        assert_eq!(result.unwrap(), Value::Int(5));

        let result = stdlib::math::execute("math.max", vec![Value::Int(5), Value::Int(10)]);
        assert_eq!(result.unwrap(), Value::Int(10));
    }

    #[test]
    fn test_math_sqrt() {
        let result = stdlib::math::execute("math.sqrt", vec![Value::Int(144)]);
        assert_eq!(result.unwrap(), Value::Float(12.0));
    }

    #[test]
    fn test_math_pow() {
        let result = stdlib::math::execute("math.pow", vec![Value::Int(2), Value::Int(10)]);
        assert_eq!(result.unwrap(), Value::Int(1024));
    }

    #[test]
    fn test_math_trig() {
        let result = stdlib::math::execute("math.sin", vec![Value::Float(0.0)]);
        assert_eq!(result.unwrap(), Value::Float(0.0));

        let result = stdlib::math::execute("math.cos", vec![Value::Float(0.0)]);
        assert_eq!(result.unwrap(), Value::Float(1.0));
    }

    #[test]
    fn test_math_mean() {
        let result = stdlib::math::execute(
            "math.mean",
            vec![Value::List(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
                Value::Int(5),
            ])],
        );
        assert_eq!(result.unwrap(), Value::Float(3.0));
    }

    #[test]
    fn test_str_length() {
        let result =
            stdlib::string::execute("str.length", vec![Value::String("hello".to_string())]);
        assert_eq!(result.unwrap(), Value::Int(5));
    }

    #[test]
    fn test_str_index_of() {
        let result = stdlib::string::execute(
            "str.index_of",
            vec![
                Value::String("héllo".to_string()),
                Value::String("l".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::Int(2));

        let not_found = stdlib::string::execute(
            "str.index_of",
            vec![
                Value::String("hello".to_string()),
                Value::String("z".to_string()),
            ],
        );
        assert_eq!(not_found.unwrap(), Value::Int(-1));
    }

    #[test]
    fn test_str_upper_lower() {
        let result = stdlib::string::execute("str.upper", vec![Value::String("hello".to_string())]);
        assert_eq!(result.unwrap(), Value::String("HELLO".to_string()));

        let result = stdlib::string::execute("str.lower", vec![Value::String("HELLO".to_string())]);
        assert_eq!(result.unwrap(), Value::String("hello".to_string()));
    }

    #[test]
    fn test_str_trim() {
        let result =
            stdlib::string::execute("str.trim", vec![Value::String("  hello  ".to_string())]);
        assert_eq!(result.unwrap(), Value::String("hello".to_string()));
    }

    #[test]
    fn test_str_split_join() {
        let result = stdlib::string::execute(
            "str.split",
            vec![
                Value::String("a,b,c".to_string()),
                Value::String(",".to_string()),
            ],
        );
        assert_eq!(
            result.unwrap(),
            Value::List(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
                Value::String("c".to_string()),
            ])
        );

        let result = stdlib::string::execute(
            "str.join",
            vec![
                Value::List(vec![
                    Value::String("a".to_string()),
                    Value::String("b".to_string()),
                    Value::String("c".to_string()),
                ]),
                Value::String("-".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::String("a-b-c".to_string()));
    }

    #[test]
    fn test_str_contains() {
        let result = stdlib::string::execute(
            "str.contains",
            vec![
                Value::String("hello world".to_string()),
                Value::String("world".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::Bool(true));

        let result = stdlib::string::execute(
            "str.contains",
            vec![
                Value::String("hello world".to_string()),
                Value::String("xyz".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_str_replace() {
        let result = stdlib::string::execute(
            "str.replace",
            vec![
                Value::String("hello world".to_string()),
                Value::String("world".to_string()),
                Value::String("rust".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::String("hello rust".to_string()));
    }

    #[test]
    fn test_str_to_int() {
        let result = stdlib::string::execute("str.to_int", vec![Value::String("42".to_string())]);
        assert_eq!(result.unwrap(), Value::Int(42));

        let result = stdlib::string::execute("str.to_int", vec![Value::String("abc".to_string())]);
        assert_eq!(result.unwrap(), Value::None);
    }

    #[test]
    fn test_str_format() {
        let result = stdlib::string::execute(
            "str.format",
            vec![
                Value::String("Hello, {0}! You are {1} years old.".to_string()),
                Value::String("Alice".to_string()),
                Value::Int(25),
            ],
        );
        assert_eq!(
            result.unwrap(),
            Value::String("Hello, Alice! You are 25 years old.".to_string())
        );

        // Single pass: an argument value containing {1} must not be re-scanned and substituted
        let result2 = stdlib::string::execute(
            "str.format",
            vec![
                Value::String("{0} {1}".to_string()),
                Value::String("{1}".to_string()),
                Value::String("world".to_string()),
            ],
        );
        assert_eq!(result2.unwrap(), Value::String("{1} world".to_string()));
    }

    #[test]
    fn test_stdlib_dispatch() {
        // Test that stdlib::execute routes correctly
        let result = stdlib::execute("math.abs", vec![Value::Int(-5)]);
        assert_eq!(result.unwrap(), Value::Int(5));

        let result = stdlib::execute("str.upper", vec![Value::String("hello".to_string())]);
        assert_eq!(result.unwrap(), Value::String("HELLO".to_string()));

        let result = stdlib::execute("io.exists", vec![Value::String("/tmp".to_string())]);
        assert_eq!(result.unwrap(), Value::Bool(true));
    }

    #[test]
    fn test_str_pad_negative_width() {
        let left = stdlib::string::execute(
            "str.pad_left",
            vec![
                Value::String("hello".to_string()),
                Value::Int(-5),
                Value::String(" ".to_string()),
            ],
        );
        assert_eq!(left.unwrap(), Value::String("hello".to_string()));

        let right = stdlib::string::execute(
            "str.pad_right",
            vec![
                Value::String("hello".to_string()),
                Value::Int(-5),
                Value::String(" ".to_string()),
            ],
        );
        assert_eq!(right.unwrap(), Value::String("hello".to_string()));
    }

    #[test]
    fn test_math_pow_edge_cases() {
        // Negative exponent produces float
        let result = stdlib::math::execute("math.pow", vec![Value::Int(2), Value::Int(-1)]);
        assert_eq!(result.unwrap(), Value::Float(0.5));

        // Overflow falls back to float without panicking
        let result = stdlib::math::execute("math.pow", vec![Value::Int(2), Value::Int(100)]);
        assert!(matches!(result.unwrap(), Value::Float(_)));

        // Large integer exponent exceeding i32 doesn't truncate
        let result = stdlib::math::execute(
            "math.pow",
            vec![Value::Float(2.0), Value::Int(5_000_000_000)],
        );
        assert_eq!(result.unwrap(), Value::Float(f64::INFINITY));

        let result_neg =
            stdlib::math::execute("math.pow", vec![Value::Int(2), Value::Int(-5_000_000_000)]);
        assert_eq!(result_neg.unwrap(), Value::Float(0.0));
    }
}

#[cfg(test)]
mod computer_tests {
    use crate::stdlib::computer;
    use crate::vm::Value;
    use std::sync::Mutex;

    // these tests share process-global state (env vars + the VM's
    // last-observation authority) — they must not run concurrently
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn test_computer_fake_observe_and_act() {
        let _g = lock();
        computer::reset_authority_for_tests();
        // canonical language surface: observe(app), then actions carry the
        // target ONLY — the runtime owns the app + state authority
        let obs = computer::execute(
            "computer.observe",
            vec![Value::String("FakeApp".to_string())],
        )
        .expect("fake observe works");
        match &obs {
            Value::Map(m) => {
                assert_eq!(m.get("state_id"), Some(&Value::String("s-1".into())));
                assert_eq!(m.get("app"), Some(&Value::String("FakeApp".into())));
                assert!(m.contains_key("elements"));
            }
            other => panic!("expected an Observation map, got {other:?}"),
        }
        let r = computer::execute("computer.click", vec![Value::Int(1)]).expect("fake act works");
        match &r {
            Value::Map(m) => {
                assert_eq!(m.get("ok"), Some(&Value::Bool(true)));
                assert!(m.contains_key("outcome"));
            }
            other => panic!("expected an ActionResult map, got {other:?}"),
        }
    }

    #[test]
    fn test_computer_fake_enforces_fail_closed_contract() {
        let _g = lock();
        computer::reset_authority_for_tests();
        // acting without a prior observe must fail — never silently succeed
        let err = computer::execute("computer.click", vec![Value::Int(1)])
            .expect_err("action without observe must fail");
        assert!(err.contains("observe"), "{err}");
        // stale/unknown targets fail against the fake desktop too
        computer::execute(
            "computer.observe",
            vec![Value::String("FakeApp".to_string())],
        )
        .expect("observe works");
        let err = computer::execute("computer.click", vec![Value::Int(42)])
            .expect_err("unknown element must fail");
        assert!(err.contains("not_actionable"), "{err}");
    }

    #[test]
    fn test_computer_kill_switch_denies() {
        let _g = lock();
        std::env::set_var("NUDGE_COMPUTER_KILL", "1");
        let err = computer::execute(
            "computer.observe",
            vec![Value::String("FakeApp".to_string())],
        )
        .expect_err("kill switch must deny");
        std::env::remove_var("NUDGE_COMPUTER_KILL");
        assert!(err.contains("ComputerDenied"), "{err}");
    }

    #[test]
    fn test_computer_bridge_transport() {
        let _g = lock();
        computer::reset_authority_for_tests();
        // real subprocess JSONL round trip through the reference bridge
        let manifest = env!("CARGO_MANIFEST_DIR");
        let bridge = format!("{manifest}/../../tools/cu_bridge.py");
        std::env::set_var(
            "NUDGE_COMPUTER_SERVERS",
            format!(r#"{{"cu": {{"command": "python3 {bridge}"}}}}"#),
        );
        std::env::set_var("NUDGE_COMPUTER_PROVIDER", "cu");
        let obs = computer::execute("computer.observe", vec![Value::String("Notes".to_string())])
            .expect("bridge observe works");
        match &obs {
            Value::Map(m) => {
                // the canonical surface returns the Observation itself
                assert!(m.contains_key("state_id"), "{m:?}");
                assert!(m.contains_key("elements"), "{m:?}");
            }
            other => panic!("expected an Observation map, got {other:?}"),
        }
        std::env::remove_var("NUDGE_COMPUTER_PROVIDER");
        std::env::remove_var("NUDGE_COMPUTER_SERVERS");
    }

    fn fake_elements() -> String {
        r#"[{"index":0,"role":"window","title":"FakeApp"},
            {"index":1,"role":"button","title":"OK","pressable":true},
            {"index":2,"role":"button","title":"Cancel","pressable":true},
            {"index":3,"role":"textfield","title":"Search","editable":true}]"#
            .replace('\n', "")
    }

    fn write_trace(name: &str, records: &[String]) -> String {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, records.join("\n") + "\n").expect("write trace");
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn test_computer_replay_observe_and_act_dry_run() {
        let _g = lock();
        computer::reset_authority_for_tests();
        let trace = write_trace(
            "nudge-cu-replay-ok.jsonl",
            &[
                format!(
                    r#"{{"kind":"computer.observe","app":"Notes","state_id":"s-9","elements":{}}}"#,
                    fake_elements()
                ),
                r#"{"kind":"computer.act","action":"click","app":"Notes","target":{"index":1},"ok":true,"outcome":"ok","latency_ms":3}"#.to_string(),
            ],
        );
        std::env::set_var("NUDGE_REPLAY", &trace);
        let obs = computer::execute("computer.observe", vec![Value::String("Notes".into())])
            .expect("replay observe returns the recorded observation");
        // the recorded ActionResult is returned DRY-RUN — no provider runs
        let r = computer::execute("computer.click", vec![Value::Int(1)]).expect("dry-run act");
        // exhaustion raises like llm replay — a third observe has no record
        let err = computer::execute("computer.observe", vec![Value::String("Notes".into())])
            .expect_err("exhaustion must raise");
        std::env::remove_var("NUDGE_REPLAY");
        match &obs {
            Value::Map(m) => {
                assert_eq!(m.get("state_id"), Some(&Value::String("s-9".into())));
                assert_eq!(m.get("app"), Some(&Value::String("Notes".into())));
            }
            other => panic!("expected an Observation map, got {other:?}"),
        }
        match &r {
            Value::Map(m) => {
                assert_eq!(m.get("ok"), Some(&Value::Bool(true)));
                assert_eq!(m.get("outcome"), Some(&Value::String("ok".into())));
            }
            other => panic!("expected an ActionResult map, got {other:?}"),
        }
        assert!(err.contains("ReplayMismatch"), "{err}");
        assert!(err.contains("more computer.observe calls"), "{err}");
    }

    #[test]
    fn test_computer_replay_signature_mismatch() {
        let _g = lock();
        computer::reset_authority_for_tests();
        // app mismatch: replaying a decision made on a DIFFERENT app
        let trace = write_trace(
            "nudge-cu-replay-app.jsonl",
            &[format!(
                r#"{{"kind":"computer.observe","app":"Notes","state_id":"s-9","elements":{}}}"#,
                fake_elements()
            )],
        );
        std::env::set_var("NUDGE_REPLAY", trace);
        let err = computer::execute("computer.observe", vec![Value::String("Other".into())])
            .expect_err("app mismatch must raise");
        assert!(err.contains("trace observed app 'Notes'"), "{err}");
        assert!(err.contains("program observed 'Other'"), "{err}");
        std::env::remove_var("NUDGE_REPLAY");

        // action mismatch: the recorded act belongs to a different call
        let trace = write_trace(
            "nudge-cu-replay-act.jsonl",
            &[
                format!(
                    r#"{{"kind":"computer.observe","app":"Notes","state_id":"s-9","elements":{}}}"#,
                    fake_elements()
                ),
                r#"{"kind":"computer.act","action":"click","app":"Notes","target":{"index":1},"ok":true,"outcome":"ok","latency_ms":1}"#.to_string(),
            ],
        );
        std::env::set_var("NUDGE_REPLAY", trace);
        computer::execute("computer.observe", vec![Value::String("Notes".into())])
            .expect("observe matches");
        let err = computer::execute("computer.key", vec![Value::String("Return".into())])
            .expect_err("action mismatch must raise");
        assert!(err.contains("trace action 'click'"), "{err}");
        std::env::remove_var("NUDGE_REPLAY");

        // target mismatch: same action name, different target
        let trace = write_trace(
            "nudge-cu-replay-target.jsonl",
            &[
                format!(
                    r#"{{"kind":"computer.observe","app":"Notes","state_id":"s-9","elements":{}}}"#,
                    fake_elements()
                ),
                r#"{"kind":"computer.act","action":"click","app":"Notes","target":{"index":1},"ok":true,"outcome":"ok","latency_ms":1}"#.to_string(),
            ],
        );
        std::env::set_var("NUDGE_REPLAY", trace);
        computer::execute("computer.observe", vec![Value::String("Notes".into())])
            .expect("observe matches");
        let err = computer::execute("computer.click", vec![Value::Int(2)])
            .expect_err("target mismatch must raise");
        std::env::remove_var("NUDGE_REPLAY");
        assert!(err.contains("ReplayMismatch"), "{err}");
        assert!(err.contains("target"), "{err}");
    }

    #[test]
    fn test_computer_replay_drift_live_reobserve() {
        let _g = lock();
        computer::reset_authority_for_tests();
        // drift mode: live re-observation (fake desktop) + mechanical diff
        // against the recording — the fake scene is unchanged, so no drift;
        // actions stay DRY-RUN (they consume the recorded ActionResult)
        let trace = write_trace(
            "nudge-cu-replay-drift.jsonl",
            &[
                format!(
                    r#"{{"kind":"computer.observe","app":"FakeApp","state_id":"s-9","elements":{}}}"#,
                    fake_elements()
                ),
                r#"{"kind":"computer.act","action":"click","app":"FakeApp","target":{"index":1},"ok":true,"outcome":"ok","latency_ms":2}"#.to_string(),
            ],
        );
        std::env::set_var("NUDGE_REPLAY", trace);
        std::env::set_var("NUDGE_COMPUTER_DRIFT", "1");
        let obs = computer::execute("computer.observe", vec![Value::String("FakeApp".into())])
            .expect("drift observe works");
        let act = computer::execute("computer.click", vec![Value::Int(1)]);
        // clean up BEFORE asserting — leaked env vars poison sibling tests
        std::env::remove_var("NUDGE_REPLAY");
        std::env::remove_var("NUDGE_COMPUTER_DRIFT");
        match &obs {
            Value::Map(m) => {
                // the LIVE state (fake desktop), not the recorded one
                assert_eq!(m.get("state_id"), Some(&Value::String("s-1".into())));
                let drift = m.get("drift").expect("drift map attached");
                match drift {
                    Value::Map(d) => {
                        assert_eq!(d.get("changed"), Some(&Value::Bool(false)));
                        assert_eq!(d.get("added"), Some(&Value::List(Vec::new())));
                    }
                    other => panic!("expected a drift map, got {other:?}"),
                }
            }
            other => panic!("expected an Observation map, got {other:?}"),
        }
        // dry-run: the recorded result, not a live dispatch
        match act.expect("dry-run act under drift") {
            Value::Map(m) => {
                assert_eq!(m.get("ok"), Some(&Value::Bool(true)));
                assert_eq!(m.get("outcome"), Some(&Value::String("ok".into())));
            }
            other => panic!("expected an ActionResult map, got {other:?}"),
        }
    }
}
