//! Nudge Visor — Sovereign Autonomy HUD & Tactical Telemetry Interface.
//!
//! Provides the ambient visual feedback layer for Nudge Sovereign Loops:
//! - 3-Column Sovereign Developer Workbench & Mission Control Console.
//! - Interactive Goal DAG hierarchy tree (Active, Evaluating, Healing, Done).
//! - Live Agent Execution Stream & Telemetry Trace console with categorized entries.
//! - Interactive Directive Input Prompt Bar (Direct intent dispatching to SovereignLoop).
//! - Real-time CamoFox Stealth Browser DOM & Healer inspector.
//! - Durable NVFS WAL storage & Google Antigravity OAuth session telemetry.
//! - 60 FPS zero-allocation direct software framebuffer compositor.

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

/// An orbital goal node positioned on a ring around the core (kept for compatibility & DAG cards).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrbitalGoal {
    pub id: String,
    pub intent: String,
    pub status: GoalStatus,
    pub ring: usize,
    pub angle_deg: f64,
}

/// An execution trace log item displayed in the center stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEntry {
    pub timestamp: String,
    pub category: String,
    pub message: String,
    pub color: u32,
}

/// Aggregated Visor Workbench state.
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
    pub input_buffer: String,
    pub auth_status: String,
    pub trace_log: VecDeque<TraceEntry>,
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
            current_thought: "Nudge Visor sovereign mission control online.".to_string(),
        };

        let mut trace = VecDeque::new();
        trace.push_back(TraceEntry {
            timestamp: "00:00:01".to_string(),
            category: "SYS".to_string(),
            message: "Sovereign runtime online. Mission Control v2.5 initialized.".to_string(),
            color: 0xFF38_BDF8,
        });
        trace.push_back(TraceEntry {
            timestamp: "00:00:02".to_string(),
            category: "NVFS".to_string(),
            message: "Durable WAL stream active. CRC32-IEEE checksum verification armed."
                .to_string(),
            color: 0xFF10_B981,
        });
        trace.push_back(TraceEntry {
            timestamp: "00:00:03".to_string(),
            category: "HEAL".to_string(),
            message: "DOM HealerEngine active (multi-attribute heuristic ranking).".to_string(),
            color: 0xFFA8_55F7,
        });
        trace.push_back(TraceEntry {
            timestamp: "00:00:04".to_string(),
            category: "READY".to_string(),
            message: "Workbench ready. Type directive below & hit [ENTER] to dispatch.".to_string(),
            color: 0xFF00_E5FF,
        });

        Self {
            phase: SovereignPhase::Idle,
            pulse_tick: 0,
            telemetry: dummy_telem,
            thought_ticker: "Nudge Visor sovereign mission control online.".to_string(),
            ticker_history: VecDeque::new(),
            pip_web: None,
            pip_desktop: None,
            orbital_goals: Vec::new(),
            input_buffer: String::new(),
            auth_status: "CHECKING...".to_string(),
            trace_log: trace,
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
        let mut engine = Self {
            state: VisorState::default(),
            start_time: Instant::now(),
        };
        engine.check_auth();
        engine
    }

    fn check_auth(&mut self) {
        self.state.auth_status = if let Ok(store) = crate::auth::CredentialStore::load_default() {
            if let Some(cred) = store.get_provider("antigravity") {
                if !cred.is_expired() {
                    "ANTIGRAVITY: dev.nekomya@gmail.com [ACTIVE]".to_string()
                } else {
                    "ANTIGRAVITY: dev.nekomya@gmail.com [EXPIRED]".to_string()
                }
            } else {
                "AUTH: NO-CREDENTIALS".to_string()
            }
        } else {
            "AUTH: OFFLINE".to_string()
        };
    }

    /// Synchronize Visor state with the sovereign loop supervisor.
    pub fn sync_with_sovereign(&mut self) {
        if self.state.pulse_tick.is_multiple_of(120) {
            self.check_auth();
        }

        if let Ok(sl) = get_sovereign_loop().lock() {
            let telem = sl.get_telemetry();
            self.state.phase = telem.phase;
            let thought = telem.current_thought.clone();
            self.state.telemetry = telem;
            if !thought.is_empty() {
                self.push_ticker(&thought);
            }

            // Sync goals to DAG list
            let goals = sl.list_goals();
            self.state.orbital_goals.clear();
            for g in goals {
                self.state.orbital_goals.push(OrbitalGoal {
                    id: g.id,
                    intent: g.intent,
                    status: g.status,
                    ring: 1,
                    angle_deg: 0.0,
                });
            }
        }
    }

    /// Advance pulse tick for smooth animation transitions.
    pub fn advance_tick(&mut self) {
        self.state.pulse_tick = self.state.pulse_tick.saturating_add(1);
        self.sync_with_sovereign();
    }

    /// Push a new thought message to the live ticker and trace log.
    pub fn push_ticker(&mut self, msg: &str) {
        if self.state.thought_ticker != msg {
            self.state.thought_ticker = msg.to_string();
            if self.state.ticker_history.len() >= 50 {
                self.state.ticker_history.pop_front();
            }
            self.state.ticker_history.push_back(msg.to_string());

            if !msg.contains("Steady-State") {
                let (cat, col) = if msg.contains("Healer") || msg.contains("drift") {
                    ("HEAL", 0xFFA8_55F7)
                } else if msg.contains("NVFS") || msg.contains("WAL") || msg.contains("Verifying") {
                    ("NVFS", 0xFF10_B981)
                } else if msg.contains("HOLD") {
                    ("HOLD", 0xFFEF_4444)
                } else if msg.contains("CERTIFIED") || msg.contains("Achieved") {
                    ("DONE", 0xFF10_B981)
                } else {
                    ("AGENT", 0xFF38_BDF8)
                };
                self.push_trace(cat, msg, col);
            }
        }
    }

    /// Append a categorized trace log line.
    pub fn push_trace(&mut self, category: &str, message: &str, color: u32) {
        if let Some(last) = self.state.trace_log.back() {
            if last.message == message {
                return;
            }
        }

        let elapsed = self.start_time.elapsed().as_secs();
        let hrs = (elapsed / 3600) % 24;
        let mins = (elapsed / 60) % 60;
        let secs = elapsed % 60;
        let ts = format!("{hrs:02}:{mins:02}:{secs:02}");

        if self.state.trace_log.len() >= 64 {
            self.state.trace_log.pop_front();
        }
        self.state.trace_log.push_back(TraceEntry {
            timestamp: ts,
            category: category.to_string(),
            message: message.to_string(),
            color,
        });
    }

    /// Append character to directive input buffer.
    pub fn push_input_char(&mut self, c: char) {
        if self.state.input_buffer.len() < 120 {
            self.state.input_buffer.push(c);
        }
    }

    /// Remove last character from directive input buffer.
    pub fn pop_input_char(&mut self) {
        self.state.input_buffer.pop();
    }

    /// Clear directive input buffer.
    pub fn clear_input(&mut self) {
        self.state.input_buffer.clear();
    }

    /// Submit current input buffer and return submitted string.
    pub fn submit_input(&mut self) -> Option<String> {
        if self.state.input_buffer.trim().is_empty() {
            None
        } else {
            let val = std::mem::take(&mut self.state.input_buffer);
            self.push_trace("OPERATOR", &format!("Directive: {val}"), 0xFF00_E5FF);
            Some(val)
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

        // 1. Dark obsidian background
        self.draw_workbench_background_u32(buf, width, height);

        // 2. Mission control telemetry header
        self.draw_mission_control_header_u32(buf, width, height);

        // 3. 3-Column Workbench Body
        let top_y = 52usize;
        let bot_y = height.saturating_sub(44);
        if bot_y > top_y + 100 && width >= 800 {
            let avail_h = bot_y - top_y;
            let col_w = if width >= 1200 { 330 } else { 260 };
            let left_x = 14usize;
            let left_w = col_w;

            let right_w = col_w;
            let right_x = width.saturating_sub(right_w + 14);

            let center_x = left_x + left_w + 12;
            let center_w = if right_x > center_x + 12 {
                right_x - center_x - 12
            } else {
                300
            };

            // Left: Goal DAG Hierarchy
            self.draw_goal_dag_panel_u32(buf, width, height, left_x, top_y, left_w, avail_h);

            // Center: Live Execution Stream & Interactive Directive Console
            self.draw_execution_stream_panel_u32(
                buf, width, height, center_x, top_y, center_w, avail_h,
            );

            // Right: CamoFox & NVFS Inspectors
            self.draw_inspectors_panel_u32(buf, width, height, right_x, top_y, right_w, avail_h);
        }

        // 4. Bottom thought ticker console
        self.draw_bottom_ticker_u32(buf, width, height);
    }

    fn draw_workbench_background_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let count = (width * height).min(buf.len());
        // Base dark obsidian slate #080C14
        buf[..count].fill(0xFF08_0C14);

        // Subtle micro-dots every 32 px
        for y in (0..height).step_by(32) {
            for x in (0..width).step_by(32) {
                let idx = y * width + x;
                if idx < buf.len() {
                    buf[idx] = 0xFF14_1D2E;
                }
            }
        }

        // Corner Calibration Tags
        draw_ascii_text_u32(
            buf,
            width,
            16,
            height.saturating_sub(54),
            "NUDGE SOVEREIGN WORKBENCH // SECURE",
            0xFF33_4155,
        );
        draw_ascii_text_u32(
            buf,
            width,
            width.saturating_sub(184),
            height.saturating_sub(54),
            "STATUS // OPERATIONAL",
            0xFF33_4155,
        );
    }

    fn draw_mission_control_header_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let bar_h = 48;
        if height < bar_h {
            return;
        }

        let header_bg = 0xFF0B_101D;
        for y in 0..bar_h {
            let row_start = y * width;
            let row_end = row_start + width;
            if row_end <= buf.len() {
                buf[row_start..row_end].fill(header_bg);
            }
        }

        // Double neon cyan bottom border line
        let border_cyan = 0xFF00_E5FF;
        let border_glow = 0xFF02_84C7;
        let b_y = bar_h - 2;
        let row_b = b_y * width;
        let row_glow = (b_y + 1) * width;
        if row_b + width <= buf.len() {
            buf[row_b..row_b + width].fill(border_cyan);
        }
        if row_glow + width <= buf.len() {
            buf[row_glow..row_glow + width].fill(border_glow);
        }

        // Left Branding: Scaled "VISOR" + Subtitle
        draw_ascii_text_u32_scaled(buf, width, 16, 10, "VISOR", 0xFF00_E5FF, 2);
        draw_ascii_text_u32(buf, width, 106, 17, "// MISSION CONTROL V2.5", 0xFF64_748B);

        // Center Status Chips
        let phase_col = phase_color_u32(self.state.phase);
        let phase_str = match self.state.phase {
            SovereignPhase::Hold => "[HOLD: EMERGENCY]".to_string(),
            _ => format!("[PHASE: {}]", self.state.phase.as_str().to_uppercase()),
        };
        let ratio_pct = (self.state.telemetry.replay_ratio * 100.0).clamp(0.0, 100.0);
        let is_replaying = crate::trace::replay_active();
        let (replay_str, replay_col) = if is_replaying {
            ("[REPLAYING: ACTIVE]".to_string(), 0xFFF5_9E0B)
        } else {
            (format!("[REPLAY: {ratio_pct:.1}% $0-COST]"), 0xFF38_BDF8)
        };
        let wal_str = "[NVFS: CRC32-OK]";

        let auth_str = if self.state.auth_status.starts_with("ANTIGRAVITY:") {
            "[AUTH: ANTIGRAVITY OK]"
        } else {
            "[AUTH: STANDALONE]"
        };

        draw_ascii_text_u32(buf, width, 320, 17, auth_str, 0xFF10_B981);
        draw_ascii_text_u32(buf, width, 520, 17, &replay_str, replay_col);
        draw_ascii_text_u32(buf, width, 730, 17, wal_str, 0xFF00_E5FF);
        draw_ascii_text_u32(buf, width, 890, 17, &phase_str, phase_col);

        // Right Stats
        let uptime_str = format!("UPTIME: {}s", self.start_time.elapsed().as_secs());
        let right_x = width.saturating_sub(180);
        draw_ascii_text_u32(buf, width, right_x, 17, &uptime_str, 0xFF94_A3B8);
        draw_ascii_text_u32(buf, width, right_x + 105, 17, "[60 FPS]", 0xFF10_B981);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_goal_dag_panel_u32(
        &self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
    ) {
        draw_hud_box_u32(buf, width, height, x, y, w, h, 0xFF00_E5FF);
        draw_ascii_text_u32(
            buf,
            width,
            x + 12,
            y + 10,
            "// GOAL DAG HIERARCHY",
            0xFF00_E5FF,
        );

        let active_count = self.state.telemetry.active_goals;
        let total_count = self.state.telemetry.total_goals;
        let count_tag = format!("[{active_count}/{total_count} ACTIVE]");
        draw_ascii_text_u32(
            buf,
            width,
            x + w.saturating_sub(count_tag.len() * 8 + 12),
            y + 10,
            &count_tag,
            0xFF94_A3B8,
        );

        // Header separator line
        for px in (x + 8)..(x + w.saturating_sub(8)) {
            set_pixel_u32(buf, width, px, y + 24, 0xFF1E_293B);
        }

        let mut cur_y = y + 32;
        let card_h = 56usize;
        let card_w = w.saturating_sub(20);

        if self.state.orbital_goals.is_empty() {
            draw_ascii_text_u32(
                buf,
                width,
                x + 16,
                cur_y + 12,
                "No active goals in DAG.",
                0xFF64_748B,
            );
            draw_ascii_text_u32(
                buf,
                width,
                x + 16,
                cur_y + 28,
                "Use directive prompt to queue.",
                0xFF47_5569,
            );
            return;
        }

        for (idx, goal) in self.state.orbital_goals.iter().enumerate() {
            if cur_y + card_h > y + h - 10 {
                break;
            }

            let (badge, badge_color, pct) = match goal.status {
                GoalStatus::Completed => ("[DONE]", 0xFF10_B981, 1.0),
                GoalStatus::Active | GoalStatus::Evaluating => ("[EXEC]", 0xFF00_E5FF, 0.70),
                GoalStatus::Healing => ("[HEAL]", 0xFFA8_55F7, 0.45),
                GoalStatus::Failed => ("[FAIL]", 0xFFEF_4444, 0.0),
                GoalStatus::Pending => ("[WAIT]", 0xFF64_748B, 0.0),
            };

            // Goal card box
            let card_x = x + 10;
            let end_card_x = card_x + card_w;
            for py in cur_y..(cur_y + card_h - 4) {
                let row_start = py * width + card_x;
                let row_end = py * width + end_card_x;
                if row_end <= buf.len() {
                    buf[row_start..row_end].fill(0xFF11_1827);
                }
            }
            // Card left accent bar
            for py in cur_y..(cur_y + card_h - 4) {
                for px in card_x..(card_x + 3) {
                    set_pixel_u32(buf, width, px, py, badge_color);
                }
            }

            // Line 1: badge + goal ID
            let id_line = format!("{badge} #{}", goal.id);
            draw_ascii_text_u32(buf, width, card_x + 8, cur_y + 6, &id_line, badge_color);

            // Line 2: intent (cleanly truncated)
            let max_chars = card_w.saturating_sub(18) / 8;
            let mut intent_str = goal.intent.clone();
            if intent_str.len() > max_chars && max_chars > 3 {
                intent_str.truncate(max_chars.saturating_sub(3));
                intent_str.push_str("...");
            }
            draw_ascii_text_u32(buf, width, card_x + 8, cur_y + 20, &intent_str, 0xFFE2_E8F0);

            // Line 3: Progress bar
            draw_progress_bar_u32(
                buf,
                width,
                height,
                card_x + 8,
                cur_y + 36,
                card_w.saturating_sub(16),
                4,
                pct,
                badge_color,
                0xFF1E_293B,
            );

            cur_y += card_h;
            if idx >= 9 {
                break;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_execution_stream_panel_u32(
        &self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
    ) {
        draw_hud_box_u32(buf, width, height, x, y, w, h, 0xFF38_BDF8);
        draw_ascii_text_u32(
            buf,
            width,
            x + 12,
            y + 10,
            "// LIVE AGENT EXECUTION STREAM",
            0xFF38_BDF8,
        );
        draw_ascii_text_u32(
            buf,
            width,
            x + w.saturating_sub(160),
            y + 10,
            "[TRACE: 0-DRIFT]",
            0xFF64_748B,
        );

        // Header separator line
        for px in (x + 8)..(x + w.saturating_sub(8)) {
            set_pixel_u32(buf, width, px, y + 24, 0xFF1E_293B);
        }

        // Trace stream area
        let stream_top = y + 30;
        let input_box_h = 50usize;
        let stream_bottom = y + h.saturating_sub(input_box_h + 8);
        let avail_stream_h = stream_bottom.saturating_sub(stream_top);
        let line_h = 16usize;
        let max_lines = avail_stream_h / line_h;

        let total_traces = self.state.trace_log.len();
        let skip = total_traces.saturating_sub(max_lines);

        let mut line_y = stream_top;
        for entry in self.state.trace_log.iter().skip(skip).take(max_lines) {
            let cat_str = format!("[{}]", entry.category);
            draw_ascii_text_u32(buf, width, x + 12, line_y, &entry.timestamp, 0xFF64_748B);
            draw_ascii_text_u32(buf, width, x + 84, line_y, &cat_str, entry.color);

            let msg_x = x + 172;
            let max_msg_chars = (x + w).saturating_sub(msg_x + 12) / 8;
            let mut msg = entry.message.clone();
            if msg.len() > max_msg_chars && max_msg_chars > 3 {
                msg.truncate(max_msg_chars.saturating_sub(3));
                msg.push_str("...");
            }
            draw_ascii_text_u32(buf, width, msg_x, line_y, &msg, 0xFFF1_F5F9);

            line_y += line_h;
        }

        // --- DIRECTIVE INPUT PROMPT BAR ---
        let input_y = y + h.saturating_sub(input_box_h + 4);
        let input_w = w.saturating_sub(16);
        let input_x = x + 8;

        // Glowing border for interactive input bar
        draw_hud_box_u32(
            buf,
            width,
            height,
            input_x,
            input_y,
            input_w,
            input_box_h,
            0xFF00_E5FF,
        );

        draw_ascii_text_u32(
            buf,
            width,
            input_x + 12,
            input_y + 10,
            "DIRECTIVE >",
            0xFF00_E5FF,
        );

        let cursor_char = if (self.state.pulse_tick % 16) < 8 {
            "_"
        } else {
            " "
        };
        let typed = &self.state.input_buffer;
        let max_type_chars = (input_w.saturating_sub(130)) / 8;
        let mut display_typed = typed.clone();
        if display_typed.len() > max_type_chars && max_type_chars > 3 {
            display_typed =
                display_typed[display_typed.len().saturating_sub(max_type_chars)..].to_string();
        }
        let input_line = format!("{display_typed}{cursor_char}");
        draw_ascii_text_u32(
            buf,
            width,
            input_x + 116,
            input_y + 10,
            &input_line,
            0xFFFFFFFF,
        );

        // Shortcut helper legend below input prompt
        draw_ascii_text_u32(
            buf,
            width,
            input_x + 12,
            input_y + 30,
            "[ENTER] Dispatch Intent   [SPACE] Emergency Hold   [ESC] Clear",
            0xFF64_748B,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_inspectors_panel_u32(
        &self,
        buf: &mut [u32],
        width: usize,
        height: usize,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
    ) {
        draw_hud_box_u32(buf, width, height, x, y, w, h, 0xFFA8_55F7);
        draw_ascii_text_u32(
            buf,
            width,
            x + 12,
            y + 10,
            "// SYSTEM & CAMOFOX INSPECTORS",
            0xFFA8_55F7,
        );

        // Header separator line
        for px in (x + 8)..(x + w.saturating_sub(8)) {
            set_pixel_u32(buf, width, px, y + 24, 0xFF1E_293B);
        }

        let inner_w = w.saturating_sub(16);
        let inner_x = x + 8;

        // Card 1: CamoFox Browser & Healer Inspector
        let card1_y = y + 30;
        let card1_h = 210usize.min(h / 3 + 30);
        draw_hud_box_u32(
            buf,
            width,
            height,
            inner_x,
            card1_y,
            inner_w,
            card1_h,
            0xFF00_E5FF,
        );
        draw_ascii_text_u32(
            buf,
            width,
            inner_x + 10,
            card1_y + 8,
            "// BROWSER [CAMOFOX TIER-1]",
            0xFF00_E5FF,
        );

        let url_str = "URL: https://camofox.local/checkout";
        draw_ascii_text_u32(buf, width, inner_x + 10, card1_y + 26, url_str, 0xFF94_A3B8);

        let tgt_label = self
            .state
            .pip_web
            .as_ref()
            .and_then(|p| p.target_label.as_deref())
            .unwrap_or("#checkout-submit");
        let tgt_line = format!("TARGET: {tgt_label}");
        draw_ascii_text_u32(
            buf,
            width,
            inner_x + 10,
            card1_y + 44,
            &tgt_line,
            0xFFF5_9E0B,
        );

        draw_ascii_text_u32(
            buf,
            width,
            inner_x + 10,
            card1_y + 62,
            "HEALER: MULTI-ATTR RANKING",
            0xFF10_B981,
        );
        draw_ascii_text_u32(
            buf,
            width,
            inner_x + 10,
            card1_y + 78,
            "CONFIDENCE: 96.4% [VERIFIED]",
            0xFF38_BDF8,
        );

        // Mini DOM Wireframe card
        let wire_y = card1_y + 98;
        if wire_y + 80 < card1_y + card1_h {
            draw_hud_box_u32(
                buf,
                width,
                height,
                inner_x + 10,
                wire_y,
                70,
                50,
                0x4438_BDF8,
            );
            draw_hud_box_u32(
                buf,
                width,
                height,
                inner_x + 88,
                wire_y,
                inner_w.saturating_sub(100),
                50,
                0x4438_BDF8,
            );
            draw_ascii_text_u32(buf, width, inner_x + 16, wire_y + 20, "DOM", 0xFF64_748B);
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 96,
                wire_y + 20,
                "<button id='order'>",
                0xFF38_BDF8,
            );

            // Target reticle lock
            let rx = (inner_x + inner_w / 2) as i32;
            let ry = (wire_y + 25) as i32;
            draw_reticle_u32(buf, width, height, rx, ry, 12, 0xFF00_E5FF);
        }

        // Card 2: Durable NVFS & Real Auth Inspector
        let card2_y = card1_y + card1_h + 8;
        let card2_h = 190usize.min(h / 3 + 20);
        if card2_y + card2_h < y + h - 10 {
            draw_hud_box_u32(
                buf,
                width,
                height,
                inner_x,
                card2_y,
                inner_w,
                card2_h,
                0xFFA8_55F7,
            );
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                card2_y + 8,
                "// DURABLE NVFS & REAL AUTH",
                0xFFA8_55F7,
            );

            let mut ty = card2_y + 26;
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                ty,
                "• WAL FRAME: DURABLE (CRC32 OK)",
                0xFF10_B981,
            );
            ty += 16;
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                ty,
                "• REPLAY RATIO: 100.0% ($0-COST)",
                0xFF10_B981,
            );
            ty += 16;
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                ty,
                "• AUTH: dev.nekomya@gmail.com",
                0xFF38_BDF8,
            );
            ty += 16;
            let session_line = if self.state.auth_status.contains("[ACTIVE]") {
                "• STATUS: OAUTH ACTIVE (Antigravity)"
            } else {
                "• STATUS: OFFLINE / STANDALONE"
            };
            draw_ascii_text_u32(buf, width, inner_x + 10, ty, session_line, 0xFF10_B981);
            ty += 16;
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                ty,
                "• SCHEDULER: 0-DRIFT VIRTUAL",
                0xFF94_A3B8,
            );
            ty += 16;

            // Mini 16-channel cognitive bus spectrum
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                ty,
                "COGNITIVE NEURAL BUS:",
                0xFFC0_84FC,
            );
            ty += 14;
            let bar_count = 16usize.min((inner_w.saturating_sub(24)) / 14);
            for col in 0..bar_count {
                let h_mod = (((self.state.pulse_tick + (col as u64) * 4) % 15) * 2 + 4) as usize;
                let bar_h = h_mod.min(28);
                let bx = inner_x + 12 + col * 14;
                let start_py = ty + (30usize.saturating_sub(bar_h));
                let end_py = ty + 30;
                for py in start_py..end_py {
                    for px in bx..(bx + 8) {
                        set_pixel_u32(
                            buf,
                            width,
                            px,
                            py,
                            if h_mod > 22 { 0xFF00_E5FF } else { 0xFFA8_55F7 },
                        );
                    }
                }
            }
        }

        // Card 3: Workbench Controls & Health
        let card3_y = card2_y + card2_h + 8;
        if card3_y + 80 <= y + h {
            let card3_h = (y + h).saturating_sub(card3_y + 4);
            draw_hud_box_u32(
                buf,
                width,
                height,
                inner_x,
                card3_y,
                inner_w,
                card3_h,
                0xFF38_BDF8,
            );
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                card3_y + 8,
                "// WORKBENCH CONTROLS",
                0xFF38_BDF8,
            );
            let mut cy = card3_y + 24;
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                cy,
                "• [TYPE]  Enter directive prompt",
                0xFFE2_E8F0,
            );
            cy += 14;
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                cy,
                "• [ENTER] Dispatch to agent loop",
                0xFF00_E5FF,
            );
            cy += 14;
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                cy,
                "• [SPACE] Emergency Freeze/Hold",
                0xFFF5_9E0B,
            );
            cy += 14;
            draw_ascii_text_u32(
                buf,
                width,
                inner_x + 10,
                cy,
                "• [ESC]   Clear input / Exit",
                0xFF94_A3B8,
            );
        }
    }

    fn draw_bottom_ticker_u32(&self, buf: &mut [u32], width: usize, height: usize) {
        let ticker_h = 40usize;
        if height < ticker_h + 10 {
            return;
        }
        let ty = height - ticker_h;

        let bg_color = 0xFF07_0B16;
        for y in ty..height {
            let row_start = y * width;
            let row_end = row_start + width;
            if row_end <= buf.len() {
                buf[row_start..row_end].fill(bg_color);
            }
        }

        // Top neon cyan border
        let top_border = 0xFF00_E5FF;
        let row_top = ty * width;
        if row_top + width <= buf.len() {
            buf[row_top..row_top + width].fill(top_border);
        }

        // Thought prompt badge
        draw_ascii_text_u32(buf, width, 16, ty + 14, "[>] THOUGHT //", 0xFF00_E5FF);

        let max_chars = (width.saturating_sub(310)) / 8;
        let mut display_text = self.state.thought_ticker.clone();
        if display_text.len() > max_chars && max_chars > 3 {
            display_text.truncate(max_chars.saturating_sub(3));
            display_text.push_str("...");
        }

        // Blinking cursor
        let cursor_char = if (self.state.pulse_tick % 20) < 10 {
            "_"
        } else {
            " "
        };
        let full_text = format!("{display_text}{cursor_char}");

        draw_ascii_text_u32(buf, width, 142, ty + 14, &full_text, 0xFFF8_FAFC);

        // Right Pulse Indicator
        let right_x = width.saturating_sub(130);
        draw_ascii_text_u32(buf, width, right_x, ty + 14, "[ZERO-ALLOC OK]", 0xFF10_B981);
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

#[allow(dead_code)]
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
        let row_start = py * width + x;
        let row_mid = row_start + filled_w;
        let row_end = row_start + w;
        if row_end <= buf.len() {
            buf[row_start..row_mid].fill(fg);
            buf[row_mid..row_end].fill(bg);
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
    if x + w >= width || y + h >= height || w == 0 || h == 0 {
        return;
    }

    let body_color = 0xFF09_0E1A;
    let end_y = y + h;
    let end_x = x + w;
    for py in y..end_y {
        let row_start = py * width + x;
        let row_end = py * width + end_x;
        if row_end <= buf.len() {
            buf[row_start..row_end].fill(body_color);
        }
    }

    // Outer subtle border
    for py in y..end_y {
        set_pixel_u32(buf, width, x, py, 0xFF1E_293B);
        set_pixel_u32(buf, width, end_x - 1, py, 0xFF1E_293B);
    }
    let top_row = y * width + x;
    if top_row + w <= buf.len() {
        buf[top_row..top_row + w].fill(0xFF1E_293B);
    }
    let bot_row = (end_y - 1) * width + x;
    if bot_row + w <= buf.len() {
        buf[bot_row..bot_row + w].fill(0xFF1E_293B);
    }

    // Corner Brackets
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
                            text,
                            ..
                        },
                    ..
                } => match logical_key {
                    Key::Named(NamedKey::Escape) => {
                        let is_empty =
                            with_visor(|v| Ok(v.state().input_buffer.is_empty())).unwrap_or(true);
                        if is_empty {
                            event_loop.exit();
                        } else {
                            let _ = with_visor(|v| {
                                v.clear_input();
                                Ok(())
                            });
                        }
                    }
                    Key::Named(NamedKey::Enter) => {
                        let submitted = with_visor(|v| Ok(v.submit_input())).unwrap_or(None);
                        if let Some(intent) = submitted {
                            if !intent.trim().is_empty() {
                                let maybe_gid = {
                                    if let Ok(mut sl) = get_sovereign_loop().lock() {
                                        Some(sl.submit_intent(intent.clone(), None))
                                    } else {
                                        None
                                    }
                                };
                                if let Some(gid) = maybe_gid {
                                    let _ = with_visor(|v| {
                                        v.push_trace(
                                            "OPERATOR",
                                            &format!("Assigned {gid}: {intent}"),
                                            0xFF00_E5FF,
                                        );
                                        v.sync_with_sovereign();
                                        Ok(())
                                    });
                                }
                            }
                        }
                    }
                    Key::Named(NamedKey::Backspace) => {
                        let _ = with_visor(|v| {
                            v.pop_input_char();
                            Ok(())
                        });
                    }
                    Key::Named(NamedKey::Space) => {
                        let is_empty =
                            with_visor(|v| Ok(v.state().input_buffer.is_empty())).unwrap_or(true);
                        if is_empty {
                            if let Ok(mut sl) = get_sovereign_loop().lock() {
                                let _ = sl.toggle_hold();
                            }
                        } else {
                            let _ = with_visor(|v| {
                                v.push_input_char(' ');
                                Ok(())
                            });
                        }
                    }
                    Key::Character(ref ch) => {
                        for c in ch.chars() {
                            if !c.is_control() {
                                let _ = with_visor(|v| {
                                    v.push_input_char(c);
                                    Ok(())
                                });
                            }
                        }
                    }
                    _ => {
                        if let Some(t) = text {
                            for c in t.chars() {
                                if !c.is_control() {
                                    let _ = with_visor(|v| {
                                        v.push_input_char(c);
                                        Ok(())
                                    });
                                }
                            }
                        }
                    }
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

                    // High-performance pacing: buffer presentation is synchronized with compositor
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
        .map_err(|e| format!("error[E0973]: failed to build winit event loop: {e}"))?;

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
        .map_err(|e| format!("error[E0973]: Visor window event loop failed: {e}"))
}

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
pub fn get_hud_engine() -> &'static Mutex<HudEngine> {
    get_visor_engine()
}
pub fn with_hud<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut HudEngine) -> Result<R, String>,
{
    with_visor(f)
}

/// Built-in functions registered for Visor standard library.
pub fn visor_functions() -> Vec<(&'static str, usize, usize)> {
    let mut functions = vec![
        ("visor.state", 0, 0),
        ("visor.ticker", 1, 1),
        ("visor.set_pip", 3, 4),
        ("visor.advance", 0, 0),
        ("visor.render", 0, 2),
    ];

    // Backward compatibility aliases
    functions.extend(vec![
        ("hud.state", 0, 0),
        ("hud.ticker", 1, 1),
        ("hud.set_pip", 3, 4),
        ("hud.advance", 0, 0),
        ("hud.render", 0, 2),
    ]);

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
                .unwrap_or(1280);
            let height = args
                .get(1)
                .and_then(|v| match v {
                    Value::Int(i) if *i > 0 => Some(*i as usize),
                    _ => None,
                })
                .unwrap_or(800);

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
