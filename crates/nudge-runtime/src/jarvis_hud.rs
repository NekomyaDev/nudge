//! Watch-Only Jarvis HUD (Non-Intrusive Sci-Fi Observer Interface).
//!
//! Provides the ambient visual feedback layer for Nudge Sovereign Loops:
//! - Central Orbital Core with reactive color states (Azure, Amber, Violet, Emerald, Crimson).
//! - Concentric orbital rings with rotating goal nodes.
//! - Corner docking Picture-in-Picture (PiP) viewports for Web and OS observation.
//! - Ambient glass telemetry header (Uptime, Replay Ratio, Goal DAG counts).
//! - Continuous thought subtitle ticker at the screen base.

use crate::font8x8::FONT_8X8_BASIC;
use crate::sovereign::{get_sovereign_loop, GoalStatus, SovereignPhase, SovereignTelemetry};
use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Picture-in-Picture viewport metadata and state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HudPipFrame {
    pub kind: String, // "web" or "desktop"
    pub title: String,
    pub active: bool,
    pub target_label: Option<String>,
    pub reticle_pos: Option<(i32, i32)>,
}

/// An orbital goal node positioned on a ring around the core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrbitalGoal {
    pub id: String,
    pub intent: String,
    pub status: GoalStatus,
    pub ring: usize,
    pub angle_deg: f64,
}

/// Aggregated Jarvis HUD state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JarvisHudState {
    pub phase: SovereignPhase,
    pub pulse_tick: u64,
    pub telemetry: SovereignTelemetry,
    pub thought_ticker: String,
    pub ticker_history: VecDeque<String>,
    pub pip_web: Option<HudPipFrame>,
    pub pip_desktop: Option<HudPipFrame>,
    pub orbital_goals: Vec<OrbitalGoal>,
}

impl Default for JarvisHudState {
    fn default() -> Self {
        let dummy_telem = SovereignTelemetry {
            phase: SovereignPhase::Idle,
            active_goal_id: None,
            total_goals: 0,
            completed_goals: 0,
            failed_goals: 0,
            active_goals: 0,
            uptime_secs: 0,
            tick_count: 0,
            replay_ratio: 1.0,
            current_thought: "Jarvis HUD initialized in watch-only observer mode.".to_string(),
        };

        Self {
            phase: SovereignPhase::Idle,
            pulse_tick: 0,
            telemetry: dummy_telem,
            thought_ticker: "Jarvis HUD initialized in watch-only observer mode.".to_string(),
            ticker_history: VecDeque::new(),
            pip_web: None,
            pip_desktop: None,
            orbital_goals: Vec::new(),
        }
    }
}

/// Manager orchestrating HUD rendering and telemetry sync.
pub struct JarvisHudEngine {
    state: JarvisHudState,
    start_time: Instant,
}

impl Default for JarvisHudEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JarvisHudEngine {
    pub fn new() -> Self {
        Self {
            state: JarvisHudState::default(),
            start_time: Instant::now(),
        }
    }

    /// Synchronize HUD state with the sovereign loop supervisor.
    pub fn sync_with_sovereign(&mut self) {
        if let Ok(sl) = get_sovereign_loop().lock() {
            let telem = sl.get_telemetry();
            self.state.phase = telem.phase;
            let thought = telem.current_thought.clone();
            self.state.telemetry = telem;
            if !thought.is_empty() {
                self.push_ticker(&thought);
            }

            // Sync goals to orbital nodes
            let goals = sl.list_goals();
            self.state.orbital_goals.clear();
            let count = goals.len().max(1);
            for (idx, g) in goals.into_iter().enumerate() {
                let angle_deg = (idx as f64 * (360.0 / count as f64)
                    + (self.state.pulse_tick as f64 * 1.5))
                    % 360.0;
                let ring = (idx % 3) + 1;
                self.state.orbital_goals.push(OrbitalGoal {
                    id: g.id,
                    intent: g.intent,
                    status: g.status,
                    ring,
                    angle_deg,
                });
            }
        }
    }

    /// Advance pulse tick for smooth animation transitions.
    pub fn advance_tick(&mut self) {
        self.state.pulse_tick = self.state.pulse_tick.saturating_add(1);
        self.sync_with_sovereign();
    }

    /// Push a new thought message to the live ticker.
    pub fn push_ticker(&mut self, msg: &str) {
        if self.state.thought_ticker != msg {
            self.state.thought_ticker = msg.to_string();
            if self.state.ticker_history.len() >= 50 {
                self.state.ticker_history.pop_front();
            }
            self.state.ticker_history.push_back(msg.to_string());
        }
    }

    /// Set PiP viewport parameters.
    pub fn set_pip(&mut self, kind: &str, active: bool, title: &str, target: Option<&str>) {
        let frame = HudPipFrame {
            kind: kind.to_string(),
            title: title.to_string(),
            active,
            target_label: target.map(|s| s.to_string()),
            reticle_pos: if active { Some((80, 50)) } else { None },
        };

        if kind == "web" {
            self.state.pip_web = if active { Some(frame) } else { None };
        } else if kind == "desktop" || kind == "os" {
            self.state.pip_desktop = if active { Some(frame) } else { None };
        }
    }

    /// Read-only snapshot of current HUD state.
    pub fn state(&self) -> &JarvisHudState {
        &self.state
    }

    /// Render a high-resolution software framebuffer of the HUD (RGBA format).
    pub fn render_frame(&self, width: usize, height: usize) -> Vec<u8> {
        let mut buf = vec![0u8; width * height * 4];

        // 1. Fill dark cybernetic backdrop (Deep Navy / Charcoal: #070913)
        let (chunks, _) = buf.as_chunks_mut::<4>();
        for chunk in chunks {
            chunk[0] = 7; // R
            chunk[1] = 9; // G
            chunk[2] = 19; // B
            chunk[3] = 255; // A
        }

        // 2. Draw Top Ambient Glass Telemetry Header
        self.draw_top_telemetry_bar(&mut buf, width, height);

        // 3. Draw Central Orbital Core & Concentric Rings
        self.draw_orbital_core(&mut buf, width, height);

        // 4. Draw Docking PiP Viewports (Top-Left Web, Top-Right OS)
        self.draw_pip_viewports(&mut buf, width, height);

        // 5. Draw Bottom Thought Subtitle Ticker
        self.draw_bottom_ticker(&mut buf, width, height);

        buf
    }

    /// Render and composite directly onto an existing `u32` ARGB framebuffer with ZERO heap allocations.
    pub fn render_to_u32_buffer(&self, buf: &mut [u32], width: usize, height: usize) {
        let count = (width * height).min(buf.len());
        if count == 0 {
            return;
        }

        // 1. Direct hardware background clear (#070913: R=7, G=9, B=19)
        buf[..count].fill(0xFF07_0913);

        // 2. Draw Top Ambient Glass Telemetry Header
        self.draw_top_telemetry_bar_u32(buf, width, height);

        // 3. Draw Central Orbital Core & Concentric Rings
        self.draw_orbital_core_u32(buf, width, height);

        // 4. Draw Docking PiP Viewports (Top-Left Web, Top-Right OS)
        self.draw_pip_viewports_u32(buf, width, height);

        // 5. Draw Bottom Thought Subtitle Ticker
        self.draw_bottom_ticker_u32(buf, width, height);
    }

    fn draw_top_telemetry_bar_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let bar_h = 42;
        if height < bar_h {
            return;
        }

        let header_bg = 0xE60C_1120;
        for y in 0..bar_h {
            for x in 0..width {
                let idx = y * width + x;
                if idx < buf.len() {
                    blend_pixel_u32(&mut buf[idx], header_bg);
                }
            }
        }

        // Header bottom border line (Cyan neon: #06b6d4)
        let border_cyan = 0xFF06_B6D4;
        let border_y = bar_h - 1;
        for x in 0..width {
            let idx = border_y * width + x;
            if idx < buf.len() {
                buf[idx] = border_cyan;
            }
        }

        let uptime_str = format!("UPTIME: {}s", self.start_time.elapsed().as_secs());
        let ratio_pct = (self.state.telemetry.replay_ratio * 100.0).clamp(0.0, 100.0);
        let ratio_str = format!("REPLAY ECONOMY: {ratio_pct:.1}% $0-COST");
        let goals_str = format!(
            "DAG: {} ACTIVE | {} COMPLETED | {} FAILED",
            self.state.telemetry.active_goals,
            self.state.telemetry.completed_goals,
            self.state.telemetry.failed_goals
        );
        let phase_str = format!("[PHASE: {}]", self.state.phase.as_str().to_uppercase());

        draw_ascii_text_u32(buf, width, 16, 16, "JARVIS HUD", 0xFF38_BDF8);
        draw_ascii_text_u32(
            buf,
            width,
            120,
            16,
            &phase_str,
            phase_color_u32(self.state.phase),
        );
        draw_ascii_text_u32(buf, width, 260, 16, &uptime_str, 0xFF94_A3B8);
        draw_ascii_text_u32(buf, width, 400, 16, &ratio_str, 0xFF34_D399);
        draw_ascii_text_u32(buf, width, 680, 16, &goals_str, 0xFF94_A3B8);
    }

    fn draw_orbital_core_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let center_x = (width / 2) as i32;
        let center_y = (height / 2) as i32;

        let core_color = phase_color_u32(self.state.phase);

        let r1 = 80i32;
        let r2 = 130i32;
        let r3 = 180i32;

        draw_circle_u32(buf, width, height, center_x, center_y, r1, 0xB41E_293B);
        draw_circle_u32(buf, width, height, center_x, center_y, r2, 0x8C1E_293B);
        draw_circle_u32(buf, width, height, center_x, center_y, r3, 0x641E_293B);

        let pulse_delta = ((self.state.pulse_tick % 20) as f64 / 20.0 * 6.0) as i32;
        let core_radius = 32 + pulse_delta;
        draw_filled_circle_u32(
            buf,
            width,
            height,
            center_x,
            center_y,
            core_radius,
            core_color,
        );

        let core_label = match self.state.phase {
            SovereignPhase::Idle => "IDLE",
            SovereignPhase::EvaluatingGoal => "EVAL",
            SovereignPhase::ExecutingPlan => "EXEC",
            SovereignPhase::SelfHealing => "HEAL",
            SovereignPhase::Snapshotting => "SNAP",
        };
        draw_ascii_text_u32(
            buf,
            width,
            (center_x - 16) as usize,
            (center_y - 4) as usize,
            core_label,
            0xFFFF_FFFF,
        );

        for goal in &self.state.orbital_goals {
            let ring_r = match goal.ring {
                1 => r1,
                2 => r2,
                _ => r3,
            };
            let rad = goal.angle_deg.to_radians();
            let nx = center_x + (ring_r as f64 * rad.cos()) as i32;
            let ny = center_y + (ring_r as f64 * rad.sin()) as i32;

            let node_color = match goal.status {
                GoalStatus::Completed => 0xFF34_D399,
                GoalStatus::Active | GoalStatus::Evaluating => 0xFFFB_BF24,
                GoalStatus::Healing => 0xFFA8_55F7,
                GoalStatus::Failed => 0xFFF8_7171,
                GoalStatus::Pending => 0xFF94_A3B8,
            };

            draw_filled_circle_u32(buf, width, height, nx, ny, 6, node_color);
            if nx > 10 && ny > 10 && (nx as usize + 8) < width && (ny as usize) < height {
                let short_id = if goal.id.len() > 6 {
                    &goal.id[..6]
                } else {
                    &goal.id
                };
                draw_ascii_text_u32(
                    buf,
                    width,
                    (nx + 8) as usize,
                    (ny - 4) as usize,
                    short_id,
                    0xFFE2_E8F0,
                );
            }
        }
    }

    fn draw_pip_viewports_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let pip_w = 240usize;
        let pip_h = 140usize;
        let margin_x = 24usize;
        let margin_y = 60usize;

        // Top-Left: Web Browser PiP Viewport
        if let Some(pip) = &self.state.pip_web {
            if pip.active && width > pip_w + margin_x * 2 {
                draw_hud_box_u32(
                    buf,
                    width,
                    height,
                    margin_x,
                    margin_y,
                    pip_w,
                    pip_h,
                    0xFF06_B6D4,
                );
                draw_ascii_text_u32(
                    buf,
                    width,
                    margin_x + 8,
                    margin_y + 8,
                    &format!("WEB: {}", pip.title),
                    0xFF38_BDF8,
                );
                if let Some(lbl) = &pip.target_label {
                    draw_ascii_text_u32(
                        buf,
                        width,
                        margin_x + 8,
                        margin_y + 24,
                        &format!("TARGET: {lbl}"),
                        0xFFFB_BF24,
                    );
                }
                draw_reticle_u32(
                    buf,
                    width,
                    height,
                    (margin_x + pip_w / 2) as i32,
                    (margin_y + pip_h / 2) as i32,
                    12,
                    0xFF06_B6D4,
                );
            }
        }

        // Top-Right: Desktop OS PiP Viewport
        if let Some(pip) = &self.state.pip_desktop {
            if pip.active && width > pip_w * 2 + margin_x * 2 {
                let rx = width - pip_w - margin_x;
                draw_hud_box_u32(buf, width, height, rx, margin_y, pip_w, pip_h, 0xFFA8_55F7);
                draw_ascii_text_u32(
                    buf,
                    width,
                    rx + 8,
                    margin_y + 8,
                    &format!("OS: {}", pip.title),
                    0xFFC0_84FC,
                );
                if let Some(lbl) = &pip.target_label {
                    draw_ascii_text_u32(
                        buf,
                        width,
                        rx + 8,
                        margin_y + 24,
                        &format!("FOCUS: {lbl}"),
                        0xFFFB_BF24,
                    );
                }
                draw_reticle_u32(
                    buf,
                    width,
                    height,
                    (rx + pip_w / 2) as i32,
                    (margin_y + pip_h / 2) as i32,
                    12,
                    0xFFA8_55F7,
                );
            }
        }
    }

    fn draw_bottom_ticker_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let ticker_h = 36usize;
        if height < ticker_h + 10 {
            return;
        }
        let ty = height - ticker_h;

        let bg_color = 0xF00A_0F1C;
        for y in ty..height {
            for x in 0..width {
                let idx = y * width + x;
                if idx < buf.len() {
                    blend_pixel_u32(&mut buf[idx], bg_color);
                }
            }
        }

        let top_border = 0xFF38_BDF8;
        for x in 0..width {
            let idx = ty * width + x;
            if idx < buf.len() {
                buf[idx] = top_border;
            }
        }

        let prompt_prefix = "THOUGHT >> ";
        draw_ascii_text_u32(buf, width, 16, ty + 12, prompt_prefix, 0xFF38_BDF8);

        let max_chars = (width.saturating_sub(130)) / 8;
        let mut display_text = self.state.thought_ticker.clone();
        if display_text.len() > max_chars && max_chars > 3 {
            display_text.truncate(max_chars.saturating_sub(3));
            display_text.push_str("...");
        }

        draw_ascii_text_u32(buf, width, 104, ty + 12, &display_text, 0xFFE2_E8F0);
    }

    fn draw_top_telemetry_bar(&self, buf: &mut [u8], width: usize, height: usize) {
        let bar_h = 42;
        if height < bar_h {
            return;
        }

        // Draw translucent dark glass header
        for y in 0..bar_h {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                buf[idx] = 12;
                buf[idx + 1] = 17;
                buf[idx + 2] = 32;
                buf[idx + 3] = 230;
            }
        }

        // Header bottom border line (Cyan neon: #06b6d4)
        for x in 0..width {
            let idx = ((bar_h - 1) * width + x) * 4;
            buf[idx] = 6;
            buf[idx + 1] = 182;
            buf[idx + 2] = 212;
            buf[idx + 3] = 255;
        }

        // Telemetry strings
        let uptime_str = format!("UPTIME: {}s", self.start_time.elapsed().as_secs());
        let ratio_pct = (self.state.telemetry.replay_ratio * 100.0).clamp(0.0, 100.0);
        let ratio_str = format!("REPLAY ECONOMY: {ratio_pct:.1}% $0-COST");
        let goals_str = format!(
            "DAG: {} ACTIVE | {} COMPLETED | {} FAILED",
            self.state.telemetry.active_goals,
            self.state.telemetry.completed_goals,
            self.state.telemetry.failed_goals
        );
        let phase_str = format!("[PHASE: {}]", self.state.phase.as_str().to_uppercase());

        draw_ascii_text(buf, width, 16, 16, "JARVIS HUD", [56, 189, 248, 255]);
        draw_ascii_text(
            buf,
            width,
            120,
            16,
            &phase_str,
            phase_color(self.state.phase),
        );
        draw_ascii_text(buf, width, 260, 16, &uptime_str, [148, 163, 184, 255]);
        draw_ascii_text(buf, width, 400, 16, &ratio_str, [52, 211, 153, 255]);
        draw_ascii_text(buf, width, 680, 16, &goals_str, [148, 163, 184, 255]);
    }

    fn draw_orbital_core(&self, buf: &mut [u8], width: usize, height: usize) {
        let center_x = (width / 2) as i32;
        let center_y = (height / 2) as i32;

        let core_color = phase_color(self.state.phase);

        // Concentric Orbital Rings
        let r1 = 80i32;
        let r2 = 130i32;
        let r3 = 180i32;

        draw_circle(
            buf,
            width,
            height,
            center_x,
            center_y,
            r1,
            [30, 41, 59, 180],
        );
        draw_circle(
            buf,
            width,
            height,
            center_x,
            center_y,
            r2,
            [30, 41, 59, 140],
        );
        draw_circle(
            buf,
            width,
            height,
            center_x,
            center_y,
            r3,
            [30, 41, 59, 100],
        );

        // Central Pulsing Core
        let pulse_delta = ((self.state.pulse_tick % 20) as f64 / 20.0 * 6.0) as i32;
        let core_radius = 32 + pulse_delta;
        draw_filled_circle(
            buf,
            width,
            height,
            center_x,
            center_y,
            core_radius,
            core_color,
        );

        // Core Inner Label
        let core_label = match self.state.phase {
            SovereignPhase::Idle => "IDLE",
            SovereignPhase::EvaluatingGoal => "EVAL",
            SovereignPhase::ExecutingPlan => "EXEC",
            SovereignPhase::SelfHealing => "HEAL",
            SovereignPhase::Snapshotting => "SNAP",
        };
        draw_ascii_text(
            buf,
            width,
            (center_x - 16) as usize,
            (center_y - 4) as usize,
            core_label,
            [255, 255, 255, 255],
        );

        // Orbiting Goal Nodes
        for goal in &self.state.orbital_goals {
            let ring_r = match goal.ring {
                1 => r1,
                2 => r2,
                _ => r3,
            };
            let rad = goal.angle_deg.to_radians();
            let nx = center_x + (ring_r as f64 * rad.cos()) as i32;
            let ny = center_y + (ring_r as f64 * rad.sin()) as i32;

            let node_color = match goal.status {
                GoalStatus::Completed => [52, 211, 153, 255], // Emerald
                GoalStatus::Active | GoalStatus::Evaluating => [251, 191, 36, 255], // Amber
                GoalStatus::Healing => [168, 85, 247, 255],   // Violet
                GoalStatus::Failed => [248, 113, 113, 255],   // Crimson
                GoalStatus::Pending => [148, 163, 184, 255],  // Slate
            };

            draw_filled_circle(buf, width, height, nx, ny, 6, node_color);
        }
    }

    fn draw_pip_viewports(&self, buf: &mut [u8], width: usize, height: usize) {
        let pip_w = 200usize;
        let pip_h = 120usize;
        let margin_x = 20usize;
        let margin_y = 60usize;

        // Top-Left: Web PiP Viewport
        if let Some(pip) = &self.state.pip_web {
            if pip.active && width > pip_w + margin_x * 2 {
                draw_hud_box(
                    buf,
                    width,
                    height,
                    margin_x,
                    margin_y,
                    pip_w,
                    pip_h,
                    [6, 182, 212, 255],
                );
                draw_ascii_text(
                    buf,
                    width,
                    margin_x + 8,
                    margin_y + 8,
                    &format!("WEB: {}", pip.title),
                    [56, 189, 248, 255],
                );
                if let Some(lbl) = &pip.target_label {
                    draw_ascii_text(
                        buf,
                        width,
                        margin_x + 8,
                        margin_y + 24,
                        &format!("TARGET: {lbl}"),
                        [251, 191, 36, 255],
                    );
                }
                // Reticle crosshair
                draw_reticle(
                    buf,
                    width,
                    height,
                    (margin_x + pip_w / 2) as i32,
                    (margin_y + pip_h / 2) as i32,
                    12,
                    [6, 182, 212, 255],
                );
            }
        }

        // Top-Right: Desktop OS PiP Viewport
        if let Some(pip) = &self.state.pip_desktop {
            if pip.active && width > pip_w * 2 + margin_x * 2 {
                let rx = width - pip_w - margin_x;
                draw_hud_box(
                    buf,
                    width,
                    height,
                    rx,
                    margin_y,
                    pip_w,
                    pip_h,
                    [168, 85, 247, 255],
                );
                draw_ascii_text(
                    buf,
                    width,
                    rx + 8,
                    margin_y + 8,
                    &format!("OS: {}", pip.title),
                    [192, 132, 252, 255],
                );
                if let Some(lbl) = &pip.target_label {
                    draw_ascii_text(
                        buf,
                        width,
                        rx + 8,
                        margin_y + 24,
                        &format!("FOCUS: {lbl}"),
                        [251, 191, 36, 255],
                    );
                }
                // Reticle crosshair
                draw_reticle(
                    buf,
                    width,
                    height,
                    (rx + pip_w / 2) as i32,
                    (margin_y + pip_h / 2) as i32,
                    12,
                    [168, 85, 247, 255],
                );
            }
        }
    }

    fn draw_bottom_ticker(&self, buf: &mut [u8], width: usize, height: usize) {
        let ticker_h = 36usize;
        if height < ticker_h + 10 {
            return;
        }
        let ty = height - ticker_h;

        // Bottom glass ticker panel
        for y in ty..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                buf[idx] = 10;
                buf[idx + 1] = 15;
                buf[idx + 2] = 28;
                buf[idx + 3] = 240;
            }
        }

        // Ticker top border
        for x in 0..width {
            let idx = (ty * width + x) * 4;
            buf[idx] = 56;
            buf[idx + 1] = 189;
            buf[idx + 2] = 248;
            buf[idx + 3] = 255;
        }

        // Draw subtitle text
        let prompt_prefix = "THOUGHT >> ";
        draw_ascii_text(buf, width, 16, ty + 12, prompt_prefix, [56, 189, 248, 255]);

        let max_chars = (width.saturating_sub(130)) / 8;
        let mut display_text = self.state.thought_ticker.clone();
        if display_text.len() > max_chars && max_chars > 3 {
            display_text.truncate(max_chars.saturating_sub(3));
            display_text.push_str("...");
        }

        draw_ascii_text(
            buf,
            width,
            104,
            ty + 12,
            &display_text,
            [226, 232, 240, 255],
        );
    }
}

fn phase_color(phase: SovereignPhase) -> [u8; 4] {
    match phase {
        SovereignPhase::Idle => [52, 211, 153, 255], // Emerald Steady
        SovereignPhase::EvaluatingGoal => [251, 191, 36, 255], // Amber Pulse
        SovereignPhase::ExecutingPlan => [56, 189, 248, 255], // Azure Steady
        SovereignPhase::SelfHealing => [168, 85, 247, 255], // Violet Double-Helix
        SovereignPhase::Snapshotting => [244, 114, 182, 255], // Pink Glow
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_hud_box(
    buf: &mut [u8],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    border_color: [u8; 4],
) {
    if x + w >= width || y + h >= height {
        return;
    }

    // Fill semi-transparent dark body
    for py in y..(y + h) {
        for px in x..(x + w) {
            let idx = (py * width + px) * 4;
            buf[idx] = 15;
            buf[idx + 1] = 23;
            buf[idx + 2] = 42;
            buf[idx + 3] = 220;
        }
    }

    // Draw Corner Brackets (length 14 px)
    let br_len = 14usize.min(w / 2).min(h / 2);

    // Top-Left
    for i in 0..br_len {
        set_pixel(buf, width, x + i, y, border_color);
        set_pixel(buf, width, x, y + i, border_color);
    }
    // Top-Right
    for i in 0..br_len {
        set_pixel(buf, width, x + w - 1 - i, y, border_color);
        set_pixel(buf, width, x + w - 1, y + i, border_color);
    }
    // Bottom-Left
    for i in 0..br_len {
        set_pixel(buf, width, x + i, y + h - 1, border_color);
        set_pixel(buf, width, x, y + h - 1 - i, border_color);
    }
    // Bottom-Right
    for i in 0..br_len {
        set_pixel(buf, width, x + w - 1 - i, y + h - 1, border_color);
        set_pixel(buf, width, x + w - 1, y + h - 1 - i, border_color);
    }
}

fn draw_reticle(
    buf: &mut [u8],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    size: i32,
    color: [u8; 4],
) {
    draw_circle(buf, width, height, cx, cy, size, color);

    for offset in (size - 4)..(size + 4) {
        set_pixel_i(buf, width, height, cx + offset, cy, color);
        set_pixel_i(buf, width, height, cx - offset, cy, color);
        set_pixel_i(buf, width, height, cx, cy + offset, color);
        set_pixel_i(buf, width, height, cx, cy - offset, color);
    }
}

fn draw_circle(
    buf: &mut [u8],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    color: [u8; 4],
) {
    let mut x = radius;
    let mut y = 0i32;
    let mut err = 0i32;

    while x >= y {
        set_pixel_i(buf, width, height, cx + x, cy + y, color);
        set_pixel_i(buf, width, height, cx + y, cy + x, color);
        set_pixel_i(buf, width, height, cx - y, cy + x, color);
        set_pixel_i(buf, width, height, cx - x, cy + y, color);
        set_pixel_i(buf, width, height, cx - x, cy - y, color);
        set_pixel_i(buf, width, height, cx - y, cy - x, color);
        set_pixel_i(buf, width, height, cx + y, cy - x, color);
        set_pixel_i(buf, width, height, cx + x, cy - y, color);

        y += 1;
        err += 1 + 2 * y;
        if 2 * (err - x) + 1 > 0 {
            x -= 1;
            err += 1 - 2 * x;
        }
    }
}

fn draw_filled_circle(
    buf: &mut [u8],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    color: [u8; 4],
) {
    let r2 = radius * radius;
    for dy in -radius..=radius {
        let py = cy + dy;
        if py < 0 || py >= height as i32 {
            continue;
        }
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= r2 {
                let px = cx + dx;
                if px >= 0 && px < width as i32 {
                    set_pixel(buf, width, px as usize, py as usize, color);
                }
            }
        }
    }
}

fn set_pixel(buf: &mut [u8], width: usize, x: usize, y: usize, color: [u8; 4]) {
    let idx = (y * width + x) * 4;
    if idx + 3 < buf.len() {
        buf[idx] = color[0];
        buf[idx + 1] = color[1];
        buf[idx + 2] = color[2];
        buf[idx + 3] = color[3];
    }
}

fn set_pixel_i(buf: &mut [u8], width: usize, height: usize, x: i32, y: i32, color: [u8; 4]) {
    if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
        set_pixel(buf, width, x as usize, y as usize, color);
    }
}

fn draw_ascii_text(
    buf: &mut [u8],
    width: usize,
    start_x: usize,
    start_y: usize,
    text: &str,
    color: [u8; 4],
) {
    let mut cur_x = start_x;
    for ch in text.chars() {
        let ascii = (ch as usize).min(127);
        let glyph = FONT_8X8_BASIC[ascii];
        for (row, &row_byte) in glyph.iter().enumerate() {
            for col in 0..8 {
                if (row_byte >> col) & 1 == 1 {
                    set_pixel(buf, width, cur_x + col, start_y + row, color);
                }
            }
        }
        cur_x = cur_x.saturating_add(8);
    }
}

fn phase_color_u32(phase: SovereignPhase) -> u32 {
    match phase {
        SovereignPhase::Idle => 0xFF34_D399,
        SovereignPhase::EvaluatingGoal => 0xFFFB_BF24,
        SovereignPhase::ExecutingPlan => 0xFF38_BDF8,
        SovereignPhase::SelfHealing => 0xFFA8_55F7,
        SovereignPhase::Snapshotting => 0xFFF4_72B6,
    }
}

#[inline(always)]
fn blend_pixel_u32(dest: &mut u32, src: u32) {
    let sa = (src >> 24) & 0xFF;
    if sa == 255 {
        *dest = src;
    } else if sa > 0 {
        let sr = (src >> 16) & 0xFF;
        let sg = (src >> 8) & 0xFF;
        let sb = src & 0xFF;

        let orig = *dest;
        let or = (orig >> 16) & 0xFF;
        let og = (orig >> 8) & 0xFF;
        let ob = orig & 0xFF;

        let inv_a = 255 - sa;
        let nr = (sr * sa + or * inv_a) / 255;
        let ng = (sg * sa + og * inv_a) / 255;
        let nb = (sb * sa + ob * inv_a) / 255;
        *dest = 0xFF00_0000 | (nr << 16) | (ng << 8) | nb;
    }
}

#[inline(always)]
fn set_pixel_u32(buf: &mut [u32], width: usize, x: usize, y: usize, color: u32) {
    let idx = y * width + x;
    if idx < buf.len() {
        buf[idx] = color;
    }
}

#[inline(always)]
fn set_pixel_i_u32(buf: &mut [u32], width: usize, height: usize, x: i32, y: i32, color: u32) {
    if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
        let idx = (y as usize) * width + (x as usize);
        if idx < buf.len() {
            buf[idx] = color;
        }
    }
}

fn draw_circle_u32(
    buf: &mut [u32],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    color: u32,
) {
    let mut x = radius;
    let mut y = 0i32;
    let mut err = 0i32;

    while x >= y {
        set_pixel_i_u32(buf, width, height, cx + x, cy + y, color);
        set_pixel_i_u32(buf, width, height, cx + y, cy + x, color);
        set_pixel_i_u32(buf, width, height, cx - y, cy + x, color);
        set_pixel_i_u32(buf, width, height, cx - x, cy + y, color);
        set_pixel_i_u32(buf, width, height, cx - x, cy - y, color);
        set_pixel_i_u32(buf, width, height, cx - y, cy - x, color);
        set_pixel_i_u32(buf, width, height, cx + y, cy - x, color);
        set_pixel_i_u32(buf, width, height, cx + x, cy - y, color);

        y += 1;
        err += 1 + 2 * y;
        if 2 * (err - x) + 1 > 0 {
            x -= 1;
            err += 1 - 2 * x;
        }
    }
}

fn draw_filled_circle_u32(
    buf: &mut [u32],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    color: u32,
) {
    let r2 = radius * radius;
    for dy in -radius..=radius {
        let py = cy + dy;
        if py < 0 || py >= height as i32 {
            continue;
        }
        for dx in -radius..=radius {
            if dx * dx + dy * dy <= r2 {
                let px = cx + dx;
                if px >= 0 && px < width as i32 {
                    set_pixel_u32(buf, width, px as usize, py as usize, color);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_hud_box_u32(
    buf: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    border_color: u32,
) {
    if x + w >= width || y + h >= height {
        return;
    }

    let body_color = 0xDC0F_172A;
    for py in y..(y + h) {
        for px in x..(x + w) {
            let idx = py * width + px;
            if idx < buf.len() {
                blend_pixel_u32(&mut buf[idx], body_color);
            }
        }
    }

    let br_len = 14usize.min(w / 2).min(h / 2);
    for i in 0..br_len {
        set_pixel_u32(buf, width, x + i, y, border_color);
        set_pixel_u32(buf, width, x, y + i, border_color);
        set_pixel_u32(buf, width, x + w - 1 - i, y, border_color);
        set_pixel_u32(buf, width, x + w - 1, y + i, border_color);
        set_pixel_u32(buf, width, x + i, y + h - 1, border_color);
        set_pixel_u32(buf, width, x, y + h - 1 - i, border_color);
        set_pixel_u32(buf, width, x + w - 1 - i, y + h - 1, border_color);
        set_pixel_u32(buf, width, x + w - 1, y + h - 1 - i, border_color);
    }
}

fn draw_reticle_u32(
    buf: &mut [u32],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    size: i32,
    color: u32,
) {
    draw_circle_u32(buf, width, height, cx, cy, size, color);

    for offset in (size - 4)..(size + 4) {
        set_pixel_i_u32(buf, width, height, cx + offset, cy, color);
        set_pixel_i_u32(buf, width, height, cx - offset, cy, color);
        set_pixel_i_u32(buf, width, height, cx, cy + offset, color);
        set_pixel_i_u32(buf, width, height, cx, cy - offset, color);
    }
}

fn draw_ascii_text_u32(
    buf: &mut [u32],
    width: usize,
    start_x: usize,
    start_y: usize,
    text: &str,
    color: u32,
) {
    let mut cur_x = start_x;
    for ch in text.chars() {
        let ascii = (ch as usize).min(127);
        let glyph = FONT_8X8_BASIC[ascii];
        for (row, &row_byte) in glyph.iter().enumerate() {
            for col in 0..8 {
                if (row_byte >> col) & 1 == 1 {
                    set_pixel_u32(buf, width, cur_x + col, start_y + row, color);
                }
            }
        }
        cur_x = cur_x.saturating_add(8);
    }
}

/// Global process-wide Jarvis HUD engine singleton.
static HUD_ENGINE: OnceLock<Mutex<JarvisHudEngine>> = OnceLock::new();

pub fn get_hud_engine() -> &'static Mutex<JarvisHudEngine> {
    HUD_ENGINE.get_or_init(|| Mutex::new(JarvisHudEngine::new()))
}

pub fn with_hud<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut JarvisHudEngine) -> Result<R, String>,
{
    let mutex = get_hud_engine();
    let mut hud = mutex
        .lock()
        .map_err(|e| format!("error[E0970]: JarvisHudEngine lock poisoned: {e}"))?;
    f(&mut hud)
}

/// Register HUD functions in standard library dispatch table.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("hud.state".to_string(), Value::Native("hud.state"));
    functions.insert("hud.ticker".to_string(), Value::Native("hud.ticker"));
    functions.insert("hud.set_pip".to_string(), Value::Native("hud.set_pip"));
    functions.insert("hud.advance".to_string(), Value::Native("hud.advance"));
    functions.insert("hud.render".to_string(), Value::Native("hud.render"));
    functions
}

/// Dispatch a call to a HUD builtin.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "hud.state" => with_hud(|h| {
            h.sync_with_sovereign();
            let mut map = HashMap::new();
            map.insert(
                "phase".to_string(),
                Value::String(h.state().phase.as_str().to_string()),
            );
            map.insert(
                "pulse_tick".to_string(),
                Value::Int(h.state().pulse_tick as i64),
            );
            map.insert(
                "thought".to_string(),
                Value::String(h.state().thought_ticker.clone()),
            );
            map.insert(
                "orbital_count".to_string(),
                Value::Int(h.state().orbital_goals.len() as i64),
            );
            map.insert(
                "pip_web_active".to_string(),
                Value::Bool(
                    h.state()
                        .pip_web
                        .as_ref()
                        .map(|p| p.active)
                        .unwrap_or(false),
                ),
            );
            map.insert(
                "pip_os_active".to_string(),
                Value::Bool(
                    h.state()
                        .pip_desktop
                        .as_ref()
                        .map(|p| p.active)
                        .unwrap_or(false),
                ),
            );
            Ok(Value::Map(map))
        }),

        "hud.ticker" => {
            if args.is_empty() {
                return Err("hud.ticker requires message argument".to_string());
            }
            let msg = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("hud.ticker: message must be string".to_string()),
            };

            with_hud(|h| {
                h.push_ticker(msg);
                Ok(Value::Bool(true))
            })
        }

        "hud.set_pip" => {
            if args.len() < 3 {
                return Err("hud.set_pip requires (kind, active, title, [target])".to_string());
            }
            let kind = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("hud.set_pip: kind must be string".to_string()),
            };
            let active = match &args[1] {
                Value::Bool(b) => *b,
                _ => return Err("hud.set_pip: active must be bool".to_string()),
            };
            let title = match &args[2] {
                Value::String(s) => s.as_str(),
                _ => return Err("hud.set_pip: title must be string".to_string()),
            };
            let target = match args.get(3) {
                Some(Value::String(s)) => Some(s.as_str()),
                _ => None,
            };

            with_hud(|h| {
                h.set_pip(kind, active, title, target);
                Ok(Value::Bool(true))
            })
        }

        "hud.advance" => with_hud(|h| {
            h.advance_tick();
            Ok(Value::Int(h.state().pulse_tick as i64))
        }),

        "hud.render" => {
            let width = match args.first() {
                Some(Value::Int(w)) => (*w).clamp(320, 3840) as usize,
                _ => 1024,
            };
            let height = match args.get(1) {
                Some(Value::Int(h)) => (*h).clamp(240, 2160) as usize,
                _ => 768,
            };

            with_hud(|h| {
                h.sync_with_sovereign();
                let frame = h.render_frame(width, height);
                let mut map = HashMap::new();
                map.insert("width".to_string(), Value::Int(width as i64));
                map.insert("height".to_string(), Value::Int(height as i64));
                map.insert("byte_count".to_string(), Value::Int(frame.len() as i64));
                Ok(Value::Map(map))
            })
        }

        _ => Err(format!("unknown hud function: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jarvis_hud_lifecycle_and_rendering() {
        let mut engine = JarvisHudEngine::new();

        assert_eq!(engine.state().pulse_tick, 0);
        engine.push_ticker("Testing Jarvis orbital core alignment.");
        assert_eq!(
            engine.state().thought_ticker,
            "Testing Jarvis orbital core alignment."
        );

        engine.set_pip("web", true, "Amazon Price Monitor", Some("#buy-button"));
        assert!(engine.state().pip_web.as_ref().unwrap().active);

        engine.set_pip("desktop", true, "GIMP Export Dialog", Some("OK Button"));
        assert!(engine.state().pip_desktop.as_ref().unwrap().active);

        engine.advance_tick();
        assert_eq!(engine.state().pulse_tick, 1);

        // Render software frame
        let frame = engine.render_frame(800, 600);
        assert_eq!(frame.len(), 800 * 600 * 4);

        // Check non-zero pixels exist in buffer
        let non_zero = frame.iter().any(|&b| b > 0);
        assert!(non_zero);
    }
}
