//! Sovereign Consciousness Loop & Goal Directed Acyclic Graph (DAG) for Nudge.
//!
//! Implements a zero-context accumulation autonomous supervisor that decomposes
//! intents into executable Goal DAGs, tracks dependencies, handles continuous
//! ticks, and exports live telemetry for the Watch-Only Visor HUD.

use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub type GoalId = String;

/// Lifecycle status of a goal node in the DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoalStatus {
    Pending,
    Active,
    Evaluating,
    Completed,
    Failed,
    Healing,
}

impl GoalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Evaluating => "evaluating",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Healing => "healing",
        }
    }
}

/// A node within the autonomous goal DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalNode {
    pub id: GoalId,
    pub parent_id: Option<GoalId>,
    pub intent: String,
    pub status: GoalStatus,
    pub dependencies: Vec<GoalId>,
    pub retry_budget: u32,
    pub retries_spent: u32,
    pub outputs: Option<serde_json::Value>,
    pub error: Option<String>,
    pub timeout_secs: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Execution phase of the sovereign loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SovereignPhase {
    Idle,
    EvaluatingGoal,
    ExecutingPlan,
    SelfHealing,
    Snapshotting,
    Hold,
}

impl SovereignPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::EvaluatingGoal => "evaluating_goal",
            Self::ExecutingPlan => "executing_plan",
            Self::SelfHealing => "self_healing",
            Self::Snapshotting => "snapshotting",
            Self::Hold => "hold",
        }
    }
}

/// Real-time telemetry export consumed by the HUD.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SovereignTelemetry {
    pub phase: SovereignPhase,
    pub active_goal_id: Option<GoalId>,
    pub total_goals: usize,
    pub completed_goals: usize,
    pub failed_goals: usize,
    pub active_goals: usize,
    pub uptime_secs: u64,
    pub tick_count: u64,
    pub replay_ratio: f64,
    pub current_thought: String,
}

/// SovereignLoop supervisor managing autonomous intent execution.
pub struct SovereignLoop {
    goals: HashMap<GoalId, GoalNode>,
    execution_order: Vec<GoalId>,
    phase: SovereignPhase,
    active_goal_id: Option<GoalId>,
    start_time: Instant,
    tick_count: u64,
    current_thought: String,
    total_replay_steps: u64,
    total_execution_steps: u64,
    seq: u64,
}

impl Default for SovereignLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl SovereignLoop {
    pub fn new() -> Self {
        Self {
            goals: HashMap::new(),
            execution_order: Vec::new(),
            phase: SovereignPhase::Idle,
            active_goal_id: None,
            start_time: Instant::now(),
            tick_count: 0,
            current_thought: "Sovereign loop initialized and awaiting intent.".to_string(),
            total_replay_steps: 0,
            total_execution_steps: 0,
            seq: 0,
        }
    }

    /// Submit a high-level intent, inserting a new Goal DAG node.
    pub fn submit_intent(&mut self, intent: String, parent_id: Option<GoalId>) -> GoalId {
        self.submit_intent_with_timeout(intent, parent_id, None)
    }

    /// Submit a high-level intent with an explicit execution timeout in seconds.
    pub fn submit_intent_with_timeout(
        &mut self,
        intent: String,
        parent_id: Option<GoalId>,
        timeout_secs: Option<u64>,
    ) -> GoalId {
        self.seq = self.seq.saturating_add(1);
        let id = format!("goal_{:04}", self.seq);
        let now = now_secs();

        let node = GoalNode {
            id: id.clone(),
            parent_id,
            intent: intent.clone(),
            status: GoalStatus::Pending,
            dependencies: Vec::new(),
            retry_budget: 3,
            retries_spent: 0,
            outputs: None,
            error: None,
            timeout_secs,
            created_at: now,
            updated_at: now,
        };

        self.goals.insert(id.clone(), node);
        self.execution_order.push(id.clone());
        self.current_thought = format!("New intent queued [{}]: {}", id, intent);
        id
    }

    /// Check if target is reachable from start through dependency edges.
    pub fn reaches(&self, from: &str, target: &str) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(from.to_string());
        visited.insert(from.to_string());

        while let Some(current) = queue.pop_front() {
            if current == target {
                return true;
            }
            if let Some(node) = self.goals.get(&current) {
                for dep in &node.dependencies {
                    if visited.insert(dep.clone()) {
                        queue.push_back(dep.clone());
                    }
                }
            }
        }
        false
    }

    /// Attach a dependency requirement (`goal_id` waits on `dep_id`).
    /// Rejects cyclic dependencies with an explicit Err.
    pub fn add_dependency(&mut self, goal_id: &str, dep_id: &str) -> Result<(), String> {
        if goal_id == dep_id {
            return Err(format!(
                "cyclic dependency detected: goal '{goal_id}' cannot depend on itself"
            ));
        }
        if !self.goals.contains_key(dep_id) {
            return Err(format!("dependency goal '{dep_id}' does not exist"));
        }
        if !self.goals.contains_key(goal_id) {
            return Err(format!("target goal '{goal_id}' does not exist"));
        }
        // Cycle check: If dep_id can already reach goal_id, adding goal_id -> dep_id forms a cycle
        if self.reaches(dep_id, goal_id) {
            return Err(format!(
                "cyclic dependency detected: adding '{goal_id}' -> '{dep_id}' creates a cycle"
            ));
        }

        let goal = self.goals.get_mut(goal_id).unwrap();
        if !goal.dependencies.contains(&dep_id.to_string()) {
            goal.dependencies.push(dep_id.to_string());
        }
        Ok(())
    }

    /// Reset internal state for test isolation or fresh supervisor runs.
    pub fn clear(&mut self) {
        self.goals.clear();
        self.execution_order.clear();
        self.phase = SovereignPhase::Idle;
        self.active_goal_id = None;
        self.start_time = Instant::now();
        self.tick_count = 0;
        self.current_thought = "Sovereign loop reset.".to_string();
        self.total_replay_steps = 0;
        self.total_execution_steps = 0;
        self.seq = 0;
    }

    /// Find the next executable goal whose dependencies are all satisfied (`Completed`).
    pub fn next_executable_goal(&self) -> Option<GoalNode> {
        for id in &self.execution_order {
            if let Some(goal) = self.goals.get(id) {
                if goal.status == GoalStatus::Pending || goal.status == GoalStatus::Active {
                    let satisfied = goal.dependencies.iter().all(|dep_id| {
                        self.goals
                            .get(dep_id)
                            .is_some_and(|d| d.status == GoalStatus::Completed)
                    });
                    if satisfied {
                        return Some(goal.clone());
                    }
                }
            }
        }
        None
    }

    /// Mark an intent as active and begin evaluation.
    pub fn start_evaluation(&mut self, goal_id: &str) -> Result<(), String> {
        let goal = self
            .goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{goal_id}' not found"))?;
        goal.status = GoalStatus::Evaluating;
        goal.updated_at = now_secs();
        self.active_goal_id = Some(goal_id.to_string());
        self.phase = SovereignPhase::EvaluatingGoal;
        self.current_thought = format!("Evaluating Goal #{}: {}", goal_id, goal.intent);
        Ok(())
    }

    /// Mark an evaluated goal as successfully completed.
    pub fn complete_goal(
        &mut self,
        goal_id: &str,
        output: serde_json::Value,
    ) -> Result<(), String> {
        let goal = self
            .goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{goal_id}' not found"))?;
        goal.status = GoalStatus::Completed;
        goal.outputs = Some(output);
        goal.updated_at = now_secs();

        if self.active_goal_id.as_deref() == Some(goal_id) {
            self.active_goal_id = None;
            self.phase = SovereignPhase::Idle;
        }

        self.current_thought = format!("Goal #{} completed successfully.", goal_id);
        Ok(())
    }

    /// Handle failure with automated self-healing retry or terminal failure.
    /// When terminal failure occurs, cascades abort to all dependent subgoals.
    pub fn fail_goal(&mut self, goal_id: &str, error: String) -> Result<bool, String> {
        let goal = self
            .goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal '{goal_id}' not found"))?;
        goal.retries_spent += 1;
        goal.updated_at = now_secs();
        goal.error = Some(error.clone());

        if goal.retries_spent <= goal.retry_budget {
            goal.status = GoalStatus::Healing;
            self.phase = SovereignPhase::SelfHealing;
            self.current_thought = format!(
                "Self-healing Goal #{}: attempt {}/{} after error: {}",
                goal_id, goal.retries_spent, goal.retry_budget, error
            );
            Ok(true) // will retry
        } else {
            goal.status = GoalStatus::Failed;
            if self.active_goal_id.as_deref() == Some(goal_id) {
                self.active_goal_id = None;
                self.phase = SovereignPhase::Idle;
            }
            self.current_thought = format!("Goal #{} permanently failed: {}", goal_id, error);

            // Cascade failure to downstream dependencies
            self.cascade_failure(goal_id, &error);

            Ok(false) // terminal fail
        }
    }

    fn cascade_failure(&mut self, failed_id: &str, error_msg: &str) {
        let mut to_fail = Vec::new();
        for (id, node) in &self.goals {
            if (node.status == GoalStatus::Pending || node.status == GoalStatus::Active)
                && node.dependencies.iter().any(|d| d == failed_id)
            {
                to_fail.push(id.clone());
            }
        }

        for id in to_fail {
            if let Some(node) = self.goals.get_mut(&id) {
                node.status = GoalStatus::Failed;
                node.error = Some(format!(
                    "aborted: upstream dependency '{failed_id}' failed: {error_msg}"
                ));
                node.updated_at = now_secs();
            }
            self.cascade_failure(&id, error_msg);
        }
    }

    /// Record execution step metrics.
    pub fn record_step(&mut self, is_replay: bool) {
        self.total_execution_steps = self.total_execution_steps.saturating_add(1);
        if is_replay {
            self.total_replay_steps = self.total_replay_steps.saturating_add(1);
        }
    }

    /// Toggle emergency hold on the sovereign loop. Returns true if held/paused.
    pub fn toggle_hold(&mut self) -> bool {
        if self.phase == SovereignPhase::Hold {
            self.phase = SovereignPhase::Idle;
            self.current_thought = "Sovereign loop resumed from operator hold.".to_string();
            false
        } else {
            self.phase = SovereignPhase::Hold;
            self.current_thought =
                "EMERGENCY HOLD: Sovereign loop execution frozen by operator.".to_string();
            true
        }
    }

    /// Advance one autonomous tick.
    pub fn tick(&mut self) -> SovereignTelemetry {
        self.tick_count = self.tick_count.saturating_add(1);

        if self.phase == SovereignPhase::Hold {
            return self.get_telemetry();
        }

        let now = now_secs();

        // Check active goal timeout
        if let Some(active_id) = &self.active_goal_id.clone() {
            if let Some(goal) = self.goals.get(active_id) {
                if let Some(timeout) = goal.timeout_secs {
                    if now.saturating_sub(goal.updated_at) >= timeout {
                        let _ = self.fail_goal(
                            active_id,
                            format!("execution deadline exceeded (>{timeout}s)"),
                        );
                    }
                }
            }
        }

        if self.phase == SovereignPhase::Idle {
            if let Some(next) = self.next_executable_goal() {
                let _ = self.start_evaluation(&next.id);
            }
        }

        self.get_telemetry()
    }

    /// Set an explicit thought note (for live subtitle stream).
    pub fn set_thought(&mut self, thought: String) {
        self.current_thought = thought;
    }

    /// Generate an immutable telemetry snapshot.
    pub fn get_telemetry(&self) -> SovereignTelemetry {
        let total_goals = self.goals.len();
        let mut completed_goals = 0;
        let mut failed_goals = 0;
        let mut active_goals = 0;

        for g in self.goals.values() {
            match g.status {
                GoalStatus::Completed => completed_goals += 1,
                GoalStatus::Failed => failed_goals += 1,
                GoalStatus::Active | GoalStatus::Evaluating | GoalStatus::Healing => {
                    active_goals += 1
                }
                GoalStatus::Pending => {}
            }
        }

        let replay_ratio = if self.total_execution_steps > 0 {
            (self.total_replay_steps as f64) / (self.total_execution_steps as f64)
        } else {
            1.0
        };

        SovereignTelemetry {
            phase: self.phase,
            active_goal_id: self.active_goal_id.clone(),
            total_goals,
            completed_goals,
            failed_goals,
            active_goals,
            uptime_secs: self.start_time.elapsed().as_secs(),
            tick_count: self.tick_count,
            replay_ratio,
            current_thought: self.current_thought.clone(),
        }
    }

    /// List all goals in topological insertion order.
    pub fn list_goals(&self) -> Vec<GoalNode> {
        let mut list = Vec::with_capacity(self.execution_order.len());
        for id in &self.execution_order {
            if let Some(g) = self.goals.get(id) {
                list.push(g.clone());
            }
        }
        list
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Global sovereign loop supervisor singleton.
static SOVEREIGN_LOOP: OnceLock<Mutex<SovereignLoop>> = OnceLock::new();

pub fn get_sovereign_loop() -> &'static Mutex<SovereignLoop> {
    SOVEREIGN_LOOP.get_or_init(|| Mutex::new(SovereignLoop::new()))
}

pub fn with_sovereign_loop<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut SovereignLoop) -> Result<R, String>,
{
    let mutex = get_sovereign_loop();
    let mut sl = mutex
        .lock()
        .map_err(|e| format!("error[E0950]: SovereignLoop lock poisoned: {e}"))?;
    f(&mut sl)
}

/// Register sovereign functions in standard library dispatch table.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert(
        "sovereign.submit".to_string(),
        Value::Native("sovereign.submit"),
    );
    functions.insert(
        "sovereign.telemetry".to_string(),
        Value::Native("sovereign.telemetry"),
    );
    functions.insert(
        "sovereign.next_goal".to_string(),
        Value::Native("sovereign.next_goal"),
    );
    functions.insert(
        "sovereign.complete_goal".to_string(),
        Value::Native("sovereign.complete_goal"),
    );
    functions.insert(
        "sovereign.fail_goal".to_string(),
        Value::Native("sovereign.fail_goal"),
    );
    functions.insert(
        "sovereign.list_goals".to_string(),
        Value::Native("sovereign.list_goals"),
    );
    functions.insert(
        "sovereign.tick".to_string(),
        Value::Native("sovereign.tick"),
    );
    functions
}

/// Dispatch a call to a sovereign builtin.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "sovereign.submit" => {
            if args.is_empty() {
                return Err("sovereign.submit requires intent argument".to_string());
            }
            let intent = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("sovereign.submit: intent must be string".to_string()),
            };
            let parent_id = args.get(1).and_then(|a| match a {
                Value::String(s) => Some(s.clone()),
                _ => None,
            });

            with_sovereign_loop(|sl| {
                let id = sl.submit_intent(intent, parent_id);
                Ok(Value::String(id))
            })
        }

        "sovereign.telemetry" => with_sovereign_loop(|sl| {
            let t = sl.get_telemetry();
            let mut map = HashMap::new();
            map.insert(
                "phase".to_string(),
                Value::String(t.phase.as_str().to_string()),
            );
            map.insert(
                "active_goal_id".to_string(),
                Value::String(t.active_goal_id.unwrap_or_default()),
            );
            map.insert("total_goals".to_string(), Value::Int(t.total_goals as i64));
            map.insert(
                "completed_goals".to_string(),
                Value::Int(t.completed_goals as i64),
            );
            map.insert(
                "failed_goals".to_string(),
                Value::Int(t.failed_goals as i64),
            );
            map.insert(
                "active_goals".to_string(),
                Value::Int(t.active_goals as i64),
            );
            map.insert("uptime_secs".to_string(), Value::Int(t.uptime_secs as i64));
            map.insert("tick_count".to_string(), Value::Int(t.tick_count as i64));
            map.insert("replay_ratio".to_string(), Value::Float(t.replay_ratio));
            map.insert("thought".to_string(), Value::String(t.current_thought));
            Ok(Value::Map(map))
        }),

        "sovereign.next_goal" => with_sovereign_loop(|sl| {
            if let Some(g) = sl.next_executable_goal() {
                let mut map = HashMap::new();
                map.insert("id".to_string(), Value::String(g.id));
                map.insert("intent".to_string(), Value::String(g.intent));
                map.insert(
                    "status".to_string(),
                    Value::String(g.status.as_str().to_string()),
                );
                Ok(Value::Map(map))
            } else {
                Ok(Value::None)
            }
        }),

        "sovereign.complete_goal" => {
            if args.is_empty() {
                return Err("sovereign.complete_goal requires id".to_string());
            }
            let id = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("sovereign.complete_goal: id must be string".to_string()),
            };
            let output = if let Some(v) = args.get(1) {
                crate::store::value_to_json(v).unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            };

            with_sovereign_loop(|sl| {
                sl.complete_goal(id, output)?;
                Ok(Value::Bool(true))
            })
        }

        "sovereign.fail_goal" => {
            if args.is_empty() {
                return Err("sovereign.fail_goal requires id and error message".to_string());
            }
            let id = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("sovereign.fail_goal: id must be string".to_string()),
            };
            let err = match args.get(1) {
                Some(Value::String(s)) => s.clone(),
                _ => "unspecified error".to_string(),
            };

            with_sovereign_loop(|sl| {
                let will_retry = sl.fail_goal(id, err)?;
                Ok(Value::Bool(will_retry))
            })
        }

        "sovereign.list_goals" => with_sovereign_loop(|sl| {
            let goals = sl.list_goals();
            let mut res = Vec::with_capacity(goals.len());
            for g in goals {
                let mut map = HashMap::new();
                map.insert("id".to_string(), Value::String(g.id));
                map.insert("intent".to_string(), Value::String(g.intent));
                map.insert(
                    "status".to_string(),
                    Value::String(g.status.as_str().to_string()),
                );
                map.insert(
                    "retry_budget".to_string(),
                    Value::Int(g.retry_budget as i64),
                );
                map.insert(
                    "retries_spent".to_string(),
                    Value::Int(g.retries_spent as i64),
                );
                res.push(Value::Map(map));
            }
            Ok(Value::List(res))
        }),

        "sovereign.tick" => with_sovereign_loop(|sl| {
            let _ = sl.tick();
            let t = sl.get_telemetry();
            let mut map = HashMap::new();
            map.insert(
                "phase".to_string(),
                Value::String(t.phase.as_str().to_string()),
            );
            map.insert(
                "active_goals".to_string(),
                Value::Int(t.active_goals as i64),
            );
            map.insert("thought".to_string(), Value::String(t.current_thought));
            Ok(Value::Map(map))
        }),

        _ => Err(format!("unknown sovereign function: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goal_dag_dependencies_and_resolution() {
        let mut sl = SovereignLoop::new();

        let id1 = sl.submit_intent("Crawl price page".into(), None);
        let id2 = sl.submit_intent("Extract product specs".into(), None);
        let id3 = sl.submit_intent("Compile summary report".into(), None);

        // id3 depends on id1 and id2
        sl.add_dependency(&id3, &id1).expect("add dep id1");
        sl.add_dependency(&id3, &id2).expect("add dep id2");

        // Initially, next executable is id1 (id3 blocked by id1 and id2)
        let next = sl.next_executable_goal().expect("has executable");
        assert_eq!(next.id, id1);

        sl.start_evaluation(&id1).expect("eval id1");
        sl.complete_goal(&id1, serde_json::json!({"prices": [99, 120]}))
            .expect("complete id1");

        // Now next is id2
        let next = sl.next_executable_goal().expect("has executable");
        assert_eq!(next.id, id2);

        sl.start_evaluation(&id2).expect("eval id2");
        sl.complete_goal(&id2, serde_json::json!({"specs": "8GB RAM"}))
            .expect("complete id2");

        // Now both id1 and id2 are completed -> id3 is unblocked!
        let next = sl.next_executable_goal().expect("id3 unblocked");
        assert_eq!(next.id, id3);

        sl.start_evaluation(&id3).expect("eval id3");
        sl.complete_goal(&id3, serde_json::json!({"report": "OK"}))
            .expect("complete id3");

        let telem = sl.get_telemetry();
        assert_eq!(telem.total_goals, 3);
        assert_eq!(telem.completed_goals, 3);
        assert_eq!(telem.failed_goals, 0);
    }

    #[test]
    fn test_self_healing_retry_and_terminal_failure() {
        let mut sl = SovereignLoop::new();
        let id = sl.submit_intent("Connect to external API".into(), None);

        sl.start_evaluation(&id).expect("start");

        // Attempt 1 fails -> triggers Healing
        let will_retry = sl.fail_goal(&id, "503 Gateway Timeout".into()).unwrap();
        assert!(will_retry);
        assert_eq!(sl.goals.get(&id).unwrap().status, GoalStatus::Healing);

        // Attempt 2 fails -> Healing
        let will_retry = sl.fail_goal(&id, "503 Gateway Timeout".into()).unwrap();
        assert!(will_retry);

        // Attempt 3 fails -> Healing
        let will_retry = sl.fail_goal(&id, "503 Gateway Timeout".into()).unwrap();
        assert!(will_retry);

        // Attempt 4 exceeds retry budget (3) -> Terminal Failure
        let will_retry = sl.fail_goal(&id, "503 Gateway Timeout".into()).unwrap();
        assert!(!will_retry);
        assert_eq!(sl.goals.get(&id).unwrap().status, GoalStatus::Failed);

        let telem = sl.get_telemetry();
        assert_eq!(telem.failed_goals, 1);
    }
}
