use crate::eval::val::NudgeVal;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Lexical scope environment with parent pointer for variable lookups.
#[derive(Debug, Clone)]
pub struct Env {
    bindings: HashMap<String, NudgeVal>,
    parent: Option<Rc<RefCell<Env>>>,
}

impl Env {
    /// Create a new root environment.
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            parent: None,
        }
    }

    /// Create a child environment pointing to a parent scope.
    pub fn new_child(parent: Rc<RefCell<Env>>) -> Self {
        Self {
            bindings: HashMap::new(),
            parent: Some(parent),
        }
    }

    /// Define or overwrite an immutable binding in the current local scope.
    pub fn define(&mut self, name: String, val: NudgeVal) {
        self.bindings.insert(name, val);
    }

    /// Look up a variable by name walking up the scope chain.
    pub fn get(&self, name: &str) -> Option<NudgeVal> {
        if let Some(val) = self.bindings.get(name) {
            return Some(val.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().get(name);
        }
        None
    }
}

/// Dedicated state storage and mutation tracking for Nudge agents.
#[derive(Debug, Clone, Default)]
pub struct AgentStore {
    states: HashMap<String, HashMap<String, NudgeVal>>,
}

impl AgentStore {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    /// Initialize an agent with its declared state fields and default values.
    pub fn init_agent(&mut self, agent: &str, fields: HashMap<String, NudgeVal>) {
        self.states.insert(agent.to_string(), fields);
    }

    /// Read a state field value.
    pub fn get_field(&self, agent: &str, field: &str) -> Result<NudgeVal, String> {
        let fields = self
            .states
            .get(agent)
            .ok_or_else(|| format!("error[E0701]: unknown agent '{agent}'"))?;
        fields
            .get(field)
            .cloned()
            .ok_or_else(|| format!("error[E0701]: agent '{agent}' has no state field '{field}'"))
    }

    /// Mutate a state field value.
    pub fn set_field(&mut self, agent: &str, field: &str, val: NudgeVal) -> Result<(), String> {
        let fields = self
            .states
            .get_mut(agent)
            .ok_or_else(|| format!("error[E0701]: unknown agent '{agent}'"))?;
        fields.insert(field.to_string(), val);
        Ok(())
    }

    /// Snapshot all fields for an agent (used when `state` is read as a whole value).
    pub fn snapshot(&self, agent: &str) -> Result<HashMap<String, NudgeVal>, String> {
        self.states
            .get(agent)
            .cloned()
            .ok_or_else(|| format!("error[E0701]: unknown agent '{agent}'"))
    }
}
