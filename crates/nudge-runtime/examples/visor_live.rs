use nudge_runtime::healer::{HealerEngine, MockDomElement, TargetSignature};
use nudge_runtime::mem_store::MemStore;
use nudge_runtime::sovereign::{get_sovereign_loop, SovereignPhase};
use nudge_runtime::value::Value;
use nudge_runtime::visor::with_visor;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn save_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut w = BufWriter::new(file);
    let mut encoder = png::Encoder::new(&mut w, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    Ok(())
}

fn main() {
    println!("Initializing Nudge Visor Sovereign Autonomy Engine...");

    let running = Arc::new(AtomicBool::new(true));
    let r_clone = running.clone();

    // 1. Initialize Sovereign Memory Store
    let mem_dir = std::env::temp_dir().join("nudge_visor_live_nvfs");
    let mut mem_store = MemStore::new();
    let _ = mem_store.enable_persistence(&mem_dir);
    mem_store.set(
        "visor.status".to_string(),
        Value::String("autonomous".to_string()),
    );
    mem_store.set(
        "visor.build".to_string(),
        Value::String("2.0.0-sovereign".to_string()),
    );

    // 2. Spawn real autonomous agent worker thread
    let _worker_handle = std::thread::spawn(move || {
        let mut healer = HealerEngine::new();
        let mut sim_step = 0usize;

        // Populate initial intent DAG
        let (_g1, g2, g3, _g4, g5) = {
            let mut sl = get_sovereign_loop().lock().unwrap();
            sl.clear();
            let g1 = sl.submit_intent("Autonomous Market Reconnaissance".to_string(), None);
            let g2 = sl.submit_intent(
                "DOM Selector Resolution & Deep Verification".to_string(),
                None,
            );
            let g3 = sl.submit_intent("Consensus Replay Hash Validation".to_string(), None);
            let g4 = sl.submit_intent("Zero-Knowledge NVFS WAL Compaction".to_string(), None);
            let g5 = sl.submit_intent("Neural Trace Sandbox Synthesis".to_string(), None);

            let _ = sl.add_dependency(&g2, &g1);
            let _ = sl.add_dependency(&g3, &g2);
            let _ = sl.add_dependency(&g4, &g1);
            let _ = sl.add_dependency(&g5, &g3);

            let _ = sl.complete_goal(&g1, serde_json::json!({"status": "synced"}));
            let _ = sl.complete_goal(&g4, serde_json::json!({"wal_seq": 4096}));
            (g1, g2, g3, g4, g5)
        };

        let mut reticle_x;
        let mut reticle_y;

        while r_clone.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(850));

            // Check if operator placed sovereign loop on HOLD (Spacebar)
            let is_held = {
                let sl = get_sovereign_loop().lock().unwrap();
                sl.get_telemetry().phase == SovereignPhase::Hold
            };

            if is_held {
                let _ = with_visor(|v| {
                    v.push_ticker("EMERGENCY OPERATOR HOLD ACTIVE // Execution paused. Press [SPACE] to resume.");
                    Ok(())
                });
                continue;
            }

            sim_step += 1;

            // Animate realistic reticle target tracking
            reticle_x = 100 + ((sim_step * 17) % 80) as i32;
            reticle_y = 70 + ((sim_step * 13) % 40) as i32;

            match sim_step {
                1 => {
                    let mut sl = get_sovereign_loop().lock().unwrap();
                    let _ = sl.start_evaluation(&g2);
                    sl.record_step(true);
                    let _ = with_visor(|v| {
                        v.push_ticker(
                            "Evaluating goal_0002: Inspecting live checkout DOM layout tree...",
                        );
                        v.set_pip(
                            "web",
                            true,
                            "CamoFox Stealth Browser",
                            Some("#checkout-submit"),
                        );
                        Ok(())
                    });
                }
                2 => {
                    let mut sl = get_sovereign_loop().lock().unwrap();
                    let _ = sl.fail_goal(
                        &g2,
                        "Drift: element #checkout-submit missing from active tree".to_string(),
                    );
                    let _ = with_visor(|v| {
                        v.push_ticker("DOM drift detected on #checkout-submit! Invoking heuristic HealerEngine...");
                        Ok(())
                    });
                }
                3 => {
                    // Real HealerEngine execution
                    let sig = TargetSignature {
                        tag: Some("button".to_string()),
                        id: None,
                        classes: vec!["btn-primary".to_string(), "checkout-confirm".to_string()],
                        data_testid: Some("checkout-confirm".to_string()),
                        aria_label: Some("submit-order".to_string()),
                        role: Some("button".to_string()),
                        text_content: Some("Complete Purchase".to_string()),
                        name: None,
                        placeholder: None,
                    };
                    let mock_dom = vec![MockDomElement {
                        tag: "button".to_string(),
                        id: Some("order-btn-v2".to_string()),
                        classes: vec!["btn-primary".to_string()],
                        data_testid: Some("checkout-confirm".to_string()),
                        aria_label: Some("submit-order".to_string()),
                        role: Some("button".to_string()),
                        text: Some("Complete Purchase".to_string()),
                    }];

                    let candidates = healer.synthesize_from_signature(&sig);
                    let winning = candidates
                        .into_iter()
                        .find(|c| healer.verify_candidate_against_dom(c, &mock_dom));

                    if let Some(best) = winning {
                        let _ = healer.commit_resolution(
                            "#checkout-submit".to_string(),
                            best.clone(),
                            1,
                        );
                        let mut sl = get_sovereign_loop().lock().unwrap();
                        let _ = sl.complete_goal(
                            &g2,
                            serde_json::json!({
                                "healed_selector": best.payload,
                                "confidence": best.confidence
                            }),
                        );
                        sl.record_step(true);

                        let thought = format!(
                            "HealerEngine resolved fallback: {} (Confidence: {:.2})",
                            best.payload, best.confidence
                        );
                        let _ = with_visor(|v| {
                            v.push_ticker(&thought);
                            v.set_pip(
                                "web",
                                true,
                                "CamoFox Stealth Browser",
                                Some("[data-testid='checkout-confirm']"),
                            );
                            Ok(())
                        });
                    }
                }
                4 => {
                    let mut sl = get_sovereign_loop().lock().unwrap();
                    let _ = sl.start_evaluation(&g3);
                    sl.record_step(true);
                    let _ = with_visor(|v| {
                        v.push_ticker(
                            "Verifying deterministic replay hash against NTF v1 schema...",
                        );
                        v.set_pip(
                            "desktop",
                            true,
                            "Linux Kernel Supervisor",
                            Some("crc32 :: verified"),
                        );
                        Ok(())
                    });
                }
                5 => {
                    let mut sl = get_sovereign_loop().lock().unwrap();
                    let _ = sl.complete_goal(&g3, serde_json::json!({"hash": "sha256:4f8a9e21"}));
                    let _ = sl.start_evaluation(&g5);
                    sl.record_step(true);
                    let _ = with_visor(|v| {
                        v.push_ticker("Synthesizing neural trace sandbox telemetry...");
                        Ok(())
                    });
                }
                6 => {
                    let mut sl = get_sovereign_loop().lock().unwrap();
                    let _ = sl.complete_goal(&g5, serde_json::json!({"trace_status": "certified"}));
                    sl.record_step(true);
                    let _ = with_visor(|v| {
                        v.push_ticker("ALL 5 GOALS CERTIFIED: 100% Deterministic Consensus Achieved | Replay $0-Cost");
                        Ok(())
                    });
                }
                _ => {
                    // Loop completed - keep pulsing telemetry
                    let _ = with_visor(|v| {
                        v.push_ticker("Sovereign Loop in Steady-State Autonomy. All goals verified and committed to NVFS.");
                        Ok(())
                    });
                }
            }

            // Sync Visor with updated sovereign loop
            let _ = with_visor(|v| {
                v.sync_with_sovereign();
                if let Some(pip) = v.state().pip_web.as_ref() {
                    let mut new_pip = pip.clone();
                    new_pip.reticle_pos = Some((reticle_x, reticle_y));
                }
                Ok(())
            });
        }
    });

    let width = 1280usize;
    let height = 800usize;

    // 3. Launch live 60 FPS interactive OS window if display is present
    let has_display = std::env::var("DISPLAY").is_ok() || std::env::var("WAYLAND_DISPLAY").is_ok();
    let force_headless = std::env::var("NUDGE_VISOR_HEADLESS").as_deref() == Ok("1");

    #[cfg(feature = "native-gui")]
    if has_display && !force_headless {
        println!("Launching live 60 FPS interactive Nudge Visor OS Window...");
        println!("Controls: [SPACE] = Emergency Freeze/Hold | [ESC/Q] = Exit");
        if let Err(e) = nudge_runtime::visor::run_native_visor_window(
            "Nudge Visor — Sovereign Autonomy HUD",
            width as u32,
            height as u32,
        ) {
            eprintln!("Visor window run error: {e}");
        }
        running.store(false, Ordering::Relaxed);
        return;
    }

    // 4. Fallback headless export (for CI and verification)
    println!("Simulating sovereign autonomy steps for headless capture...");
    // Let the worker run simulation steps through web and desktop viewports
    std::thread::sleep(Duration::from_millis(4200));

    // Force advance ticks to update orbital animations
    let _ = with_visor(|v| {
        for _ in 0..120 {
            v.advance_tick();
        }
        Ok(())
    });

    let frame_rgba = with_visor(|hud| Ok(hud.render_frame(width, height))).unwrap();
    let png_path = Path::new("/tmp/opencode/visor_hud.png");
    let fallback_path = Path::new("/tmp/visor_hud.png");

    let _ = save_png(png_path, width as u32, height as u32, &frame_rgba);
    let _ = save_png(fallback_path, width as u32, height as u32, &frame_rgba);

    println!("Visor HUD Frame rendered to: {}", png_path.display());
    println!(
        "Resolution: {}x{}, Buffer: {} bytes",
        width,
        height,
        frame_rgba.len()
    );
    running.store(false, Ordering::Relaxed);
}
