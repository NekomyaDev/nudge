use crate::eval::val::NudgeVal;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// A variable binding with its mutability flag.
#[derive(Debug, Clone)]
pub struct Binding {
    pub val: NudgeVal,
    pub mutable: bool,
}

/// Lexical scope environment with parent pointer for variable lookups.
#[derive(Debug, Clone)]
pub struct Env {
    bindings: HashMap<String, Binding>,
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

    /// Create an environment populated with initial bindings.
    pub fn from_bindings(bindings: HashMap<String, Binding>) -> Self {
        Self {
            bindings,
            parent: None,
        }
    }

    /// Define or overwrite an immutable binding in the current local scope.
    pub fn define(&mut self, name: String, val: NudgeVal) {
        self.bindings.insert(
            name,
            Binding {
                val,
                mutable: false,
            },
        );
    }

    /// Define a binding with explicit mutability.
    pub fn define_mut(&mut self, name: String, val: NudgeVal, mutable: bool) {
        self.bindings.insert(name, Binding { val, mutable });
    }

    /// Look up a variable by name walking up the scope chain.
    pub fn get(&self, name: &str) -> Option<NudgeVal> {
        if let Some(b) = self.bindings.get(name) {
            return Some(b.val.clone());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow().get(name);
        }
        None
    }

    /// Reassign an existing variable in the scope chain.
    /// Returns error if variable is immutable or undefined.
    pub fn assign(&mut self, name: &str, val: NudgeVal) -> Result<(), String> {
        if let Some(b) = self.bindings.get_mut(name) {
            if !b.mutable {
                return Err(format!(
                    "cannot assign to immutable local '{name}'; declare it with `let mut`"
                ));
            }
            b.val = val;
            return Ok(());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow_mut().assign(name, val);
        }
        Err(format!("unknown identifier '{name}'"))
    }

    /// Mutate an existing mutable variable in place.
    pub fn mutate<F, R>(&mut self, name: &str, f: F) -> Result<R, String>
    where
        F: FnOnce(&mut NudgeVal) -> Result<R, String>,
    {
        if let Some(b) = self.bindings.get_mut(name) {
            if !b.mutable {
                return Err(format!(
                    "cannot mutate immutable local '{name}'; declare it with `let mut`"
                ));
            }
            return f(&mut b.val);
        }
        if let Some(parent) = &self.parent {
            return parent.borrow_mut().mutate(name, f);
        }
        Err(format!("unknown identifier '{name}'"))
    }

    /// Snapshot all visible bindings into a flat, independent environment.
    pub fn snapshot(&self) -> Self {
        Self::from_bindings(self.snapshot_bindings())
    }

    /// Snapshot all visible bindings into a flat HashMap.
    pub fn snapshot_bindings(&self) -> HashMap<String, Binding> {
        let mut flat_bindings = HashMap::new();
        if let Some(parent) = &self.parent {
            let parent_snapshot = parent.borrow().snapshot_bindings();
            flat_bindings.extend(parent_snapshot);
        }
        flat_bindings.extend(self.bindings.clone());
        flat_bindings
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
