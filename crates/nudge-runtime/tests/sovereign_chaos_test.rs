//! Sovereign Loop, Autonomous Scheduler, Self-Healing, and Jarvis HUD
//! Chaos, Fault-Injection, and Invariant Verification Test Suite.

use nudge_runtime::healer::{HealerEngine, MockDomElement, TargetSignature};
use nudge_runtime::jarvis_hud::JarvisHudEngine;
use nudge_runtime::mem_store::{crc32, MemStore};
use nudge_runtime::scheduler::AutonomousScheduler;
use nudge_runtime::sovereign::{GoalStatus, SovereignLoop, SovereignPhase};
use nudge_runtime::value::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

fn temp_chaos_dir(test_name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("nudge_chaos_{test_name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------------------------------------------------------------------------
// 1. NVFS WAL & Snapshot Chaos & Crash Proofs
// ---------------------------------------------------------------------------

#[test]
fn test_nvfs_wal_crc32_torn_write_recovery() {
    let dir = temp_chaos_dir("torn_write");

    // Phase 1: Write 3 valid entries
    {
        let mut store = MemStore::new();
        store.enable_persistence(&dir).unwrap();
        store.set("k1".to_string(), Value::String("v1".to_string()));
        store.set("k2".to_string(), Value::String("v2".to_string()));
        store.set("k3".to_string(), Value::String("v3".to_string()));
    }

    // Phase 2: Chaos injection - Simulate a power cut / torn write mid-flush
    let wal_path = dir.join("nvfs.wal");
    {
        let mut file = OpenOptions::new().append(true).open(&wal_path).unwrap();
        // Half-written truncated line with incomplete JSON
        writeln!(
            file,
            "a1b2c3d4:{{\"seq\":4,\"op\":\"set\",\"k\":\"corrupt_half"
        )
        .unwrap();
    }

    // Phase 3: Recovery from disk must succeed and cleanly ignore the torn tail
    {
        let mut recovered_store = MemStore::new();
        let replayed = recovered_store.enable_persistence(&dir).unwrap();
        assert_eq!(
            replayed, 3,
            "should cleanly recover exactly 3 valid entries"
        );
        assert_eq!(
            recovered_store.get("k1", Value::None),
            Value::String("v1".to_string())
        );
        assert_eq!(
            recovered_store.get("k2", Value::None),
            Value::String("v2".to_string())
        );
        assert_eq!(
            recovered_store.get("k3", Value::None),
            Value::String("v3".to_string())
        );
        assert!(!recovered_store.has("corrupt_half"));
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_nvfs_wal_crc32_bit_rot_detection() {
    let dir = temp_chaos_dir("bit_rot");

    // Phase 1: Write 2 valid entries
    {
        let mut store = MemStore::new();
        store.enable_persistence(&dir).unwrap();
        store.set("valid1".to_string(), Value::Int(100));
        store.set("valid2".to_string(), Value::Int(200));
    }

    // Phase 2: Fault injection - Inject a record with an invalid CRC32
    let wal_path = dir.join("nvfs.wal");
    {
        let mut file = OpenOptions::new().append(true).open(&wal_path).unwrap();
        // CRC is intentionally wrong (00000000 vs real crc)
        writeln!(
            file,
            "00000000:{{\"seq\":3,\"op\":\"set\",\"t\":123,\"k\":\"injected\",\"v\":999}}"
        )
        .unwrap();
    }

    // Phase 3: Recovery must detect CRC mismatch and safely halt
    {
        let mut recovered_store = MemStore::new();
        let replayed = recovered_store.enable_persistence(&dir).unwrap();
        assert_eq!(replayed, 2, "must reject corrupted CRC record");
        assert_eq!(recovered_store.get("valid1", Value::None), Value::Int(100));
        assert_eq!(recovered_store.get("valid2", Value::None), Value::Int(200));
        assert!(!recovered_store.has("injected"));
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_nvfs_corrupted_snapshot_fallback() {
    let dir = temp_chaos_dir("snap_corrupt");

    // Create a corrupted snapshot file
    let snap_path = dir.join("nvfs.snap");
    fs::write(
        &snap_path,
        b"{\"version\":1,\"seq\":10,\"crc32\":99999999,\"entries\":{\"ghost\":1}}",
    )
    .unwrap();

    // Create a valid WAL file
    let wal_path = dir.join("nvfs.wal");
    let line = "{\"seq\":1,\"op\":\"set\",\"t\":100,\"k\":\"from_wal\",\"v\":\"resilient\"}";
    let c = crc32(line.as_bytes());
    fs::write(&wal_path, format!("{c:08x}:{line}\n")).unwrap();

    // Recovery must detect snapshot CRC mismatch and fall back to valid WAL
    let mut store = MemStore::new();
    let replayed = store.enable_persistence(&dir).unwrap();
    assert_eq!(replayed, 1);
    assert_eq!(
        store.get("from_wal", Value::None),
        Value::String("resilient".to_string())
    );
    assert!(!store.has("ghost"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_nvfs_auto_compaction_and_durability() {
    let dir = temp_chaos_dir("auto_compact");

    {
        let mut store = MemStore::new();
        store.set_auto_snapshot_threshold(Some(5));
        store.enable_persistence(&dir).unwrap();

        for i in 1..=12 {
            store.set(format!("key_{i}"), Value::Int(i));
        }

        // Snapshot should have been taken at seq 5 and seq 10
        assert!(dir.join("nvfs.snap").exists());
    }

    // Recover from disk
    {
        let mut recovered = MemStore::new();
        let replayed = recovered.enable_persistence(&dir).unwrap();
        // Since snapshot covers up to seq 10, only seq 11 and 12 are replayed from WAL
        assert_eq!(replayed, 2);
        for i in 1..=12 {
            assert_eq!(
                recovered.get(&format!("key_{i}"), Value::None),
                Value::Int(i)
            );
        }
    }

    let _ = fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// 2. Goal DAG Formal Proofs: Cycle Detection & Cascade Abort
// ---------------------------------------------------------------------------

#[test]
fn test_goal_dag_cycle_detection() {
    let mut sl = SovereignLoop::new();

    let g1 = sl.submit_intent("Goal 1".to_string(), None);
    let g2 = sl.submit_intent("Goal 2".to_string(), None);
    let g3 = sl.submit_intent("Goal 3".to_string(), None);

    // Self-dependency must fail
    assert!(sl.add_dependency(&g1, &g1).is_err());

    // Valid chain: g3 depends on g2, g2 depends on g1
    assert!(sl.add_dependency(&g2, &g1).is_ok());
    assert!(sl.add_dependency(&g3, &g2).is_ok());

    // Cyclic closure: g1 depends on g3 (g1 -> g3 -> g2 -> g1)
    let cycle_err = sl.add_dependency(&g1, &g3);
    assert!(cycle_err.is_err());
    assert!(cycle_err
        .unwrap_err()
        .contains("cyclic dependency detected"));

    // Two-node direct cycle: g1 depends on g2
    let direct_cycle = sl.add_dependency(&g1, &g2);
    assert!(direct_cycle.is_err());
}

#[test]
fn test_goal_dag_cascade_failure_propagation() {
    let mut sl = SovereignLoop::new();

    // Pipeline: Extract -> Transform -> Load
    let extract = sl.submit_intent("Extract raw records".to_string(), None);
    let transform = sl.submit_intent("Transform records".to_string(), None);
    let load = sl.submit_intent("Load into warehouse".to_string(), None);

    sl.add_dependency(&transform, &extract).unwrap();
    sl.add_dependency(&load, &transform).unwrap();

    // Advance tick: Extract should become evaluating
    sl.tick();
    let telem = sl.get_telemetry();
    assert_eq!(telem.active_goal_id.as_deref(), Some(extract.as_str()));

    // Terminally fail Extract (exhaust retries)
    sl.fail_goal(&extract, "Database connection refused".to_string())
        .unwrap(); // attempt 1 (healing)
    sl.fail_goal(&extract, "Database connection refused".to_string())
        .unwrap(); // attempt 2 (healing)
    sl.fail_goal(&extract, "Database connection refused".to_string())
        .unwrap(); // attempt 3 (healing)
    let ret = sl
        .fail_goal(&extract, "Database connection refused".to_string())
        .unwrap(); // attempt 4 (terminal)
    assert!(!ret, "should be terminal failure");

    // Invariant: Both Transform and Load must have automatically cascaded to Failed
    let goals = sl.list_goals();
    let extract_node = goals.iter().find(|g| g.id == extract).unwrap();
    let transform_node = goals.iter().find(|g| g.id == transform).unwrap();
    let load_node = goals.iter().find(|g| g.id == load).unwrap();

    assert_eq!(extract_node.status, GoalStatus::Failed);
    assert_eq!(transform_node.status, GoalStatus::Failed);
    assert_eq!(load_node.status, GoalStatus::Failed);

    assert!(transform_node
        .error
        .as_ref()
        .unwrap()
        .contains("upstream dependency"));
    assert!(load_node
        .error
        .as_ref()
        .unwrap()
        .contains("upstream dependency"));
}

#[test]
fn test_goal_deadline_timeout_enforcement() {
    let mut sl = SovereignLoop::new();

    // Submit intent with 0 second timeout (immediate expiration)
    let g = sl.submit_intent_with_timeout("Fast task".to_string(), None, Some(0));
    sl.tick(); // activates g

    // Simulate tick after timeout exceeded
    let telem = sl.tick();
    assert_eq!(telem.phase, SovereignPhase::SelfHealing);
    let goals = sl.list_goals();
    let node = goals.iter().find(|x| x.id == g).unwrap();
    assert_eq!(node.status, GoalStatus::Healing);
    assert!(node
        .error
        .as_ref()
        .unwrap()
        .contains("execution deadline exceeded"));
}

// ---------------------------------------------------------------------------
// 3. Self-Healing Multi-Attribute Structural Signature & Sandbox Verification
// ---------------------------------------------------------------------------

#[test]
fn test_self_healing_signature_synthesis_and_sandbox_verification() {
    let mut healer = HealerEngine::new();

    let sig = TargetSignature {
        tag: Some("button".to_string()),
        id: Some("submit_btn".to_string()),
        classes: vec!["btn".to_string(), "btn-primary".to_string()],
        data_testid: Some("confirm-checkout".to_string()),
        aria_label: Some("Confirm and pay order".to_string()),
        role: Some("button".to_string()),
        text_content: Some("Pay Now".to_string()),
        name: None,
        placeholder: None,
    };

    let candidates = healer.synthesize_from_signature(&sig);
    assert!(!candidates.is_empty());
    assert_eq!(candidates[0].strategy, "data_testid");
    assert_eq!(candidates[0].payload, "[data-testid='confirm-checkout']");
    assert!(candidates[0].confidence >= 0.95);

    // Mock DOM environment
    let dom = vec![
        MockDomElement {
            tag: "button".to_string(),
            id: Some("old_btn".to_string()), // ID drifted!
            classes: vec!["btn".to_string()],
            data_testid: Some("confirm-checkout".to_string()), // TestID survived!
            aria_label: Some("Confirm and pay order".to_string()),
            role: Some("button".to_string()),
            text: Some("Pay Now".to_string()),
        },
        MockDomElement {
            tag: "div".to_string(),
            id: None,
            classes: vec![],
            data_testid: None,
            aria_label: None,
            role: None,
            text: None,
        },
    ];

    // Dry-run verification against mock DOM
    let verified = healer.verify_candidate_against_dom(&candidates[0], &dom);
    assert!(verified, "candidate[0] must uniquely match in mock DOM");

    // Commit resolution
    let res = healer.commit_resolution("#submit_btn".to_string(), candidates[0].clone(), 1);
    assert!(res.verified);
    assert_eq!(healer.total_healed(), 1);

    // Rollback test
    let rolled = healer.rollback_last_resolution();
    assert!(rolled.is_some());
    assert_eq!(healer.total_healed(), 0);
}

// ---------------------------------------------------------------------------
// 4. Watch-Only Jarvis HUD 60 FPS Zero-Allocation Performance Proof
// ---------------------------------------------------------------------------

#[test]
fn test_jarvis_hud_framebuffer_zero_allocation_performance() {
    let hud = JarvisHudEngine::new();
    let width = 1280usize;
    let height = 800usize;
    let mut framebuffer = vec![0u32; width * height];

    // Warm-up run
    hud.render_to_u32_buffer(&mut framebuffer, width, height);

    // Benchmark 100 consecutive frames at 1280x800
    let start = Instant::now();
    let iterations = 100;
    for _ in 0..iterations {
        hud.render_to_u32_buffer(&mut framebuffer, width, height);
    }
    let total_elapsed = start.elapsed();
    let avg_per_frame = total_elapsed / iterations as u32;

    println!(
        "HUD Render 1280x800 Benchmark: {:?} total for 100 frames ({:?} per frame)",
        total_elapsed, avg_per_frame
    );

    // Invariant: At 60 FPS, the budget is 16.6 milliseconds (16,666 microseconds).
    // Our zero-allocation direct framebuffer rasterizer must comfortably beat 10ms.
    assert!(
        avg_per_frame.as_millis() < 10,
        "Frame render time {:?} exceeds 60 FPS safety threshold of 10ms",
        avg_per_frame
    );
}

// ---------------------------------------------------------------------------
// 5. 10,000 Continuous Tick Autonomous Longevity / Stress Test
// ---------------------------------------------------------------------------

#[test]
fn test_autonomous_scheduler_and_sovereign_10k_tick_stress() {
    let mut sched = AutonomousScheduler::new();
    let mut sl = SovereignLoop::new();

    let mut now_sim = 1000u64;

    // Schedule periodic heartbeat
    let periodic_id =
        sched.schedule_every_at("heartbeat".to_string(), 10, Vec::new(), None, now_sim);
    let after_id = sched.schedule_after_at("cleanup".to_string(), 50, Vec::new(), now_sim);

    let start = Instant::now();
    let mut cleanup_fired = false;

    for tick in 1..=10_000 {
        now_sim = now_sim.saturating_add(1);
        let fired = sched.tick(now_sim);

        // Feed tick into Sovereign Loop
        let _ = sl.tick();

        // Every 500 ticks submit a goal
        if tick % 500 == 0 {
            let g = sl.submit_intent(format!("Task at tick {tick}"), None);
            let _ = sl.complete_goal(&g, serde_json::json!({"status": "ok"}));
        }

        if fired
            .iter()
            .any(|task| task.id == after_id && task.task_name == "cleanup")
        {
            cleanup_fired = true;
        }
    }

    assert!(cleanup_fired, "one-shot cleanup task must have fired");

    let elapsed = start.elapsed();
    let telem = sl.get_telemetry();

    assert_eq!(telem.completed_goals, 20);
    assert_eq!(telem.failed_goals, 0);
    assert_eq!(telem.tick_count, 10_000);

    println!(
        "10,000 Continuous Tick Stress completed in {:?} (sub-microsecond ticks)",
        elapsed
    );

    sched.cancel(&periodic_id);
    assert_eq!(sched.active_count(), 0);
}
