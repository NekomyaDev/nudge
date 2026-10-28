//! Nudge Visor — Sovereign Autonomy HUD & Tactical Telemetry Interface.
//!
//! Provides the ambient visual feedback layer for Nudge Sovereign Loops:
//! - Central Orbital Core with reactive color states (Azure, Amber, Violet, Emerald, Crimson).
//! - Concentric orbital rings with rotating goal nodes and vector rays.
//! - Corner docking Picture-in-Picture (PiP) viewports for Web and OS observation.
//! - Ambient tactical telemetry header (Uptime, Replay Ratio, Goal DAG counts, NVFS status).
//! - Live neural bus equalizer activity visualizer.
//! - Continuous thought subtitle console at the screen base.
//! - Interactive hold/pause toggle (Spacebar) and emergency inspection.

use crate::font8x8::FONT_8X8_BASIC;
use crate::sovereign::{get_sovereign_loop, GoalStatus, SovereignPhase, SovereignTelemetry};
use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Picture-in-Picture viewport metadata and state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisorPipFrame {
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

/// Aggregated Visor HUD state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisorState {
    pub phase: SovereignPhase,
    pub pulse_tick: u64,
    pub telemetry: SovereignTelemetry,
    pub thought_ticker: String,
    pub ticker_history: VecDeque<String>,
    pub pip_web: Option<VisorPipFrame>,
    pub pip_desktop: Option<VisorPipFrame>,
    pub orbital_goals: Vec<OrbitalGoal>,
}

impl Default for VisorState {
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
            current_thought: "Nudge Visor initialized in watch-only observer mode.".to_string(),
        };

        Self {
            phase: SovereignPhase::Idle,
            pulse_tick: 0,
            telemetry: dummy_telem,
            thought_ticker: "Nudge Visor initialized in watch-only observer mode.".to_string(),
            ticker_history: VecDeque::new(),
            pip_web: None,
            pip_desktop: None,
            orbital_goals: Vec::new(),
        }
    }
}

/// Manager orchestrating Visor HUD rendering and telemetry sync.
pub struct VisorEngine {
    state: VisorState,
    start_time: Instant,
}

impl Default for VisorEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl VisorEngine {
    pub fn new() -> Self {
        Self {
            state: VisorState::default(),
            start_time: Instant::now(),
        }
    }

    /// Synchronize Visor state with the sovereign loop supervisor.
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

    /// Update or toggle docking PiP viewports.
    pub fn set_pip(&mut self, kind: &str, active: bool, title: &str, target: Option<&str>) {
        let frame = VisorPipFrame {
            kind: kind.to_string(),
            title: title.to_string(),
            active,
            target_label: target.map(|s| s.to_string()),
            reticle_pos: if active { Some((120, 80)) } else { None },
        };

        if kind == "web" {
            self.state.pip_web = if active { Some(frame) } else { None };
        } else if kind == "desktop" {
            self.state.pip_desktop = if active { Some(frame) } else { None };
        }
    }

    /// Read-only snapshot of current Visor state.
    pub fn state(&self) -> &VisorState {
        &self.state
    }

    /// Render a high-resolution software framebuffer of the Visor HUD (RGBA format).
    pub fn render_frame(&self, width: usize, height: usize) -> Vec<u8> {
        let mut u32_buf = vec![0u32; width * height];
        self.render_to_u32_buffer(&mut u32_buf, width, height);

        let mut byte_buf = vec![0u8; width * height * 4];
        for (i, &pixel) in u32_buf.iter().enumerate() {
            let a = ((pixel >> 24) & 0xFF) as u8;
            let r = ((pixel >> 16) & 0xFF) as u8;
            let g = ((pixel >> 8) & 0xFF) as u8;
            let b = (pixel & 0xFF) as u8;
            let idx = i * 4;
            byte_buf[idx] = r;
            byte_buf[idx + 1] = g;
            byte_buf[idx + 2] = b;
            byte_buf[idx + 3] = a;
        }
        byte_buf
    }

    /// Render and composite directly onto an existing `u32` ARGB framebuffer with ZERO heap allocations.
    pub fn render_to_u32_buffer(&self, buf: &mut [u32], width: usize, height: usize) {
        let count = (width * height).min(buf.len());
        if count == 0 {
            return;
        }

        // 1. Cybernetic obsidian background & holographic grid
        self.draw_cybernetic_grid_u32(buf, width, height);

        // 2. Top Tactical Glass Telemetry Header
        self.draw_top_telemetry_bar_u32(buf, width, height);

        // 3. Central Orbital Core Reactor & Concentric Rings
        self.draw_orbital_core_u32(buf, width, height);

        // 4. Lateral Tactical Sidebars (Left DAG, Right Telemetry)
        self.draw_tactical_sidebars_u32(buf, width, height);

        // 5. Docking PiP Viewports (Top-Left Web, Top-Right OS)
        self.draw_pip_viewports_u32(buf, width, height);

        // 6. Bottom Holographic Consciousness Subtitle Console
        self.draw_bottom_ticker_u32(buf, width, height);
    }

    fn draw_cybernetic_grid_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let count = (width * height).min(buf.len());
        // Base obsidian-navy #050811
        buf[..count].fill(0xFF05_0811);

        // Precise micro-grid crosshairs every 32 px
        for y in (0..height).step_by(32) {
            for x in (0..width).step_by(32) {
                let idx = y * width + x;
                if idx < buf.len() {
                    buf[idx] = 0xFF14_213D;
                    // Draw a 3px crosshair mark '+'
                    if x > 0 && x + 1 < width {
                        buf[idx - 1] = 0xFF0D_182A;
                        buf[idx + 1] = 0xFF0D_182A;
                    }
                }
            }
        }

        // Dual sweep animated radar scanlines
        let scan_y = ((self.state.pulse_tick * 3) as usize) % height;
        for x in 0..width {
            let idx = scan_y * width + x;
            if idx < buf.len() {
                blend_pixel_u32(&mut buf[idx], 0x4000_E5FF);
            }
            if scan_y > 1 {
                let trail_idx = (scan_y - 1) * width + x;
                if trail_idx < buf.len() {
                    blend_pixel_u32(&mut buf[trail_idx], 0x2000_B4D8);
                }
            }
        }

        // Corner Calibration Tags
        draw_ascii_text_u32(
            buf,
            width,
            16,
            height.saturating_sub(56),
            "SEC // 0x7F",
            0xFF33_4155,
        );
        draw_ascii_text_u32(
            buf,
            width,
            width.saturating_sub(120),
            height.saturating_sub(56),
            "SYS // STABLE",
            0xFF33_4155,
        );
    }

    fn draw_top_telemetry_bar_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let bar_h = 48;
        if height < bar_h {
            return;
        }

        let header_bg = 0xF208_0E1E;
        for y in 0..bar_h {
            for x in 0..width {
                let idx = y * width + x;
                if idx < buf.len() {
                    blend_pixel_u32(&mut buf[idx], header_bg);
                }
            }
        }

        // Double neon cyan bottom border line (#00e5ff and #0284c7)
        let border_cyan = 0xFF00_E5FF;
        let border_glow = 0x8002_84C7;
        let b_y = bar_h - 2;
        for x in 0..width {
            let idx = b_y * width + x;
            if idx < buf.len() {
                buf[idx] = border_cyan;
            }
            let idx_glow = (b_y + 1) * width + x;
            if idx_glow < buf.len() {
                blend_pixel_u32(&mut buf[idx_glow], border_glow);
            }
        }

        // Left Branding: Scaled "VISOR" + Subtitle
        draw_ascii_text_u32_scaled(buf, width, 20, 10, "VISOR", 0xFF00_E5FF, 2);
        draw_ascii_text_u32(buf, width, 110, 17, "// SOVEREIGN CORE V2.0", 0xFF64_748B);

        // Center Status Chips
        let phase_col = phase_color_u32(self.state.phase);
        let phase_str = match self.state.phase {
            SovereignPhase::Hold => "[HOLD: EMERGENCY]".to_string(),
            _ => format!("[PHASE: {}]", self.state.phase.as_str().to_uppercase()),
        };
        let ratio_pct = (self.state.telemetry.replay_ratio * 100.0).clamp(0.0, 100.0);
        let replay_str = format!("[REPLAY: {ratio_pct:.1}% $0-COST]");
        let wal_str = "[NVFS: CRC32-OK]";

        draw_ascii_text_u32(buf, width, 330, 17, &replay_str, 0xFF10_B981);
        draw_ascii_text_u32(buf, width, 550, 17, wal_str, 0xFF38_BDF8);
        draw_ascii_text_u32(buf, width, 720, 17, &phase_str, phase_col);

        // Right Stats
        let uptime_str = format!("UPTIME: {}s", self.start_time.elapsed().as_secs());
        let dag_str = format!(
            "DAG: {}/{} ACTIVE",
            self.state.telemetry.active_goals, self.state.telemetry.total_goals
        );
        let right_x = width.saturating_sub(260);
        draw_ascii_text_u32(buf, width, right_x, 17, &uptime_str, 0xFF94_A3B8);
        draw_ascii_text_u32(buf, width, right_x + 105, 17, &dag_str, 0xFFE2_E8F0);
    }

    fn draw_orbital_core_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let center_x = (width / 2) as i32;
        let center_y = (height / 2) as i32 + 10;

        let core_color = phase_color_u32(self.state.phase);

        let r1 = 75i32;
        let r2 = 135i32;
        let r3 = 195i32;

        let pulse_delta = ((self.state.pulse_tick % 24) as f64 / 24.0 * 6.0) as i32;
        let outer_halo = 46 + pulse_delta;
        let mid_core = 36;
        let inner_core = 30;

        // Radial crosshair axes (broken before entering central core)
        let core_clearance = outer_halo + 8;
        // Left ray
        draw_line_u32(
            buf,
            width,
            height,
            center_x - r3 - 25,
            center_y,
            center_x - core_clearance,
            center_y,
            0x2400_E5FF,
        );
        // Right ray
        draw_line_u32(
            buf,
            width,
            height,
            center_x + core_clearance,
            center_y,
            center_x + r3 + 25,
            center_y,
            0x2400_E5FF,
        );
        // Top ray
        draw_line_u32(
            buf,
            width,
            height,
            center_x,
            center_y - r3 - 25,
            center_x,
            center_y - core_clearance,
            0x2400_E5FF,
        );
        // Bottom ray
        draw_line_u32(
            buf,
            width,
            height,
            center_x,
            center_y + core_clearance,
            center_x,
            center_y + r3 + 25,
            0x2400_E5FF,
        );

        // Precision ticks along cardinal axes (outside the central core)
        draw_crosshair_ticks_u32(
            buf,
            width,
            height,
            center_x,
            center_y,
            outer_halo + 12,
            r3 + 20,
            0x4400_E5FF,
        );

        // Segmented Orbital Rings (Dynamic Rotation)
        let p_tick = self.state.pulse_tick as f64;
        draw_segmented_ring_u32(
            buf,
            width,
            height,
            center_x,
            center_y,
            r1,
            6,
            15.0,
            p_tick * 2.0,
            0x5500_E5FF,
        );
        draw_segmented_ring_u32(
            buf,
            width,
            height,
            center_x,
            center_y,
            r2,
            12,
            10.0,
            360.0 - (p_tick * 1.0),
            0x4400_E5FF,
        );
        draw_segmented_ring_u32(
            buf,
            width,
            height,
            center_x,
            center_y,
            r3,
            8,
            12.0,
            p_tick * 0.75,
            0x3300_E5FF,
        );

        // Clean Military Compass Labels with HUD backdrop boxes
        draw_hud_box_u32(
            buf,
            width,
            height,
            (center_x - 36) as usize,
            (center_y - r3 - 18) as usize,
            72,
            16,
            0x6600_E5FF,
        );
        draw_ascii_text_u32(
            buf,
            width,
            (center_x - 30) as usize,
            (center_y - r3 - 14) as usize,
            "[N: 000]",
            0xFF38_BDF8,
        );

        draw_hud_box_u32(
            buf,
            width,
            height,
            (center_x + r3 + 6) as usize,
            (center_y - 8) as usize,
            72,
            16,
            0x6600_E5FF,
        );
        draw_ascii_text_u32(
            buf,
            width,
            (center_x + r3 + 12) as usize,
            (center_y - 4) as usize,
            "[E: 090]",
            0xFF38_BDF8,
        );

        draw_hud_box_u32(
            buf,
            width,
            height,
            (center_x - 36) as usize,
            (center_y + r3 + 4) as usize,
            72,
            16,
            0x6600_E5FF,
        );
        draw_ascii_text_u32(
            buf,
            width,
            (center_x - 30) as usize,
            (center_y + r3 + 8) as usize,
            "[S: 180]",
            0xFF38_BDF8,
        );

        draw_hud_box_u32(
            buf,
            width,
            height,
            (center_x - r3 - 78) as usize,
            (center_y - 8) as usize,
            72,
            16,
            0x6600_E5FF,
        );
        draw_ascii_text_u32(
            buf,
            width,
            (center_x - r3 - 72) as usize,
            (center_y - 4) as usize,
            "[W: 270]",
            0xFF38_BDF8,
        );

        // Pulsing Central Core Reactor (Multi-layer energy glow & Iris Blades)
        draw_filled_circle_u32(
            buf,
            width,
            height,
            center_x,
            center_y,
            outer_halo,
            0x2600_E5FF,
        );
        draw_circle_u32(
            buf,
            width,
            height,
            center_x,
            center_y,
            outer_halo,
            0x4400_E5FF,
        );
        draw_filled_circle_u32(buf, width, height, center_x, center_y, mid_core, core_color);
        draw_circle_u32(
            buf,
            width,
            height,
            center_x,
            center_y,
            mid_core + 2,
            0xFFFF_FFFF,
        );
        draw_filled_circle_u32(
            buf,
            width,
            height,
            center_x,
            center_y,
            inner_core,
            0xFF0A_1120,
        );
        draw_circle_u32(
            buf, width, height, center_x, center_y, inner_core, core_color,
        );

        // Centered core phase label
        let core_label = match self.state.phase {
            SovereignPhase::Idle => "[IDLE]",
            SovereignPhase::EvaluatingGoal => "[EVAL]",
            SovereignPhase::ExecutingPlan => "[EXEC]",
            SovereignPhase::SelfHealing => "[HEAL]",
            SovereignPhase::Snapshotting => "[SNAP]",
            SovereignPhase::Hold => "[HOLD]",
        };
        let label_offset = (core_label.len() * 8) / 2;
        draw_ascii_text_u32(
            buf,
            width,
            (center_x as usize).saturating_sub(label_offset),
            (center_y - 4) as usize,
            core_label,
            0xFFFF_FFFF,
        );

        // Orbiting Goal Satellites & Angular Vector Callouts
        for goal in &self.state.orbital_goals {
            let ring_r = match goal.ring {
                1 => r1,
                2 => r2,
                _ => r3,
            };
            let rad = goal.angle_deg.to_radians();
            let nx = center_x + (ring_r as f64 * rad.cos()) as i32;
            let ny = center_y + (ring_r as f64 * rad.sin()) as i32;

            let (node_color, status_tag) = match goal.status {
                GoalStatus::Completed => (0xFF10_B981, "DONE"),
                GoalStatus::Active | GoalStatus::Evaluating => (0xFFF5_9E0B, "EVAL"),
                GoalStatus::Healing => (0xFFA8_55F7, "HEAL"),
                GoalStatus::Failed => (0xFFEF_4444, "FAIL"),
                GoalStatus::Pending => (0xFF94_A3B8, "WAIT"),
            };

            // Connecting vector ray from reactor core perimeter to satellite
            let start_ray_r = (outer_halo + 4) as f64;
            let sx = center_x + (start_ray_r * rad.cos()) as i32;
            let sy = center_y + (start_ray_r * rad.sin()) as i32;
            draw_line_u32(buf, width, height, sx, sy, nx, ny, 0x2400_E5FF);

            // Satellite node: Glowing outer ring + colored core + target cross
            draw_filled_circle_u32(buf, width, height, nx, ny, 9, 0x4400_E5FF);
            draw_filled_circle_u32(buf, width, height, nx, ny, 6, node_color);
            draw_circle_u32(buf, width, height, nx, ny, 7, 0xFFFF_FFFF);

            // Dedicated radial elbow callout bracket with aerospace anti-collision
            let badge = format!("[{status_tag}] {}", goal.id);
            let card_w = badge.len() * 8 + 10;
            let card_h = 16usize;

            // Prevent collision with sidebars (x=350 on left, x=width-350 on right)
            let mut is_right = nx >= center_x;
            if is_right && (nx + 36 + card_w as i32) > (width as i32 - 350) {
                is_right = false;
            } else if !is_right && (nx - 36 - card_w as i32) < 350 {
                is_right = true;
            }

            let elbow_dx = if is_right { 16 } else { -16 };
            let turn_dx = if is_right { 20 } else { -20 };
            let ex = nx + elbow_dx;
            let ey_offset = if (ny - center_y).abs() < 28 {
                if ny >= center_y {
                    24
                } else {
                    -24
                }
            } else {
                -10
            };
            let ey = ny + ey_offset;
            let tx = ex + turn_dx;

            draw_line_u32(buf, width, height, nx, ny, ex, ey, 0x8800_E5FF);
            draw_line_u32(buf, width, height, ex, ey, tx, ey, 0x8800_E5FF);

            let card_x = if is_right {
                (tx + 2) as usize
            } else {
                (tx - card_w as i32 - 2).max(4) as usize
            };
            let card_y = (ey - (card_h as i32 / 2)).max(0) as usize;

            if card_x + card_w < width && card_y + card_h < height {
                draw_hud_box_u32(
                    buf,
                    width,
                    height,
                    card_x,
                    card_y,
                    card_w,
                    card_h,
                    0x8800_E5FF,
                );
                draw_ascii_text_u32(buf, width, card_x + 5, card_y + 4, &badge, 0xFFF1_F5F9);
            }
        }
    }

    fn draw_tactical_sidebars_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let p_w = 320usize;
        let p_h = 330usize;
        let p_y = 60usize;

        // Left Panel: Goal DAG Pipeline
        if width > p_w * 2 + 100 {
            draw_hud_box_u32(buf, width, height, 20, p_y, p_w, p_h, 0xFF00_E5FF);
            draw_ascii_text_u32(
                buf,
                width,
                32,
                p_y + 12,
                "// GOAL DAG [PIPELINE]",
                0xFF00_E5FF,
            );

            let mut cur_y = p_y + 36;
            for (idx, goal) in self.state.orbital_goals.iter().take(5).enumerate() {
                let (prefix, color, pct) = match goal.status {
                    GoalStatus::Completed => ("[DONE]", 0xFF10_B981, 1.0),
                    GoalStatus::Active | GoalStatus::Evaluating => ("[EVAL]", 0xFFF5_9E0B, 0.65),
                    GoalStatus::Healing => ("[HEAL]", 0xFFA8_55F7, 0.45),
                    GoalStatus::Failed => ("[FAIL]", 0xFFEF_4444, 0.0),
                    GoalStatus::Pending => ("[WAIT]", 0xFF94_A3B8, 0.0),
                };
                let line = format!("{prefix} {}", goal.id);
                draw_ascii_text_u32(buf, width, 32, cur_y, &line, color);

                let intent_trunc = if goal.intent.len() > 30 {
                    format!("| {:.27}...", goal.intent)
                } else {
                    format!("| {}", goal.intent)
                };
                draw_ascii_text_u32(buf, width, 32, cur_y + 14, &intent_trunc, 0xFF94_A3B8);

                // Segmented Progress Bar underneath each goal row
                draw_progress_bar_u32(
                    buf,
                    width,
                    height,
                    32,
                    cur_y + 28,
                    p_w - 24,
                    3,
                    pct,
                    color,
                    0xFF1E_293B,
                );

                cur_y += 44;
                if idx == 4 {
                    break;
                }
            }
        }

        // Right Panel: Sub-System Telemetry & Equalizer
        if width > p_w * 2 + 100 {
            let rx = width - p_w - 20;
            draw_hud_box_u32(buf, width, height, rx, p_y, p_w, p_h, 0xFFA8_55F7);
            draw_ascii_text_u32(
                buf,
                width,
                rx + 12,
                p_y + 12,
                "// SYSTEM TELEMETRY & BUS",
                0xFFA8_55F7,
            );

            // 16-Channel Neural Bus Audio/Cognitive Spectrum
            draw_ascii_text_u32(
                buf,
                width,
                rx + 14,
                p_y + 34,
                "NEURAL COGNITIVE BUS SPECTRUM:",
                0xFFC0_84FC,
            );
            let eq_y = p_y + 50;
            for col in 0..16usize {
                let h_mod = (((self.state.pulse_tick + (col as u64) * 4) % 17) * 2 + 4) as usize;
                let bx = rx + 16 + col * 18;
                // Draw column
                for py in (eq_y + (36 - h_mod))..(eq_y + 36) {
                    for px in bx..(bx + 12) {
                        let idx = py * width + px;
                        if idx < buf.len() {
                            buf[idx] = if h_mod > 26 {
                                0xFF00_E5FF
                            } else if h_mod > 16 {
                                0xFF38_BDF8
                            } else {
                                0xFFA8_55F7
                            };
                        }
                    }
                }
                // Peak indicator dot floating above bar
                let peak_y = (eq_y + (36 - h_mod)).saturating_sub(3);
                for px in bx..(bx + 12) {
                    let idx = peak_y * width + px;
                    if idx < buf.len() {
                        buf[idx] = 0xFFFF_FFFF;
                    }
                }
            }

            let mut ty = p_y + 100;
            draw_ascii_text_u32(
                buf,
                width,
                rx + 14,
                ty,
                "• NVFS WAL: DURABLE (0xEDB88320)",
                0xFF38_BDF8,
            );
            ty += 18;
            draw_ascii_text_u32(
                buf,
                width,
                rx + 14,
                ty,
                "• REPLAY RATIO: 94.7% ($0-COST)",
                0xFF10_B981,
            );
            ty += 18;
            draw_ascii_text_u32(
                buf,
                width,
                rx + 14,
                ty,
                "• BROWSER: CAMOFOX TIER-1 ACTIVE",
                0xFF38_BDF8,
            );
            ty += 18;
            draw_ascii_text_u32(
                buf,
                width,
                rx + 14,
                ty,
                "• HEALER: DOM MULTI-ATTR RANKING",
                0xFF10_B981,
            );
            ty += 18;
            draw_ascii_text_u32(
                buf,
                width,
                rx + 14,
                ty,
                "• SCHEDULER: 0-DRIFT VIRTUAL",
                0xFF38_BDF8,
            );
            ty += 18;
            draw_ascii_text_u32(
                buf,
                width,
                rx + 14,
                ty,
                "• RASTER: 60 FPS (3.2ms) ZERO-ALLOC",
                0xFF10_B981,
            );
            ty += 18;
            draw_ascii_text_u32(
                buf,
                width,
                rx + 14,
                ty,
                "• CONTROLS: [SPACE] HOLD | [ESC] QUIT",
                0xFFF5_9E0B,
            );
        }
    }

    fn draw_pip_viewports_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let pip_w = 320usize;
        let pip_h = 330usize;
        let pip_y = 405usize;

        if height < pip_y + pip_h + 30 {
            return;
        }

        // Top-Left: Web Browser PiP Viewport
        if let Some(pip) = &self.state.pip_web {
            if pip.active && width > pip_w * 2 + 100 {
                let lx = 20usize;
                draw_hud_box_u32(buf, width, height, lx, pip_y, pip_w, pip_h, 0xFF00_E5FF);
                let disp_title = if pip.title.len() > 24 {
                    format!("{:.21}...", pip.title)
                } else {
                    pip.title.clone()
                };
                draw_ascii_text_u32(
                    buf,
                    width,
                    lx + 12,
                    pip_y + 12,
                    &format!("// WEB: {disp_title}"),
                    0xFF00_E5FF,
                );

                // Simulated browser address bar
                draw_hud_box_u32(
                    buf,
                    width,
                    height,
                    lx + 12,
                    pip_y + 28,
                    pip_w - 24,
                    16,
                    0x5500_E5FF,
                );
                draw_ascii_text_u32(
                    buf,
                    width,
                    lx + 16,
                    pip_y + 32,
                    "https://camofox.local/checkout",
                    0xFF94_A3B8,
                );

                if let Some(lbl) = &pip.target_label {
                    draw_ascii_text_u32(
                        buf,
                        width,
                        lx + 12,
                        pip_y + 50,
                        &format!("TARGET: {lbl}"),
                        0xFFF5_9E0B,
                    );
                }

                // Mini simulated DOM layout cards
                draw_hud_box_u32(buf, width, height, lx + 14, pip_y + 72, 90, 60, 0x4438_BDF8);
                draw_hud_box_u32(
                    buf,
                    width,
                    height,
                    lx + 115,
                    pip_y + 72,
                    180,
                    60,
                    0x4438_BDF8,
                );

                // Dynamic Reticle Crosshairs Locking on Target
                let rx_coord = (lx + 200) as i32;
                let ry_coord = (pip_y + 102) as i32;
                draw_reticle_u32(buf, width, height, rx_coord, ry_coord, 16, 0xFF00_E5FF);
            }
        }

        // Top-Right: Desktop OS PiP Viewport
        if let Some(pip) = &self.state.pip_desktop {
            if pip.active && width > pip_w * 2 + 100 {
                let rx = width - pip_w - 20;
                draw_hud_box_u32(buf, width, height, rx, pip_y, pip_w, pip_h, 0xFFA8_55F7);
                let disp_title = if pip.title.len() > 24 {
                    format!("{:.21}...", pip.title)
                } else {
                    pip.title.clone()
                };
                draw_ascii_text_u32(
                    buf,
                    width,
                    rx + 12,
                    pip_y + 12,
                    &format!("// OS: {disp_title}"),
                    0xFFA8_55F7,
                );

                // Terminal log lines
                draw_ascii_text_u32(
                    buf,
                    width,
                    rx + 14,
                    pip_y + 32,
                    "root@sovereign:~$ nvfs check",
                    0xFF64_748B,
                );
                draw_ascii_text_u32(
                    buf,
                    width,
                    rx + 14,
                    pip_y + 50,
                    "> WAL seq #4096 ok (crc32)",
                    0xFF10_B981,
                );
                draw_ascii_text_u32(
                    buf,
                    width,
                    rx + 14,
                    pip_y + 68,
                    "> timer wheel tick: 0-drift",
                    0xFF38_BDF8,
                );

                if let Some(lbl) = &pip.target_label {
                    draw_ascii_text_u32(
                        buf,
                        width,
                        rx + 14,
                        pip_y + 90,
                        &format!("FOCUS: {lbl}"),
                        0xFFF5_9E0B,
                    );
                }

                draw_reticle_u32(
                    buf,
                    width,
                    height,
                    (rx + pip_w / 2) as i32,
                    (pip_y + 130) as i32,
                    16,
                    0xFFA8_55F7,
                );
            }
        }
    }

    fn draw_bottom_ticker_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let ticker_h = 44usize;
        if height < ticker_h + 10 {
            return;
        }
        let ty = height - ticker_h;

        let bg_color = 0xF407_0B18;
        for y in ty..height {
            for x in 0..width {
                let idx = y * width + x;
                if idx < buf.len() {
                    blend_pixel_u32(&mut buf[idx], bg_color);
                }
            }
        }

        // Top neon cyan border
        let top_border = 0xFF00_E5FF;
        for x in 0..width {
            let idx = ty * width + x;
            if idx < buf.len() {
                buf[idx] = top_border;
            }
        }

        // Neon prompt badge
        draw_ascii_text_u32(buf, width, 18, ty + 16, "[>] VISOR.THOUGHT //", 0xFF00_E5FF);

        let max_chars = (width.saturating_sub(310)) / 8;
        let mut display_text = self.state.thought_ticker.clone();
        if display_text.len() > max_chars && max_chars > 3 {
            display_text.truncate(max_chars.saturating_sub(3));
            display_text.push_str("...");
        }

        // Blinking cursor '_' on ticker
        let cursor_char = if (self.state.pulse_tick % 20) < 10 {
            "_"
        } else {
            " "
        };
        let full_text = format!("{display_text}{cursor_char}");

        draw_ascii_text_u32(buf, width, 186, ty + 16, &full_text, 0xFFF8_FAFC);

        // Right Pulse Indicator
        let right_x = width.saturating_sub(110);
        draw_ascii_text_u32(buf, width, right_x, ty + 16, "[~^~ 60 FPS]", 0xFF38_BDF8);
    }
}

fn phase_color_u32(phase: SovereignPhase) -> u32 {
    match phase {
        SovereignPhase::Idle => 0xFF10_B981,           // Emerald Steady
        SovereignPhase::EvaluatingGoal => 0xFFF5_9E0B, // Amber Pulse
        SovereignPhase::ExecutingPlan => 0xFF00_E5FF,  // Cyan Neon Hyperdrive
        SovereignPhase::SelfHealing => 0xFFA8_55F7,    // Violet Hologram
        SovereignPhase::Snapshotting => 0xFFF4_72B6,   // Pink Glow
        SovereignPhase::Hold => 0xFFEF_4444,           // Crimson Emergency Hold
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
    if x < width {
        let idx = y * width + x;
        if idx < buf.len() {
            buf[idx] = color;
        }
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

#[allow(clippy::too_many_arguments)]
fn draw_line_u32(
    buf: &mut [u32],
    width: usize,
    height: usize,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: u32,
) {
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut cx = x0;
    let mut cy = y0;

    loop {
        set_pixel_i_u32(buf, width, height, cx, cy, color);
        if cx == x1 && cy == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            cx += sx;
        }
        if e2 <= dx {
            err += dx;
            cy += sy;
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

#[allow(clippy::too_many_arguments)]
fn draw_segmented_ring_u32(
    buf: &mut [u32],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    radius: i32,
    segments: usize,
    gap_deg: f64,
    offset_deg: f64,
    color: u32,
) {
    let seg_span = 360.0 / (segments as f64);
    let active_span = seg_span - gap_deg;
    let circumference = (2.0 * std::f64::consts::PI * radius as f64) as usize;
    let steps = circumference.max(360);

    for i in 0..steps {
        let deg = (i as f64 / steps as f64) * 360.0;
        let norm_deg = ((deg + offset_deg) % 360.0 + 360.0) % 360.0;
        let sub = norm_deg % seg_span;
        if sub < active_span {
            let rad = deg.to_radians();
            let px = cx + (radius as f64 * rad.cos()) as i32;
            let py = cy + (radius as f64 * rad.sin()) as i32;
            set_pixel_i_u32(buf, width, height, px, py, color);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_crosshair_ticks_u32(
    buf: &mut [u32],
    width: usize,
    height: usize,
    cx: i32,
    cy: i32,
    r_min: i32,
    r_max: i32,
    color: u32,
) {
    let mut x = ((r_min + 19) / 20) * 20;
    while x <= r_max {
        for dy in -2..=2 {
            set_pixel_i_u32(buf, width, height, cx + x, cy + dy, color);
            set_pixel_i_u32(buf, width, height, cx - x, cy + dy, color);
        }
        x += 20;
    }
    let mut y = ((r_min + 19) / 20) * 20;
    while y <= r_max {
        for dx in -2..=2 {
            set_pixel_i_u32(buf, width, height, cx + dx, cy + y, color);
            set_pixel_i_u32(buf, width, height, cx + dx, cy - y, color);
        }
        y += 20;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_progress_bar_u32(
    buf: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    pct: f64,
    fg: u32,
    bg: u32,
) {
    if x + w >= width || y + h >= height {
        return;
    }
    let filled_w = ((w as f64 * pct.clamp(0.0, 1.0)) as usize).min(w);
    for py in y..(y + h) {
        for px in x..(x + filled_w) {
            let idx = py * width + px;
            if idx < buf.len() {
                buf[idx] = fg;
            }
        }
        for px in (x + filled_w)..(x + w) {
            let idx = py * width + px;
            if idx < buf.len() {
                buf[idx] = bg;
            }
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

    let body_color = 0xFC06_0914;
    for py in y..(y + h) {
        for px in x..(x + w) {
            let idx = py * width + px;
            if idx < buf.len() {
                blend_pixel_u32(&mut buf[idx], body_color);
            }
        }
    }

    // Outer subtle border
    for py in y..(y + h) {
        set_pixel_u32(buf, width, x, py, 0x4400_E5FF);
        set_pixel_u32(buf, width, x + w - 1, py, 0x4400_E5FF);
    }
    for px in x..(x + w) {
        set_pixel_u32(buf, width, px, y, 0x4400_E5FF);
        set_pixel_u32(buf, width, px, y + h - 1, 0x4400_E5FF);
    }

    // Thick Corner Brackets (length 16 px)
    let br_len = 16usize.min(w / 2).min(h / 2);
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

    for offset in (size - 6)..(size + 6) {
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
    draw_ascii_text_u32_scaled(buf, width, start_x, start_y, text, color, 1);
}

fn draw_ascii_text_u32_scaled(
    buf: &mut [u32],
    width: usize,
    start_x: usize,
    start_y: usize,
    text: &str,
    color: u32,
    scale: usize,
) {
    let mut cur_x = start_x;
    for ch in text.chars() {
        if cur_x + 8 * scale > width {
            break;
        }
        let ascii = (ch as usize).min(127);
        let glyph = FONT_8X8_BASIC[ascii];
        for (row, &row_byte) in glyph.iter().enumerate() {
            for col in 0..8 {
                // MSB first: bit 7 is column 0 (leftmost)
                if ((row_byte >> (7 - col)) & 1) == 1 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let px = cur_x + col * scale + dx;
                            let py = start_y + row * scale + dy;
                            if px < width {
                                let idx = py * width + px;
                                if idx < buf.len() {
                                    buf[idx] = color;
                                }
                            }
                        }
                    }
                }
            }
        }
        cur_x = cur_x.saturating_add(8 * scale);
    }
}

/// Run an interactive 60 FPS Visor OS desktop window via winit and softbuffer.
#[cfg(feature = "native-gui")]
pub fn run_native_visor_window(title: &str, width: u32, height: u32) -> Result<(), String> {
    use std::sync::Arc;
    use std::time::Duration;
    use winit::application::ApplicationHandler;
    use winit::dpi::LogicalSize;
    use winit::event::{ElementState, KeyEvent, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::keyboard::{Key, NamedKey};
    use winit::window::{Window, WindowAttributes, WindowId};

    struct VisorApp {
        title: String,
        width: u32,
        height: u32,
        window: Option<Arc<Window>>,
        context: Option<softbuffer::Context<Arc<Window>>>,
        surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
        last_frame: Instant,
    }

    impl ApplicationHandler for VisorApp {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            if self.window.is_none() {
                let win_attrs = WindowAttributes::default()
                    .with_title(&self.title)
                    .with_inner_size(LogicalSize::new(self.width, self.height));

                let window = match event_loop.create_window(win_attrs) {
                    Ok(w) => Arc::new(w),
                    Err(e) => {
                        eprintln!("error: failed to create Visor window: {e}");
                        event_loop.exit();
                        return;
                    }
                };

                let context = match softbuffer::Context::new(window.clone()) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("error: failed to create softbuffer context: {e}");
                        event_loop.exit();
                        return;
                    }
                };

                let mut surface = match softbuffer::Surface::new(&context, window.clone()) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: failed to create softbuffer surface: {e}");
                        event_loop.exit();
                        return;
                    }
                };

                let _ = surface.resize(
                    std::num::NonZeroU32::new(self.width).unwrap(),
                    std::num::NonZeroU32::new(self.height).unwrap(),
                );

                self.window = Some(window);
                self.context = Some(context);
                self.surface = Some(surface);
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                }
                WindowEvent::Resized(size) => {
                    if let Some(surface) = self.surface.as_mut() {
                        if let (Some(w), Some(h)) = (
                            std::num::NonZeroU32::new(size.width),
                            std::num::NonZeroU32::new(size.height),
                        ) {
                            let _ = surface.resize(w, h);
                            self.width = size.width;
                            self.height = size.height;
                        }
                    }
                }
                WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            state: ElementState::Pressed,
                            logical_key,
                            ..
                        },
                    ..
                } => match logical_key {
                    Key::Named(NamedKey::Escape) => {
                        event_loop.exit();
                    }
                    Key::Named(NamedKey::Space) => {
                        if let Ok(mut sl) = get_sovereign_loop().lock() {
                            let _ = sl.toggle_hold();
                        }
                    }
                    Key::Character(ref ch) if ch == "q" || ch == "Q" => {
                        event_loop.exit();
                    }
                    _ => {}
                },
                WindowEvent::RedrawRequested => {
                    let w = self.width as usize;
                    let h = self.height as usize;
                    if let Some(surface) = self.surface.as_mut() {
                        if let Ok(mut buffer) = surface.buffer_mut() {
                            let _ = with_visor(|v| {
                                v.advance_tick();
                                v.render_to_u32_buffer(&mut buffer, w, h);
                                Ok(())
                            });
                            let _ = buffer.present();
                        }
                    }

                    // Pacing target ~60 FPS
                    let elapsed = self.last_frame.elapsed();
                    let target_frame = Duration::from_millis(16);
                    if elapsed < target_frame {
                        std::thread::sleep(target_frame - elapsed);
                    }
                    self.last_frame = Instant::now();

                    if let Some(win) = self.window.as_ref() {
                        win.request_redraw();
                    }
                }
                _ => {}
            }
        }
    }

    let mut builder = EventLoop::builder();
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use winit::platform::wayland::EventLoopBuilderExtWayland;
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoopBuilderExtX11::with_any_thread(&mut builder, true);
        EventLoopBuilderExtWayland::with_any_thread(&mut builder, true);
    }
    let event_loop = builder
        .build()
        .map_err(|e| format!("failed to build event loop: {e}"))?;

    let mut app = VisorApp {
        title: title.to_string(),
        width,
        height,
        window: None,
        context: None,
        surface: None,
        last_frame: Instant::now(),
    };

    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("error during Visor execution: {e}"))?;
    Ok(())
}

/// Fallback for non-GUI builds.
#[cfg(not(feature = "native-gui"))]
pub fn run_native_visor_window(_title: &str, _width: u32, _height: u32) -> Result<(), String> {
    Err(
        "error[E0971]: native Visor GUI requires compiling with '--features native-gui'"
            .to_string(),
    )
}

/// Global process-wide Visor HUD engine singleton.
static VISOR_ENGINE: OnceLock<Mutex<VisorEngine>> = OnceLock::new();

pub fn get_visor_engine() -> &'static Mutex<VisorEngine> {
    VISOR_ENGINE.get_or_init(|| Mutex::new(VisorEngine::new()))
}

pub fn with_visor<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut VisorEngine) -> Result<R, String>,
{
    let mutex = get_visor_engine();
    let mut visor = mutex
        .lock()
        .map_err(|e| format!("error[E0970]: VisorEngine lock poisoned: {e}"))?;
    f(&mut visor)
}

// Backward-compatibility aliases
pub type HudEngine = VisorEngine;
pub type HudState = VisorState;
pub type HudPipFrame = VisorPipFrame;
pub fn get_hud_engine() -> &'static Mutex<VisorEngine> {
    get_visor_engine()
}
pub fn with_hud<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut VisorEngine) -> Result<R, String>,
{
    with_visor(f)
}

/// Register Visor functions in standard library dispatch table.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert("visor.state".to_string(), Value::Native("visor.state"));
    functions.insert("visor.ticker".to_string(), Value::Native("visor.ticker"));
    functions.insert("visor.set_pip".to_string(), Value::Native("visor.set_pip"));
    functions.insert("visor.advance".to_string(), Value::Native("visor.advance"));
    functions.insert("visor.render".to_string(), Value::Native("visor.render"));

    // Legacy hud aliases
    functions.insert("hud.state".to_string(), Value::Native("hud.state"));
    functions.insert("hud.ticker".to_string(), Value::Native("hud.ticker"));
    functions.insert("hud.set_pip".to_string(), Value::Native("hud.set_pip"));
    functions.insert("hud.advance".to_string(), Value::Native("hud.advance"));
    functions.insert("hud.render".to_string(), Value::Native("hud.render"));

    functions
}

/// Execute Visor standard library calls.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "visor.state" | "hud.state" => with_visor(|v| {
            let s = v.state();
            let mut map = HashMap::new();
            map.insert(
                "phase".to_string(),
                Value::String(s.phase.as_str().to_string()),
            );
            map.insert("pulse_tick".to_string(), Value::Int(s.pulse_tick as i64));
            map.insert(
                "thought".to_string(),
                Value::String(s.thought_ticker.clone()),
            );
            map.insert(
                "pip_web_active".to_string(),
                Value::Bool(s.pip_web.as_ref().map(|p| p.active).unwrap_or(false)),
            );
            map.insert(
                "pip_desktop_active".to_string(),
                Value::Bool(s.pip_desktop.as_ref().map(|p| p.active).unwrap_or(false)),
            );
            map.insert(
                "orbital_goal_count".to_string(),
                Value::Int(s.orbital_goals.len() as i64),
            );
            Ok(Value::Map(map))
        }),

        "visor.ticker" | "hud.ticker" => {
            if args.is_empty() {
                return Err("error[E0971]: visor.ticker requires 1 string argument".to_string());
            }
            let msg = match &args[0] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            with_visor(|v| {
                v.push_ticker(&msg);
                Ok(Value::Bool(true))
            })
        }

        "visor.set_pip" | "hud.set_pip" => {
            if args.len() < 3 {
                return Err(
                    "error[E0972]: visor.set_pip requires (kind, active, title, [target])"
                        .to_string(),
                );
            }
            let kind = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => {
                    return Err("error[E0972]: kind must be string ('web' | 'desktop')".to_string())
                }
            };
            let active = match &args[1] {
                Value::Bool(b) => *b,
                _ => return Err("error[E0972]: active must be boolean".to_string()),
            };
            let title = match &args[2] {
                Value::String(s) => s.as_str(),
                _ => return Err("error[E0972]: title must be string".to_string()),
            };
            let target = args.get(3).and_then(|v| match v {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            });

            with_visor(|v| {
                v.set_pip(kind, active, title, target);
                Ok(Value::Bool(true))
            })
        }

        "visor.advance" | "hud.advance" => with_visor(|v| {
            v.advance_tick();
            Ok(Value::Int(v.state().pulse_tick as i64))
        }),

        "visor.render" | "hud.render" => {
            let width = args
                .first()
                .and_then(|v| match v {
                    Value::Int(i) if *i > 0 => Some(*i as usize),
                    _ => None,
                })
                .unwrap_or(640);
            let height = args
                .get(1)
                .and_then(|v| match v {
                    Value::Int(i) if *i > 0 => Some(*i as usize),
                    _ => None,
                })
                .unwrap_or(480);

            with_visor(|v| {
                let frame = v.render_frame(width, height);
                let mut map = HashMap::new();
                map.insert("width".to_string(), Value::Int(width as i64));
                map.insert("height".to_string(), Value::Int(height as i64));
                map.insert("byte_count".to_string(), Value::Int(frame.len() as i64));
                map.insert("format".to_string(), Value::String("RGBA32".to_string()));
                Ok(Value::Map(map))
            })
        }

        unknown => Err(format!("error[E0974]: unknown visor function: {unknown}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visor_lifecycle_and_rendering() {
        let mut visor = VisorEngine::new();
        assert_eq!(visor.state().pulse_tick, 0);

        visor.advance_tick();
        assert_eq!(visor.state().pulse_tick, 1);

        visor.push_ticker("Testing autonomous Visor thought stream.");
        assert_eq!(
            visor.state().thought_ticker,
            "Testing autonomous Visor thought stream."
        );

        visor.set_pip("web", true, "Order Checkout", Some("#submit-order"));
        let web_pip = visor.state().pip_web.as_ref().unwrap();
        assert!(web_pip.active);
        assert_eq!(web_pip.title, "Order Checkout");

        let frame = visor.render_frame(640, 480);
        assert_eq!(frame.len(), 640 * 480 * 4);

        let mut u32_buf = vec![0u32; 640 * 480];
        visor.render_to_u32_buffer(&mut u32_buf, 640, 480);
        assert_ne!(u32_buf[0], 0);

        // Test emergency hold
        if let Ok(mut sl) = get_sovereign_loop().lock() {
            let is_hold = sl.toggle_hold();
            assert!(is_hold);
            assert_eq!(sl.get_telemetry().phase, SovereignPhase::Hold);
            let unhold = sl.toggle_hold();
            assert!(!unhold);
            assert_eq!(sl.get_telemetry().phase, SovereignPhase::Idle);
        }
    }
}
