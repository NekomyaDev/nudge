#[cfg(test)]
#[allow(clippy::module_inception)] // conventional inner test module
mod tests {
    use crate::stdlib;
    use crate::value::Value;

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
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let uid = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temp_dir =
            std::env::temp_dir().join(format!("nudge_io_test_{}_{}", std::process::id(), uid));
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

        let result = stdlib::math::execute("math.abs", vec![Value::Int(i64::MIN)]);
        assert!(result.is_err());
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

        let hyp = stdlib::math::execute("math.hypot", vec![Value::Float(3.0), Value::Float(4.0)]).unwrap();
        assert_eq!(hyp, Value::Float(5.0));

        let deg = stdlib::math::execute("math.degrees", vec![Value::Float(std::f64::consts::PI)]).unwrap();
        assert_eq!(deg, Value::Float(180.0));

        let rad = stdlib::math::execute("math.radians", vec![Value::Float(180.0)]).unwrap();
        assert_eq!(rad, Value::Float(std::f64::consts::PI));

        let gcd_res = stdlib::math::execute("math.gcd", vec![Value::Int(48), Value::Int(18)]).unwrap();
        assert_eq!(gcd_res, Value::Int(6));

        let lcm_res = stdlib::math::execute("math.lcm", vec![Value::Int(4), Value::Int(6)]).unwrap();
        assert_eq!(lcm_res, Value::Int(12));
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

        let multibyte = stdlib::string::execute(
            "str.index_of",
            vec![
                Value::String("🦀x".to_string()),
                Value::String("x".to_string()),
            ],
        );
        assert_eq!(multibyte.unwrap(), Value::Int(1));

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
    fn test_str_trim_start_and_end() {
        let r1 = stdlib::string::execute(
            "str.trim_start",
            vec![Value::String("  hello  ".to_string())],
        );
        assert_eq!(r1.unwrap(), Value::String("hello  ".to_string()));
        let r2 =
            stdlib::string::execute("str.trim_end", vec![Value::String("  hello  ".to_string())]);
        assert_eq!(r2.unwrap(), Value::String("  hello".to_string()));
    }

    #[test]
    fn test_str_last_index_of() {
        let result = stdlib::string::execute(
            "str.last_index_of",
            vec![
                Value::String("/path/to/file.ext.txt".to_string()),
                Value::String(".".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::Int(17));

        let not_found = stdlib::string::execute(
            "str.last_index_of",
            vec![
                Value::String("hello world".to_string()),
                Value::String("xyz".to_string()),
            ],
        );
        assert_eq!(not_found.unwrap(), Value::Int(-1));
    }

    #[test]
    fn test_str_predicates_and_lines() {
        let d1 = stdlib::string::execute("str.is_digit", vec![Value::String("12345".to_string())]).unwrap();
        assert_eq!(d1, Value::Bool(true));
        let d2 = stdlib::string::execute("str.is_digit", vec![Value::String("123a".to_string())]).unwrap();
        assert_eq!(d2, Value::Bool(false));

        let a1 = stdlib::string::execute("str.is_alpha", vec![Value::String("Hello".to_string())]).unwrap();
        assert_eq!(a1, Value::Bool(true));
        let a2 = stdlib::string::execute("str.is_alpha", vec![Value::String("Hello1".to_string())]).unwrap();
        assert_eq!(a2, Value::Bool(false));

        let e1 = stdlib::string::execute("str.is_empty", vec![Value::String("".to_string())]).unwrap();
        assert_eq!(e1, Value::Bool(true));
        let e2 = stdlib::string::execute("str.is_empty", vec![Value::String("nonempty".to_string())]).unwrap();
        assert_eq!(e2, Value::Bool(false));

        let cap = stdlib::string::execute("str.capitalize", vec![Value::String("wORLD".to_string())]).unwrap();
        assert_eq!(cap, Value::String("World".to_string()));

        let lns = stdlib::string::execute("str.lines", vec![Value::String("a\nb\r\nc".to_string())]).unwrap();
        assert_eq!(lns, Value::List(vec![
            Value::String("a".to_string()),
            Value::String("b".to_string()),
            Value::String("c".to_string()),
        ]));
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
    fn test_str_repeat_huge() {
        let result = crate::stdlib::string::execute(
            "str.repeat",
            vec![Value::String("a".to_string()), Value::Int(i64::MAX)],
        );
        assert!(result.is_err());
        let result2 = crate::stdlib::string::execute(
            "str.repeat",
            vec![Value::String("abc".to_string()), Value::Int(30_000_000)],
        );
        assert!(result2.is_err());
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
    use crate::value::Value;

    // These tests share PROCESS-GLOBAL state: env vars, the VM's last-observation
    // authority, and the global trace sink/seq that the computer-replay tests emit
    // into — so they must not run concurrently, NOR concurrently with
    // trace::tests, which guard the SAME global trace state. One shared lock is
    // what provides that mutual exclusion: a private per-module lock would NOT
    // exclude the trace tests (that lock mismatch was the cause of a 1-in-N
    // parallel flake in `bridge_records_llm_call_with_request_hash`).
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        crate::trace::testing::TEST_STATE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner())
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
                // the canonical zcode `tree` — byte-identical to the
                // Python/TS `_render_tree` for the same scene
                assert_eq!(
                    m.get("tree"),
                    Some(&Value::String(
                        "[0] window \"FakeApp\"\n[1] button \"OK\" actions=['press'] (pressable)\n[2] button \"Cancel\" actions=['press'] (pressable)\n[3] textfield \"Search\" (focused,editable)"
                            .into()
                    ))
                );
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
    fn test_computer_fake_geometry_and_movement() {
        let _g = lock();
        computer::reset_authority_for_tests();

        // Test computer.screen_size
        let size = computer::execute("computer.screen_size", vec![]).expect("screen_size works");
        match &size {
            Value::Map(m) => {
                assert_eq!(m.get("width"), Some(&Value::Int(1920)));
                assert_eq!(m.get("height"), Some(&Value::Int(1080)));
            }
            other => panic!("expected screen size map, got {other:?}"),
        }

        // Test computer.window_info with app argument
        let win = computer::execute(
            "computer.window_info",
            vec![Value::String("FakeApp".into())],
        )
        .expect("window_info works");
        match &win {
            Value::Map(m) => {
                assert_eq!(m.get("app"), Some(&Value::String("FakeApp".into())));
                assert_eq!(
                    m.get("title"),
                    Some(&Value::String("FakeApp Window".into()))
                );
                assert_eq!(m.get("focused"), Some(&Value::Bool(true)));
            }
            other => panic!("expected window info map, got {other:?}"),
        }

        // Observe to establish state authority
        computer::execute("computer.observe", vec![Value::String("FakeApp".into())])
            .expect("observe works");

        // Test computer.type (acting with text or raster movement / drag / type)
        // Note: type method in stdlib/computer.rs is registered as "computer.type"
        let type_res =
            computer::execute("computer.type", vec![Value::String("Hello Nudge".into())]);
        assert!(type_res.is_ok());

        // Test computer.key
        let key_res = computer::execute("computer.key", vec![Value::String("Return".into())]);
        assert!(key_res.is_ok());

        // Test computer.drag (with map coordinates or target indices)
        let mut from_map = std::collections::HashMap::new();
        from_map.insert("x".into(), Value::Int(100));
        from_map.insert("y".into(), Value::Int(200));

        let mut to_map = std::collections::HashMap::new();
        to_map.insert("x".into(), Value::Int(300));
        to_map.insert("y".into(), Value::Int(400));

        let drag_res = computer::execute(
            "computer.drag",
            vec![Value::Map(from_map), Value::Map(to_map)],
        );
        assert!(drag_res.is_ok());
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
        // Clean any stale lease file in /tmp or temp dir so cu_bridge does not return controller_busy
        let _ = std::fs::remove_file("/tmp/cu_bridge.lock");
        let _ = std::fs::remove_file(std::env::temp_dir().join("cu_bridge.lock"));
        let _ = std::fs::remove_file("/tmp/nudge-cu-bridge.lock");
        let _ = std::fs::remove_file(std::env::temp_dir().join("nudge-cu-bridge.lock"));

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
        let _ = std::fs::remove_file("/tmp/nudge-cu-bridge.lock");
        let _ = std::fs::remove_file(std::env::temp_dir().join("nudge-cu-bridge.lock"));
    }

    fn fake_elements() -> String {
        r#"[{"index":0,"role":"window","title":"FakeApp"},
            {"index":1,"role":"button","title":"OK","pressable":true},
            {"index":2,"role":"button","title":"Cancel","pressable":true},
            {"index":3,"role":"textfield","title":"Search","editable":true}]"#
            .replace('\n', "")
    }

    /// The canonical zcode tree of the fake scene (byte-parity with the
    /// Python/TS `_render_tree` and the fake desktop's live `tree`).
    fn fake_tree() -> &'static str {
        r#""[0] window \"FakeApp\"\n[1] button \"OK\" actions=['press'] (pressable)\n[2] button \"Cancel\" actions=['press'] (pressable)\n[3] textfield \"Search\" (focused,editable)""#
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
                format!(
                    r#"{{"kind":"computer.observe","app":"Notes","state_id":"s-9","elements":{}}}"#,
                    fake_elements()
                ),
            ],
        );
        std::env::set_var("NUDGE_REPLAY", &trace);
        let obs = computer::execute("computer.observe", vec![Value::String("Notes".into())])
            .expect("replay observe returns the recorded observation");
        // the recorded ActionResult is returned DRY-RUN — no provider runs
        let r = computer::execute("computer.click", vec![Value::Int(1)]).expect("dry-run act");
        // exhaustion raises like llm replay — a third observe has no record
        let _err = computer::execute("computer.observe", vec![Value::String("Notes".into())])
            .expect("second observe succeeds");
        let err_exhaust =
            computer::execute("computer.observe", vec![Value::String("Notes".into())])
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
        assert!(err_exhaust.contains("ReplayMismatch"), "{err_exhaust}");
        assert!(
            err_exhaust.contains("more computer.observe calls"),
            "{err_exhaust}"
        );
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
                    r#"{{"kind":"computer.observe","app":"FakeApp","state_id":"s-9","tree":{},"elements":{}}}"#,
                    fake_tree(),
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

    #[test]
    fn test_computer_mouse_and_cursor_pos() {
        let _g = lock();
        computer::reset_authority_for_tests();

        // cursor_pos before observe / move
        let pos1 = computer::execute("computer.cursor_pos", vec![]).expect("cursor_pos works");
        match &pos1 {
            Value::Map(m) => {
                let x = m.get("x").unwrap();
                assert!(matches!(x, Value::Float(0.0) | Value::Int(0)));
                let y = m.get("y").unwrap();
                assert!(matches!(y, Value::Float(0.0) | Value::Int(0)));
            }
            other => panic!("expected a CursorPos map, got {other:?}"),
        }

        computer::execute("computer.observe", vec![Value::String("FakeApp".into())])
            .expect("observe works");

        // mouse_move
        let move_res = computer::execute(
            "computer.mouse_move",
            vec![Value::Int(150), Value::Int(250)],
        )
        .expect("mouse_move works");
        match &move_res {
            Value::Map(m) => {
                assert_eq!(m.get("ok"), Some(&Value::Bool(true)));
            }
            other => panic!("expected ActionResult, got {other:?}"),
        }

        // cursor_pos after move
        let pos2 = computer::execute("computer.cursor_pos", vec![]).expect("cursor_pos works");
        match &pos2 {
            Value::Map(m) => {
                let x = m.get("x").unwrap();
                assert!(matches!(x, Value::Float(150.0) | Value::Int(150)));
                let y = m.get("y").unwrap();
                assert!(matches!(y, Value::Float(250.0) | Value::Int(250)));
            }
            other => panic!("expected a CursorPos map, got {other:?}"),
        }

        // mouse_down and mouse_up
        let down_res = computer::execute("computer.mouse_down", vec![Value::Int(1)])
            .expect("mouse_down works");
        match &down_res {
            Value::Map(m) => assert_eq!(m.get("ok"), Some(&Value::Bool(true))),
            other => panic!("expected ActionResult, got {other:?}"),
        }

        let up_res =
            computer::execute("computer.mouse_up", vec![Value::Int(1)]).expect("mouse_up works");
        match &up_res {
            Value::Map(m) => assert_eq!(m.get("ok"), Some(&Value::Bool(true))),
            other => panic!("expected ActionResult, got {other:?}"),
        }

        // out of bounds check
        let err = computer::execute(
            "computer.mouse_move",
            vec![Value::Int(3000), Value::Int(500)],
        )
        .expect_err("out of bounds mouse_move must fail");
        assert!(err.contains("out_of_bounds"), "{err}");
    }

    #[test]
    fn test_time_now_and_format() {
        let now_res = crate::stdlib::time::execute("time.now", vec![]);
        assert!(now_res.is_ok());
        let _ts = match now_res.unwrap() {
            Value::Float(f) => {
                assert!(f > 0.0);
                f
            }
            _ => panic!("time.now should return a float timestamp"),
        };

        // Test formatting fixed epoch timestamp 0 (1970-01-01 00:00:00 UTC)
        let fmt_res = crate::stdlib::time::execute(
            "time.format",
            vec![
                Value::Float(0.0),
                Value::String("%Y-%m-%d %H:%M:%S".to_string()),
            ],
        );
        assert!(fmt_res.is_ok());
        assert_eq!(
            fmt_res.unwrap(),
            Value::String("1970-01-01 00:00:00".to_string())
        );

        // Test formatting dynamic timestamp with integer representation
        let fmt_int = crate::stdlib::time::execute(
            "time.format",
            vec![Value::Int(0), Value::String("%Y".to_string())],
        );
        assert!(fmt_int.is_ok());
        assert_eq!(fmt_int.unwrap(), Value::String("1970".to_string()));
    }

    #[test]
    fn test_time_sleep() {
        let start = std::time::Instant::now();
        let res = crate::stdlib::time::execute("time.sleep", vec![Value::Int(10)]);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), Value::None);
        let elapsed = start.elapsed();
        assert!(elapsed >= std::time::Duration::from_millis(5));

        assert!(crate::stdlib::time::execute("time.sleep", vec![Value::Float(f64::NAN)]).is_err());
        assert!(
            crate::stdlib::time::execute("time.sleep", vec![Value::Float(f64::INFINITY)]).is_err()
        );
        assert!(crate::stdlib::time::execute("time.sleep", vec![Value::Float(1.0 / 0.0)]).is_err());
        assert!(crate::stdlib::time::execute("time.sleep", vec![Value::Float(-1.0)]).is_err());
        assert!(crate::stdlib::time::execute("time.sleep", vec![Value::Float(f64::MAX)]).is_err());
    }

    #[test]
    fn test_os_process_functions() {
        let args_res = crate::stdlib::os::execute("os.args", vec![]);
        assert!(args_res.is_ok());
        match args_res.unwrap() {
            Value::List(v) => assert!(!v.is_empty()),
            other => panic!("expected List, got {:?}", other),
        }

        let cwd_res = crate::stdlib::os::execute("os.cwd", vec![]);
        assert!(cwd_res.is_ok());
        match cwd_res.unwrap() {
            Value::String(s) => assert!(!s.is_empty()),
            other => panic!("expected String, got {:?}", other),
        }

        let pid_res = crate::stdlib::os::execute("os.pid", vec![]);
        assert!(pid_res.is_ok());
        match pid_res.unwrap() {
            Value::Int(pid) => assert!(pid > 0),
            other => panic!("expected Int, got {:?}", other),
        }

        let plat = crate::stdlib::os::execute("os.platform", vec![]).unwrap();
        match plat {
            Value::String(s) => assert!(!s.is_empty()),
            other => panic!("expected String, got {:?}", other),
        }

        let arch = crate::stdlib::os::execute("os.arch", vec![]).unwrap();
        match arch {
            Value::String(s) => assert!(!s.is_empty()),
            other => panic!("expected String, got {:?}", other),
        }

        let fam = crate::stdlib::os::execute("os.family", vec![]).unwrap();
        match fam {
            Value::String(s) => assert!(!s.is_empty()),
            other => panic!("expected String, got {:?}", other),
        }
    }

    #[test]
    fn test_env_functions() {
        let key = "NUDGE_TEST_ENV_VAR_12345";
        let val = "test_value_987";

        // env.has initially false
        let has_res = crate::stdlib::env::execute("env.has", vec![Value::String(key.to_string())]);
        assert!(has_res.is_ok());
        assert_eq!(has_res.unwrap(), Value::Bool(false));

        // env.get initially Value::None
        let get_res = crate::stdlib::env::execute("env.get", vec![Value::String(key.to_string())]);
        assert!(get_res.is_ok());
        assert_eq!(get_res.unwrap(), Value::None);

        // env.set sets variable
        let set_res = crate::stdlib::env::execute(
            "env.set",
            vec![
                Value::String(key.to_string()),
                Value::String(val.to_string()),
            ],
        );
        assert!(set_res.is_ok());
        assert_eq!(set_res.unwrap(), Value::None);

        // env.has now true
        let has_res2 = crate::stdlib::env::execute("env.has", vec![Value::String(key.to_string())]);
        assert!(has_res2.is_ok());
        assert_eq!(has_res2.unwrap(), Value::Bool(true));

        // env.get now returns Value::String(val)
        let get_res2 = crate::stdlib::env::execute("env.get", vec![Value::String(key.to_string())]);
        assert!(get_res2.is_ok());
        assert_eq!(get_res2.unwrap(), Value::String(val.to_string()));

        // env.all / env.vars
        let vars_res = crate::stdlib::env::execute("env.all", vec![]).unwrap();
        match vars_res {
            Value::Map(m) => assert!(m.contains_key(key)),
            other => panic!("expected Map, got {:?}", other),
        }

        // env.delete
        let del_res = crate::stdlib::env::execute("env.delete", vec![Value::String(key.to_string())]).unwrap();
        assert_eq!(del_res, Value::None);
        let has_after = crate::stdlib::env::execute("env.has", vec![Value::String(key.to_string())]).unwrap();
        assert_eq!(has_after, Value::Bool(false));

        // Clean up
        std::env::remove_var(key);
    }

    #[test]
    fn test_math_random_range() {
        for _ in 0..100 {
            let res = crate::stdlib::math::execute("math.random", vec![]).unwrap();
            match res {
                Value::Float(f) => {
                    assert!((0.0..1.0).contains(&f), "random value {f} out of [0, 1)");
                }
                _ => panic!("math.random must return float"),
            }
        }
        let err = crate::stdlib::math::execute("math.random", vec![Value::Int(1)]);
        assert!(err.is_err());
    }

    #[test]
    fn test_math_random_int_bounds() {
        // Positive interval [10, 20)
        for _ in 0..100 {
            let res = crate::stdlib::math::execute(
                "math.random_int",
                vec![Value::Int(10), Value::Int(20)],
            )
            .unwrap();
            match res {
                Value::Int(n) => {
                    assert!((10..20).contains(&n), "random_int {n} out of [10, 20)");
                }
                _ => panic!("math.random_int must return int"),
            }
        }

        // Negative interval [-50, -10)
        for _ in 0..100 {
            let res = crate::stdlib::math::execute(
                "math.random_int",
                vec![Value::Int(-50), Value::Int(-10)],
            )
            .unwrap();
            match res {
                Value::Int(n) => {
                    assert!((-50..-10).contains(&n), "random_int {n} out of [-50, -10)");
                }
                _ => panic!("math.random_int must return int"),
            }
        }

        // Error cases: min >= max
        assert!(crate::stdlib::math::execute(
            "math.random_int",
            vec![Value::Int(10), Value::Int(10)],
        )
        .is_err());
        assert!(crate::stdlib::math::execute(
            "math.random_int",
            vec![Value::Int(20), Value::Int(10)],
        )
        .is_err());

        // Error cases: invalid types / wrong arity
        assert!(crate::stdlib::math::execute("math.random_int", vec![Value::Int(1)]).is_err());
        assert!(crate::stdlib::math::execute(
            "math.random_int",
            vec![Value::Float(1.0), Value::Int(2)],
        )
        .is_err());
    }

    #[test]
    fn test_time_format_epoch_values_and_specifiers() {
        // 0 -> 1970-01-01 00:00:00 UTC (Thursday)
        let f0 = crate::stdlib::time::execute(
            "time.format",
            vec![
                Value::Float(0.0),
                Value::String("%Y-%m-%d %H:%M:%S %a %A %b %B %p %I %y %Z %%".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(
            f0,
            Value::String(
                "1970-01-01 00:00:00 Thu Thursday Jan January AM 12 70 UTC %".to_string()
            )
        );

        // 1000000000 -> 2001-09-09 01:46:40 UTC (Sunday)
        let f1b = crate::stdlib::time::execute(
            "time.format",
            vec![
                Value::Int(1_000_000_000),
                Value::String("%Y-%m-%d %H:%M:%S (%a, %b) %s %y".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(
            f1b,
            Value::String("2001-09-09 01:46:40 (Sun, Sep) 1000000000 01".to_string())
        );

        // Leap day: 951825600 -> 2000-02-29 12:00:00 UTC (Tuesday)
        let fleap = crate::stdlib::time::execute(
            "time.format",
            vec![
                Value::Float(951_825_600.0),
                Value::String("%Y-%m-%d %H:%M:%S %a %b %p %I".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(
            fleap,
            Value::String("2000-02-29 12:00:00 Tue Feb PM 12".to_string())
        );

        // Fractional seconds %f
        let ffract = crate::stdlib::time::execute(
            "time.format",
            vec![
                Value::Float(1_000_000_000.5),
                Value::String("%S.%f".to_string()),
            ],
        )
        .unwrap();
        assert_eq!(ffract, Value::String("40.500000000".to_string()));
    }

    #[test]
    fn test_time_format_invalid_timestamps() {
        assert!(crate::stdlib::time::execute(
            "time.format",
            vec![
                Value::Float(f64::NAN),
                Value::String("%Y-%m-%d".to_string())
            ],
        )
        .is_err());

        assert!(crate::stdlib::time::execute(
            "time.format",
            vec![
                Value::Float(f64::INFINITY),
                Value::String("%Y-%m-%d".to_string())
            ],
        )
        .is_err());

        assert!(crate::stdlib::time::execute(
            "time.format",
            vec![Value::Float(1e25), Value::String("%Y-%m-%d".to_string())],
        )
        .is_err());

        assert!(crate::stdlib::time::execute(
            "time.format",
            vec![
                Value::String("not a number".to_string()),
                Value::String("%Y".to_string())
            ],
        )
        .is_err());
    }

    #[test]
    fn test_browser_fetch_file_fixture_and_kill_switch() {
        use std::io::Write;
        let temp_dir = std::env::temp_dir().join("nudge_browser_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        let fixture_path = temp_dir.join("sample.html");
        {
            let mut f = std::fs::File::create(&fixture_path).expect("create test file");
            f.write_all(b"<html><body><h1>Hello Nudge Browser</h1></body></html>")
                .expect("write test file");
        }

        let file_url = format!("file://{}", fixture_path.to_str().unwrap());

        // 1. Successful file fetch
        let res =
            crate::stdlib::browser::execute("browser.fetch", vec![Value::String(file_url.clone())])
                .expect("browser.fetch file fixture works");

        match res {
            Value::Map(m) => {
                assert_eq!(m.get("status"), Some(&Value::Int(200)));
                assert_eq!(m.get("url"), Some(&Value::String(file_url)));
                let body = m.get("body").expect("body present");
                assert_eq!(
                    body,
                    &Value::String(
                        "<html><body><h1>Hello Nudge Browser</h1></body></html>".to_string()
                    )
                );
            }
            other => panic!("expected Map, got {other:?}"),
        }

        // 2. Unsupported scheme fail-closed
        let bad_scheme = crate::stdlib::browser::execute(
            "browser.fetch",
            vec![Value::String("ftp://example.com/test.txt".to_string())],
        );
        assert!(bad_scheme.is_err());
        assert!(bad_scheme
            .unwrap_err()
            .contains("BrowserDenied: unsupported scheme"));

        // 3. Kill-switch check
        std::env::set_var("NUDGE_BROWSER_KILL", "1");
        let killed = crate::stdlib::browser::execute(
            "browser.fetch",
            vec![Value::String("http://example.com".to_string())],
        );
        std::env::remove_var("NUDGE_BROWSER_KILL");
        assert!(killed.is_err());
        assert!(killed
            .unwrap_err()
            .contains("BrowserDenied: NUDGE_BROWSER_KILL=1"));

        // Clean up
        let _ = std::fs::remove_file(fixture_path);
        let _ = std::fs::remove_dir(temp_dir);
    }
}
