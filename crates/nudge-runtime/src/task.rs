//! Task lifecycle and cooperative cancellation primitives for Nudge.
//!
//! Implements:
//! - Hierarchical task identifiers ([`TaskId`]) e.g. `root`, `root.0`, `root.0.1`.
//! - Explicit task lifecycle state machine ([`TaskState`]): `Pending`, `Running`, `Completed`, `Failed`, `Cancelled`.
//! - Cooperative cancellation tokens ([`TaskToken`]) with hierarchical cancellation propagation.
//! - Global task registry and thread-local active task tracking.
//! - Budget reservation release and fail-closed tool invocation prevention on cancel.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// Hierarchical task identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(String);

impl TaskId {
    /// Create the root task ID ("root").
    pub fn root() -> Self {
        Self("root".to_string())
    }

    /// Create an explicit task ID.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Create a child task ID with index (e.g. `root.0`).
    pub fn child(&self, index: usize) -> Self {
        Self(format!("{}.{}", self.0, index))
    }

    /// Create a named child task ID (e.g. `root.worker`).
    pub fn child_named(&self, name: &str) -> Self {
        Self(format!("{}.{}", self.0, name))
    }

    /// Borrow the string representation of this task ID.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Explicit lifecycle states for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl TaskState {
    /// Terminal states cannot transition to any other state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskState::Completed | TaskState::Failed | TaskState::Cancelled
        )
    }

    /// Standard string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Pending => "pending",
            TaskState::Running => "running",
            TaskState::Completed => "completed",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for TaskState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(TaskState::Pending),
            "running" => Ok(TaskState::Running),
            "completed" => Ok(TaskState::Completed),
            "failed" => Ok(TaskState::Failed),
            "cancelled" => Ok(TaskState::Cancelled),
            _ => Err(format!("unknown task state '{s}'")),
        }
    }
}

type TaskResultSlot = Arc<(Mutex<Option<Result<serde_json::Value, String>>>, Condvar)>;

/// A cooperative cancellation and lifecycle token for a task.
#[derive(Debug, Clone)]
pub struct TaskToken {
    pub id: TaskId,
    cancelled: Arc<AtomicBool>,
    parent: Option<Arc<TaskToken>>,
    children: Arc<Mutex<Vec<TaskToken>>>,
    state: Arc<Mutex<TaskState>>,
    budget_reserved: Arc<Mutex<f64>>,
    result: TaskResultSlot,
}

impl TaskToken {
    /// Create a new root task token.
    pub fn root() -> Self {
        Self {
            id: TaskId::root(),
            cancelled: Arc::new(AtomicBool::new(false)),
            parent: None,
            children: Arc::new(Mutex::new(Vec::new())),
            state: Arc::new(Mutex::new(TaskState::Pending)),
            budget_reserved: Arc::new(Mutex::new(0.0)),
            result: Arc::new((Mutex::new(None), Condvar::new())),
        }
    }

    /// Create a task token with a given ID.
    pub fn new(id: TaskId) -> Self {
        Self {
            id,
            cancelled: Arc::new(AtomicBool::new(false)),
            parent: None,
            children: Arc::new(Mutex::new(Vec::new())),
            state: Arc::new(Mutex::new(TaskState::Pending)),
            budget_reserved: Arc::new(Mutex::new(0.0)),
            result: Arc::new((Mutex::new(None), Condvar::new())),
        }
    }

    /// Spawn a child task token with an index.
    pub fn child(&self, index: usize) -> Self {
        let is_parent_cancelled = self.is_cancelled();
        let child = Self {
            id: self.id.child(index),
            cancelled: Arc::new(AtomicBool::new(is_parent_cancelled)),
            parent: Some(Arc::new(self.clone())),
            children: Arc::new(Mutex::new(Vec::new())),
            state: Arc::new(Mutex::new(if is_parent_cancelled {
                TaskState::Cancelled
            } else {
                TaskState::Pending
            })),
            budget_reserved: Arc::new(Mutex::new(0.0)),
            result: Arc::new((Mutex::new(None), Condvar::new())),
        };
        self.children.lock().unwrap().push(child.clone());
        child
    }

    /// Spawn a child task token with a name.
    pub fn child_named(&self, name: &str) -> Self {
        let is_parent_cancelled = self.is_cancelled();
        let child = Self {
            id: self.id.child_named(name),
            cancelled: Arc::new(AtomicBool::new(is_parent_cancelled)),
            parent: Some(Arc::new(self.clone())),
            children: Arc::new(Mutex::new(Vec::new())),
            state: Arc::new(Mutex::new(if is_parent_cancelled {
                TaskState::Cancelled
            } else {
                TaskState::Pending
            })),
            budget_reserved: Arc::new(Mutex::new(0.0)),
            result: Arc::new((Mutex::new(None), Condvar::new())),
        };
        self.children.lock().unwrap().push(child.clone());
        child
    }

    /// Check if this task or any of its ancestors is cancelled.
    pub fn is_cancelled(&self) -> bool {
        {
            let st = self.state.lock().unwrap();
            if *st == TaskState::Cancelled {
                return true;
            }
            if *st == TaskState::Completed || *st == TaskState::Failed {
                return false;
            }
        }
        if self.cancelled.load(Ordering::SeqCst) {
            return true;
        }
        if let Some(parent) = &self.parent {
            if parent.is_cancelled() {
                let mut st = self.state.lock().unwrap();
                if st.is_terminal() {
                    return false;
                }
                self.cancelled.store(true, Ordering::SeqCst);
                *st = TaskState::Cancelled;
                drop(st);
                self.release_budget();
                return true;
            }
        }
        false
    }

    /// Cancel this task and propagate cancellation to all subtasks.
    pub fn cancel(&self) {
        {
            let mut st = self.state.lock().unwrap();
            if st.is_terminal() {
                let kids = self.children.lock().unwrap().clone();
                for kid in kids {
                    kid.cancel();
                }
                return;
            }
            *st = TaskState::Cancelled;
        }
        self.cancelled.store(true, Ordering::SeqCst);
        self.release_budget();
        {
            let (lock, cvar) = &*self.result;
            let mut r = lock.lock().unwrap();
            if r.is_none() {
                *r = Some(Err("error[E-task-0001]: TaskCancelled".to_string()));
            }
            cvar.notify_all();
        }
        let kids = self.children.lock().unwrap().clone();
        for kid in kids {
            kid.cancel();
        }
    }

    /// Current lifecycle state.
    pub fn state(&self) -> TaskState {
        if self.is_cancelled() {
            let mut st = self.state.lock().unwrap();
            if !st.is_terminal() {
                *st = TaskState::Cancelled;
            }
            return TaskState::Cancelled;
        }
        *self.state.lock().unwrap()
    }

    /// Update the lifecycle state (terminal states cannot be overwritten).
    pub fn set_state(&self, new_state: TaskState) {
        let mut st = self.state.lock().unwrap();
        if st.is_terminal() {
            return;
        }
        *st = new_state;
        if new_state.is_terminal() {
            let (lock, cvar) = &*self.result;
            let mut r = lock.lock().unwrap();
            if r.is_none() {
                *r = match new_state {
                    TaskState::Completed => Some(Ok(serde_json::Value::Null)),
                    TaskState::Cancelled => {
                        Some(Err("error[E-task-0001]: TaskCancelled".to_string()))
                    }
                    TaskState::Failed => Some(Err("error[E-task-0002]: TaskFailed".to_string())),
                    _ => None,
                };
            }
            cvar.notify_all();
        }
        if new_state == TaskState::Cancelled {
            drop(st);
            self.cancel();
        }
    }

    /// Record task outcome.
    pub fn set_result(&self, res: Result<serde_json::Value, String>) {
        let (lock, cvar) = &*self.result;
        let mut r = lock.lock().unwrap();
        {
            let mut st = self.state.lock().unwrap();
            let is_cancelled = *st == TaskState::Cancelled
                || self.cancelled.load(Ordering::SeqCst)
                || self
                    .parent
                    .as_ref()
                    .map(|p| p.is_cancelled())
                    .unwrap_or(false);
            if is_cancelled {
                if !st.is_terminal() {
                    *st = TaskState::Cancelled;
                }
                if r.is_none() {
                    *r = Some(Err("error[E-task-0001]: TaskCancelled".to_string()));
                }
                cvar.notify_all();
                return;
            }
            if !st.is_terminal() {
                *st = match &res {
                    Ok(_) => TaskState::Completed,
                    Err(e) if e.contains("TaskCancelled") || e.contains("E-task-0001") => {
                        TaskState::Cancelled
                    }
                    Err(_) => TaskState::Failed,
                };
            }
        }
        *r = Some(res);
        cvar.notify_all();
    }

    /// Retrieve recorded task outcome.
    pub fn get_result(&self) -> Option<Result<serde_json::Value, String>> {
        let (lock, _) = &*self.result;
        lock.lock().unwrap().clone()
    }

    /// Join task synchronously, blocking until completion, failure, or cancellation.
    pub fn join(&self) -> Result<serde_json::Value, String> {
        let (lock, cvar) = &*self.result;
        let mut guard = lock.lock().unwrap();
        while guard.is_none() {
            guard = cvar.wait(guard).unwrap();
        }
        match guard.as_ref().unwrap() {
            Ok(val) => Ok(val.clone()),
            Err(e) => {
                if e.contains("TaskCancelled") || e.contains("E-task-0001") {
                    Err("error[E-task-0001]: TaskCancelled".to_string())
                } else if e.starts_with("error[E-task-0002]: TaskFailed") {
                    Err(e.clone())
                } else {
                    Err(format!("error[E-task-0002]: TaskFailed: {e}"))
                }
            }
        }
    }

    /// Add to this task's held budget reservation.
    pub fn add_reservation(&self, amount: f64) {
        let mut r = self.budget_reserved.lock().unwrap();
        *r += amount;
    }

    /// Deduct from this task's held budget reservation.
    pub fn sub_reservation(&self, amount: f64) {
        let mut r = self.budget_reserved.lock().unwrap();
        *r = (*r - amount).max(0.0);
    }

    /// Release all budget reservations held by this task.
    pub fn release_budget(&self) {
        let mut r = self.budget_reserved.lock().unwrap();
        let amount = *r;
        *r = 0.0;
        drop(r);
        if amount > 0.0 {
            budget_release_internal(amount);
        }
    }
}

// ── Budget tracking under NUDGE_BUDGET ───────────────────────────────

#[derive(Debug, Default)]
struct BudgetTracker {
    spent: f64,
    reserved: f64,
}

static BUDGET_TRACKER: Mutex<BudgetTracker> = Mutex::new(BudgetTracker {
    spent: 0.0,
    reserved: 0.0,
});

/// Get `NUDGE_BUDGET` from environment if configured and numeric.
pub fn budget_limit() -> Option<f64> {
    std::env::var("NUDGE_BUDGET")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
}

/// Reserve budget capacity for an in-flight operation.
///
/// Refuses with `error[E-task-0001]: TaskCancelled` if the active task is cancelled.
pub fn budget_reserve(amount: f64) -> Result<(), String> {
    if is_current_cancelled() {
        return Err("error[E-task-0001]: TaskCancelled".to_string());
    }
    let cur = current_task();
    if let Some(ref task) = cur {
        if task.is_cancelled() {
            return Err("error[E-task-0001]: TaskCancelled".to_string());
        }
    }
    if let Some(limit) = budget_limit() {
        let mut tracker = BUDGET_TRACKER.lock().unwrap();
        if is_current_cancelled() {
            return Err("error[E-task-0001]: TaskCancelled".to_string());
        }
        let available = limit - tracker.spent - tracker.reserved;
        if amount > available + 1e-9 {
            return Err(format!(
                "run budget exhausted: ${:.4} spent of ${:.4} (${:.4} reserved by in-flight calls)",
                tracker.spent, limit, tracker.reserved
            ));
        }
        tracker.reserved += amount;
    }
    if let Some(task) = cur {
        if task.is_cancelled() {
            budget_release_internal(amount);
            return Err("error[E-task-0001]: TaskCancelled".to_string());
        }
        task.add_reservation(amount);
        if task.is_cancelled() {
            task.release_budget();
            return Err("error[E-task-0001]: TaskCancelled".to_string());
        }
    }
    Ok(())
}

fn budget_release_internal(amount: f64) {
    if let Some(_limit) = budget_limit() {
        let mut tracker = BUDGET_TRACKER.lock().unwrap();
        tracker.reserved = (tracker.reserved - amount).max(0.0);
    }
}

/// Release held budget capacity.
pub fn budget_release(amount: f64) {
    budget_release_internal(amount);
    if let Some(task) = current_task() {
        task.sub_reservation(amount);
    }
}

/// Charge spent budget capacity.
pub fn budget_charge(amount: f64) -> Result<(), String> {
    if let Some(limit) = budget_limit() {
        let mut tracker = BUDGET_TRACKER.lock().unwrap();
        tracker.spent += amount;
        if tracker.spent > limit + 1e-9 {
            return Err(format!(
                "run budget exceeded: ${:.4} spent of ${:.4}",
                tracker.spent, limit
            ));
        }
    }
    Ok(())
}

/// Reset budget tracking (for test isolation).
pub fn budget_reset() {
    let mut tracker = BUDGET_TRACKER.lock().unwrap();
    tracker.spent = 0.0;
    tracker.reserved = 0.0;
}

/// Query budget status (spent, reserved, limit).
pub fn budget_status() -> (f64, f64, Option<f64>) {
    let tracker = BUDGET_TRACKER.lock().unwrap();
    (tracker.spent, tracker.reserved, budget_limit())
}

// ── Task Registry ───────────────────────────────────────────────────

static TASK_REGISTRY: Mutex<Option<HashMap<String, TaskToken>>> = Mutex::new(None);

fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<String, TaskToken>) -> R,
{
    let mut lock = TASK_REGISTRY.lock().unwrap();
    if lock.is_none() {
        *lock = Some(HashMap::new());
    }
    f(lock.as_mut().unwrap())
}

/// Register a task token in the global registry.
pub fn task_register(token: TaskToken) {
    with_registry(|reg| {
        reg.insert(token.id.as_str().to_string(), token);
    });
}

/// Spawn a child task under the active task (or root).
pub fn task_spawn(name: Option<&str>) -> TaskToken {
    let parent = current_task().unwrap_or_else(TaskToken::root);
    let child = if let Some(n) = name {
        parent.child_named(n)
    } else {
        let count = parent.children.lock().unwrap().len();
        parent.child(count)
    };
    task_register(child.clone());
    child
}

/// Lookup a task token by ID.
pub fn task_lookup(id: &str) -> Option<TaskToken> {
    with_registry(|reg| reg.get(id).cloned())
}

/// Cancel a task by ID.
pub fn task_cancel(id: &str) -> Result<(), String> {
    if let Some(token) = task_lookup(id) {
        token.cancel();
        Ok(())
    } else {
        Err(format!(
            "error[E-task-0003]: InvalidTaskHandle: task '{id}' not found"
        ))
    }
}

/// Query status of a task by ID.
pub fn task_status(id: &str) -> Result<TaskState, String> {
    if let Some(token) = task_lookup(id) {
        Ok(token.state())
    } else {
        Err(format!(
            "error[E-task-0003]: InvalidTaskHandle: task '{id}' not found"
        ))
    }
}

/// Check if a task (or the current active task if None) is cancelled.
pub fn task_is_cancelled(id: Option<&str>) -> bool {
    if let Some(id) = id {
        task_lookup(id).map(|t| t.is_cancelled()).unwrap_or(false)
    } else {
        is_current_cancelled()
    }
}

/// Join a task by ID.
pub fn task_join(id: &str) -> Result<serde_json::Value, String> {
    let token = task_lookup(id)
        .ok_or_else(|| format!("error[E-task-0003]: InvalidTaskHandle: task '{id}' not found"))?;
    token.join()
}

/// Clear the task registry and budget state.
pub fn task_reset() {
    set_current_task(None);
    with_registry(|reg| {
        reg.clear();
    });
    budget_reset();
}

/// Query the number of registered tasks in the global task registry.
pub fn task_registry_count() -> usize {
    with_registry(|reg| reg.len())
}

/// Query whether the global task registry is empty.
pub fn task_registry_is_empty() -> bool {
    with_registry(|reg| reg.is_empty())
}

/// Serialize tests that share process-global task state like `TASK_REGISTRY`,
/// `BUDGET_TRACKER`, and `NUDGE_BUDGET`.
pub static TASK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ── Thread-local current task & checkpoints ──────────────────────────

thread_local! {
    static CURRENT_TASK: RefCell<Option<TaskToken>> = const { RefCell::new(None) };
}

/// Get the active task token for the current thread.
pub fn current_task() -> Option<TaskToken> {
    CURRENT_TASK.with(|c| c.borrow().clone())
}

/// Set the active task token for the current thread, returning the previous token.
pub fn set_current_task(task: Option<TaskToken>) -> Option<TaskToken> {
    CURRENT_TASK.with(|c| c.replace(task))
}

/// Check if the current thread's active task is cancelled.
pub fn is_current_cancelled() -> bool {
    current_task().map(|t| t.is_cancelled()).unwrap_or(false)
}

/// Checkpoint check: returns `Err(error[E-task-0001]: TaskCancelled)` if cancelled.
pub fn check_checkpoint() -> Result<(), String> {
    if is_current_cancelled() {
        Err("error[E-task-0001]: TaskCancelled".to_string())
    } else {
        Ok(())
    }
}

/// Scoped guard restoring previous task token on drop.
pub struct CurrentTaskGuard {
    prev: Option<TaskToken>,
}

impl Drop for CurrentTaskGuard {
    fn drop(&mut self) {
        set_current_task(self.prev.take());
    }
}

/// Push a task token as the active task for the current thread.
pub fn push_current_task(task: TaskToken) -> CurrentTaskGuard {
    let prev = set_current_task(Some(task));
    CurrentTaskGuard { prev }
}

/// Execute a closure with the specified task token as active.
pub fn with_task<F, R>(task: TaskToken, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = push_current_task(task);
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_hierarchy() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = TaskId::root();
        assert_eq!(root.as_str(), "root");

        let c0 = root.child(0);
        assert_eq!(c0.as_str(), "root.0");

        let c0_1 = c0.child(1);
        assert_eq!(c0_1.as_str(), "root.0.1");

        let named = root.child_named("search");
        assert_eq!(named.as_str(), "root.search");
    }

    #[test]
    fn test_task_state_machine() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let t = TaskToken::root();
        assert_eq!(t.state(), TaskState::Pending);

        t.set_state(TaskState::Running);
        assert_eq!(t.state(), TaskState::Running);

        t.set_state(TaskState::Completed);
        assert_eq!(t.state(), TaskState::Completed);

        // Terminal state cannot transition
        t.set_state(TaskState::Running);
        assert_eq!(t.state(), TaskState::Completed);
    }

    #[test]
    fn test_cancellation_propagation_to_children() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = TaskToken::root();
        let child1 = root.child(0);
        let child2 = root.child(1);
        let grandchild = child1.child(0);

        assert!(!root.is_cancelled());
        assert!(!child1.is_cancelled());
        assert!(!grandchild.is_cancelled());

        // Cancel root
        root.cancel();

        assert!(root.is_cancelled());
        assert_eq!(root.state(), TaskState::Cancelled);

        // Children and grandchildren observe cancellation
        assert!(child1.is_cancelled());
        assert_eq!(child1.state(), TaskState::Cancelled);

        assert!(child2.is_cancelled());
        assert_eq!(child2.state(), TaskState::Cancelled);

        assert!(grandchild.is_cancelled());
        assert_eq!(grandchild.state(), TaskState::Cancelled);
    }

    #[test]
    fn test_cooperative_checkpoint() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let t = TaskToken::root();
        assert!(check_checkpoint().is_ok());

        let _guard = push_current_task(t.clone());
        assert!(check_checkpoint().is_ok());

        t.cancel();
        assert_eq!(
            check_checkpoint().unwrap_err(),
            "error[E-task-0001]: TaskCancelled"
        );
    }

    #[test]
    fn test_budget_reservation_released_on_cancel() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        budget_reset();

        unsafe {
            std::env::set_var("NUDGE_BUDGET", "0.0025");
        }

        let t1 = TaskToken::root();
        let _g1 = push_current_task(t1.clone());

        // Reserve 0.0020
        assert!(budget_reserve(0.0020).is_ok());
        let (_, reserved, _) = budget_status();
        assert!((reserved - 0.0020).abs() < 1e-6);

        // Reserving another 0.0010 trips BudgetExceeded
        assert!(budget_reserve(0.0010).is_err());

        // Cancel t1 -> releases reservation immediately
        t1.cancel();
        let (_, reserved_after_cancel, _) = budget_status();
        assert_eq!(reserved_after_cancel, 0.0);

        // Now t2 can reserve 0.0010 without error
        let t2 = TaskToken::new(TaskId::new("root.2"));
        let _g2 = push_current_task(t2.clone());
        assert!(budget_reserve(0.0010).is_ok());

        t2.cancel();
        budget_reset();
        unsafe {
            std::env::remove_var("NUDGE_BUDGET");
        }
    }

    #[test]
    fn test_cancel_pending_task_join_raises_cancelled() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        task_reset();

        let t = TaskToken::new(TaskId::new("root.pending_test"));
        task_register(t.clone());
        assert_eq!(t.state(), TaskState::Pending);

        // Cancel while still pending
        t.cancel();
        assert_eq!(t.state(), TaskState::Cancelled);
        assert!(t.is_cancelled());

        // Join must raise TaskCancelled immediately
        let res = t.join();
        assert_eq!(res.unwrap_err(), "error[E-task-0001]: TaskCancelled");

        task_reset();
    }

    #[test]
    fn test_cancel_twice_is_idempotent_no_panic() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        task_reset();

        let t = TaskToken::new(TaskId::new("root.idempotent_test"));
        t.cancel();
        assert_eq!(t.state(), TaskState::Cancelled);
        assert!(t.is_cancelled());

        // Second cancel should be a clean no-op, no panic, state unchanged
        t.cancel();
        assert_eq!(t.state(), TaskState::Cancelled);
        assert!(t.is_cancelled());

        let res = t.join();
        assert_eq!(res.unwrap_err(), "error[E-task-0001]: TaskCancelled");

        task_reset();
    }

    #[test]
    fn test_cancel_already_completed_task_terminal_state_unchanged() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        task_reset();

        let t = TaskToken::new(TaskId::new("root.completed_test"));
        t.set_state(TaskState::Running);
        t.set_result(Ok(serde_json::json!(42)));
        assert_eq!(t.state(), TaskState::Completed);
        assert!(!t.is_cancelled());

        // Cancelling an already-completed task MUST NOT change terminal state or value
        t.cancel();
        assert_eq!(t.state(), TaskState::Completed);
        assert!(!t.is_cancelled());

        let res = t.join().expect("join on completed task must succeed");
        assert_eq!(res, serde_json::json!(42));

        task_reset();
    }

    #[test]
    fn test_join_unknown_handle_raises_invalid_task_handle() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        task_reset();

        let res = task_join("nonexistent_unknown_handle_9999");
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            err.contains("error[E-task-0003]: InvalidTaskHandle"),
            "expected InvalidTaskHandle error, got: {err}"
        );

        task_reset();
    }

    #[test]
    fn test_shutdown_registry_leak_prevention_after_run_and_task_reset() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        task_reset();
        assert!(task_registry_is_empty());
        assert_eq!(task_registry_count(), 0);

        let t1 = TaskToken::new(TaskId::new("root.task1"));
        let t2 = TaskToken::new(TaskId::new("root.task2"));
        task_register(t1);
        task_register(t2);
        assert_eq!(task_registry_count(), 2);
        assert!(!task_registry_is_empty());

        task_reset();
        assert_eq!(task_registry_count(), 0);
        assert!(task_registry_is_empty());
    }

    #[test]
    fn test_cancelled_task_cannot_reserve_budget_no_false_exceeded_for_siblings() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        budget_reset();
        task_reset();

        unsafe {
            std::env::set_var("NUDGE_BUDGET", "0.0050");
        }

        let root = TaskToken::root();
        task_register(root.clone());
        let t1 = root.child_named("worker1");
        let t2 = root.child_named("worker2");
        task_register(t1.clone());
        task_register(t2.clone());

        // t1 reserves 0.0040 of 0.0050
        {
            let _g1 = push_current_task(t1.clone());
            assert!(budget_reserve(0.0040).is_ok());
        }

        // t2 trying to reserve 0.0030 fails because only 0.0010 is available
        {
            let _g2 = push_current_task(t2.clone());
            assert!(budget_reserve(0.0030).is_err());
        }

        // Cancel t1 -> reservation is released immediately
        t1.cancel();
        assert_eq!(budget_status().1, 0.0);

        // t2 can now reserve 0.0030 without false BudgetExceeded!
        {
            let _g2 = push_current_task(t2.clone());
            assert!(budget_reserve(0.0030).is_ok());
        }

        // Cancelled t1 cannot reserve new budget; returns TaskCancelled, NOT BudgetExceeded
        {
            let _g1 = push_current_task(t1.clone());
            let err = budget_reserve(0.0010).unwrap_err();
            assert_eq!(err, "error[E-task-0001]: TaskCancelled");
        }

        t2.cancel();
        budget_reset();
        task_reset();
        unsafe {
            std::env::remove_var("NUDGE_BUDGET");
        }
    }

    #[test]
    fn test_cancelled_descendants_cannot_reserve_budget() {
        let _lock = TASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        budget_reset();
        task_reset();

        unsafe {
            std::env::set_var("NUDGE_BUDGET", "0.0050");
        }

        let root = TaskToken::root();
        let child = root.child(0);
        let grandchild = child.child(0);

        // Cancel root
        root.cancel();

        // Grandchild observes cancellation and cannot reserve budget
        let _g = push_current_task(grandchild);
        let err = budget_reserve(0.0010).unwrap_err();
        assert_eq!(err, "error[E-task-0001]: TaskCancelled");

        budget_reset();
        task_reset();
        unsafe {
            std::env::remove_var("NUDGE_BUDGET");
        }
    }
}
