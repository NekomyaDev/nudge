//! Autonomous Scheduler & Timer Wheel for Nudge Sovereign Agents.
//!
//! Provides wall-clock cron, periodic intervals (`schedule.every`),
//! delayed execution (`schedule.after`), and reactive event watchers
//! (`schedule.on_change`). Supports persistent serialization to NVFS.

use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Timing schedule specifications.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleType {
    /// Periodic execution recurring every `interval_ms`.
    Interval { interval_ms: u64 },
    /// One-shot execution fired after `delay_ms`.
    OneShot { delay_ms: u64 },
    /// Reactive execution triggered when a watched file, URL, or state key changes.
    OnChange { target: String },
}

/// Lifecycle status of a scheduled autonomous task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScheduleStatus {
    Active,
    Paused,
    Completed,
    Cancelled,
}

impl ScheduleStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Scheduled task descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub id: String,
    pub task_name: String,
    pub schedule_type: ScheduleType,
    pub next_fire_ms: u64,
    pub fire_count: u64,
    pub max_runs: Option<u64>,
    pub args: Vec<serde_json::Value>,
    pub status: ScheduleStatus,
    pub created_at: u64,
}

/// Central autonomous scheduler managing timers and trigger queues.
#[derive(Debug, Default)]
pub struct AutonomousScheduler {
    tasks: HashMap<String, ScheduledTask>,
    seq: u64,
}

impl AutonomousScheduler {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            seq: 0,
        }
    }

    /// Schedule an action recurring every `interval_ms`.
    pub fn schedule_every(
        &mut self,
        task_name: String,
        interval_ms: u64,
        args: Vec<serde_json::Value>,
        max_runs: Option<u64>,
    ) -> String {
        self.schedule_every_at(task_name, interval_ms, args, max_runs, now_ms())
    }

    /// Schedule an action recurring every `interval_ms` starting at reference time `now`.
    pub fn schedule_every_at(
        &mut self,
        task_name: String,
        interval_ms: u64,
        args: Vec<serde_json::Value>,
        max_runs: Option<u64>,
        now: u64,
    ) -> String {
        self.seq = self.seq.saturating_add(1);
        let id = format!("sched_{}_{}", self.seq, task_name);

        let task = ScheduledTask {
            id: id.clone(),
            task_name,
            schedule_type: ScheduleType::Interval { interval_ms },
            next_fire_ms: now.saturating_add(interval_ms),
            fire_count: 0,
            max_runs,
            args,
            status: ScheduleStatus::Active,
            created_at: now,
        };

        self.tasks.insert(id.clone(), task);
        id
    }

    /// Schedule a one-shot action delayed by `delay_ms`.
    pub fn schedule_after(
        &mut self,
        task_name: String,
        delay_ms: u64,
        args: Vec<serde_json::Value>,
    ) -> String {
        self.schedule_after_at(task_name, delay_ms, args, now_ms())
    }

    /// Schedule a one-shot action delayed by `delay_ms` starting at reference time `now`.
    pub fn schedule_after_at(
        &mut self,
        task_name: String,
        delay_ms: u64,
        args: Vec<serde_json::Value>,
        now: u64,
    ) -> String {
        self.seq = self.seq.saturating_add(1);
        let id = format!("sched_once_{}_{}", self.seq, task_name);

        let task = ScheduledTask {
            id: id.clone(),
            task_name,
            schedule_type: ScheduleType::OneShot { delay_ms },
            next_fire_ms: now.saturating_add(delay_ms),
            fire_count: 0,
            max_runs: Some(1),
            args,
            status: ScheduleStatus::Active,
            created_at: now,
        };

        self.tasks.insert(id.clone(), task);
        id
    }

    /// Register a reactive watcher fired when `target` triggers a change.
    pub fn schedule_on_change(
        &mut self,
        task_name: String,
        target: String,
        args: Vec<serde_json::Value>,
    ) -> String {
        self.seq = self.seq.saturating_add(1);
        let id = format!("sched_watch_{}_{}", self.seq, task_name);
        let now = now_ms();

        let task = ScheduledTask {
            id: id.clone(),
            task_name,
            schedule_type: ScheduleType::OnChange { target },
            next_fire_ms: u64::MAX,
            fire_count: 0,
            max_runs: None,
            args,
            status: ScheduleStatus::Active,
            created_at: now,
        };

        self.tasks.insert(id.clone(), task);
        id
    }

    /// Notify that `target` has changed, triggering matching watchers.
    pub fn notify_change(&mut self, target: &str) -> Vec<ScheduledTask> {
        let mut triggered = Vec::new();
        for task in self.tasks.values_mut() {
            if task.status == ScheduleStatus::Active {
                if let ScheduleType::OnChange { target: t } = &task.schedule_type {
                    if t == target {
                        task.fire_count += 1;
                        if let Some(limit) = task.max_runs {
                            if task.fire_count >= limit {
                                task.status = ScheduleStatus::Completed;
                            }
                        }
                        triggered.push(task.clone());
                    }
                }
            }
        }
        triggered
    }

    /// Cancel a scheduled task by ID.
    pub fn cancel(&mut self, id: &str) -> bool {
        if let Some(task) = self.tasks.get_mut(id) {
            task.status = ScheduleStatus::Cancelled;
            true
        } else {
            false
        }
    }

    /// Advance time to `now` and harvest due tasks ready for execution.
    pub fn tick(&mut self, now: u64) -> Vec<ScheduledTask> {
        let mut due = Vec::new();

        for task in self.tasks.values_mut() {
            if task.status != ScheduleStatus::Active {
                continue;
            }

            match task.schedule_type {
                ScheduleType::Interval { interval_ms } => {
                    if task.next_fire_ms <= now {
                        task.fire_count += 1;
                        task.next_fire_ms = now.saturating_add(interval_ms);
                        if let Some(limit) = task.max_runs {
                            if task.fire_count >= limit {
                                task.status = ScheduleStatus::Completed;
                            }
                        }
                        due.push(task.clone());
                    }
                }
                ScheduleType::OneShot { .. } => {
                    if task.next_fire_ms <= now {
                        task.fire_count += 1;
                        task.status = ScheduleStatus::Completed;
                        due.push(task.clone());
                    }
                }
                ScheduleType::OnChange { .. } => {}
            }
        }

        due
    }

    /// List all tasks currently registered.
    pub fn list(&self) -> Vec<ScheduledTask> {
        let mut list: Vec<ScheduledTask> = self.tasks.values().cloned().collect();
        list.sort_by_key(|a| a.created_at);
        list
    }

    /// Active task count.
    pub fn active_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|t| t.status == ScheduleStatus::Active)
            .count()
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Parse human-readable duration strings ("500ms", "10s", "5m", "2h", "1d") to milliseconds.
pub fn parse_duration_ms(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if let Ok(num) = s.parse::<u64>() {
        return Ok(num);
    }

    if let Some(val) = s.strip_suffix("ms") {
        return val
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("invalid duration '{s}': {e}"));
    }
    if let Some(val) = s.strip_suffix('s') {
        let secs = val
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("invalid duration '{s}': {e}"))?;
        return Ok(secs * 1000);
    }
    if let Some(val) = s.strip_suffix('m') {
        let mins = val
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("invalid duration '{s}': {e}"))?;
        return Ok(mins * 60 * 1000);
    }
    if let Some(val) = s.strip_suffix('h') {
        let hours = val
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("invalid duration '{s}': {e}"))?;
        return Ok(hours * 3600 * 1000);
    }
    if let Some(val) = s.strip_suffix('d') {
        let days = val
            .trim()
            .parse::<u64>()
            .map_err(|e| format!("invalid duration '{s}': {e}"))?;
        return Ok(days * 86400 * 1000);
    }

    Err(format!(
        "unrecognized duration '{s}', expected e.g. '500ms', '10s', '5m', '2h', '1d'"
    ))
}

/// Global process-wide scheduler singleton.
static SCHEDULER: OnceLock<Mutex<AutonomousScheduler>> = OnceLock::new();

pub fn get_scheduler() -> &'static Mutex<AutonomousScheduler> {
    SCHEDULER.get_or_init(|| Mutex::new(AutonomousScheduler::new()))
}

pub fn with_scheduler<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&mut AutonomousScheduler) -> Result<R, String>,
{
    let mutex = get_scheduler();
    let mut sched = mutex
        .lock()
        .map_err(|e| format!("error[E0940]: AutonomousScheduler lock poisoned: {e}"))?;
    f(&mut sched)
}

/// Register schedule functions in standard library dispatch table.
pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    functions.insert(
        "schedule.every".to_string(),
        Value::Native("schedule.every"),
    );
    functions.insert(
        "schedule.after".to_string(),
        Value::Native("schedule.after"),
    );
    functions.insert(
        "schedule.on_change".to_string(),
        Value::Native("schedule.on_change"),
    );
    functions.insert(
        "schedule.notify".to_string(),
        Value::Native("schedule.notify"),
    );
    functions.insert(
        "schedule.cancel".to_string(),
        Value::Native("schedule.cancel"),
    );
    functions.insert("schedule.list".to_string(), Value::Native("schedule.list"));
    functions.insert("schedule.tick".to_string(), Value::Native("schedule.tick"));
    functions
}

/// Dispatch a call to a schedule builtin.
pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "schedule.every" => {
            if args.len() < 2 {
                return Err("schedule.every requires duration and task_name".to_string());
            }
            let interval_ms = match &args[0] {
                Value::Int(i) => (*i).max(1) as u64,
                Value::String(s) => parse_duration_ms(s)?,
                _ => return Err("schedule.every: first arg must be duration or ms".to_string()),
            };
            let task_name = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err("schedule.every: task_name must be a string".to_string()),
            };

            let call_args = if args.len() > 2 {
                match &args[2] {
                    Value::List(xs) => xs
                        .iter()
                        .map(|x| crate::store::value_to_json(x).unwrap_or(serde_json::Value::Null))
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };

            with_scheduler(|s| {
                let id = s.schedule_every(task_name, interval_ms, call_args, None);
                Ok(Value::String(id))
            })
        }

        "schedule.after" => {
            if args.len() < 2 {
                return Err("schedule.after requires delay and task_name".to_string());
            }
            let delay_ms = match &args[0] {
                Value::Int(i) => (*i).max(0) as u64,
                Value::String(s) => parse_duration_ms(s)?,
                _ => return Err("schedule.after: first arg must be delay or ms".to_string()),
            };
            let task_name = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err("schedule.after: task_name must be a string".to_string()),
            };

            let call_args = if args.len() > 2 {
                match &args[2] {
                    Value::List(xs) => xs
                        .iter()
                        .map(|x| crate::store::value_to_json(x).unwrap_or(serde_json::Value::Null))
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };

            with_scheduler(|s| {
                let id = s.schedule_after(task_name, delay_ms, call_args);
                Ok(Value::String(id))
            })
        }

        "schedule.on_change" => {
            if args.len() < 2 {
                return Err("schedule.on_change requires target and task_name".to_string());
            }
            let target = match &args[0] {
                Value::String(s) => s.clone(),
                _ => return Err("schedule.on_change: target must be a string".to_string()),
            };
            let task_name = match &args[1] {
                Value::String(s) => s.clone(),
                _ => return Err("schedule.on_change: task_name must be a string".to_string()),
            };

            let call_args = if args.len() > 2 {
                match &args[2] {
                    Value::List(xs) => xs
                        .iter()
                        .map(|x| crate::store::value_to_json(x).unwrap_or(serde_json::Value::Null))
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };

            with_scheduler(|s| {
                let id = s.schedule_on_change(task_name, target, call_args);
                Ok(Value::String(id))
            })
        }

        "schedule.notify" => {
            if args.is_empty() {
                return Err("schedule.notify requires target argument".to_string());
            }
            let target = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("schedule.notify: target must be a string".to_string()),
            };

            with_scheduler(|s| {
                let fired = s.notify_change(target);
                Ok(Value::Int(fired.len() as i64))
            })
        }

        "schedule.cancel" => {
            if args.is_empty() {
                return Err("schedule.cancel requires id argument".to_string());
            }
            let id = match &args[0] {
                Value::String(s) => s.as_str(),
                _ => return Err("schedule.cancel: id must be a string".to_string()),
            };

            with_scheduler(|s| Ok(Value::Bool(s.cancel(id))))
        }

        "schedule.list" => with_scheduler(|s| {
            let tasks = s.list();
            let mut list_vals = Vec::with_capacity(tasks.len());
            for t in tasks {
                let mut map = HashMap::new();
                map.insert("id".to_string(), Value::String(t.id));
                map.insert("task_name".to_string(), Value::String(t.task_name));
                map.insert(
                    "status".to_string(),
                    Value::String(t.status.as_str().to_string()),
                );
                map.insert(
                    "next_fire_ms".to_string(),
                    Value::Int(t.next_fire_ms as i64),
                );
                map.insert("fire_count".to_string(), Value::Int(t.fire_count as i64));
                list_vals.push(Value::Map(map));
            }
            Ok(Value::List(list_vals))
        }),

        "schedule.tick" => with_scheduler(|s| {
            let now = now_ms();
            let due = s.tick(now);
            let mut res = Vec::with_capacity(due.len());
            for t in due {
                let mut map = HashMap::new();
                map.insert("id".to_string(), Value::String(t.id));
                map.insert("task_name".to_string(), Value::String(t.task_name));
                map.insert("fire_count".to_string(), Value::Int(t.fire_count as i64));
                res.push(Value::Map(map));
            }
            Ok(Value::List(res))
        }),

        _ => Err(format!("unknown schedule function: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_units() {
        assert_eq!(parse_duration_ms("500ms").unwrap(), 500);
        assert_eq!(parse_duration_ms("10s").unwrap(), 10000);
        assert_eq!(parse_duration_ms("5m").unwrap(), 300000);
        assert_eq!(parse_duration_ms("2h").unwrap(), 7200000);
        assert_eq!(parse_duration_ms("1d").unwrap(), 86400000);
    }

    #[test]
    fn test_scheduler_lifecycle() {
        let mut sched = AutonomousScheduler::new();
        let base_time = now_ms();

        let id_every = sched.schedule_every("sync_inventory".into(), 5000, Vec::new(), Some(3));
        let id_once = sched.schedule_after("send_report".into(), 2000, Vec::new());
        let id_watch =
            sched.schedule_on_change("alert_ops".into(), "status_page".into(), Vec::new());

        assert_eq!(sched.active_count(), 3);

        // Advance 1 second: nothing due yet
        let due = sched.tick(base_time + 1000);
        assert!(due.is_empty());

        // Advance 2.5 seconds: one-shot fires
        let due = sched.tick(base_time + 2500);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, id_once);
        assert_eq!(due[0].fire_count, 1);

        // Advance 5.5 seconds: interval fires
        let due = sched.tick(base_time + 5500);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, id_every);
        assert_eq!(due[0].fire_count, 1);

        // Trigger change notification on watcher
        let triggered = sched.notify_change("status_page");
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].id, id_watch);

        // Cancel interval task
        assert!(sched.cancel(&id_every));
        let due = sched.tick(base_time + 15000);
        assert!(due.is_empty());
    }
}
