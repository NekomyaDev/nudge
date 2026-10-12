use std::collections::HashMap;
use std::fmt;

/// Runtime value representation for the Nudge native interpreter (RFC §4.1).
#[derive(Debug, Clone, PartialEq)]
pub enum NudgeVal {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<NudgeVal>),
    Map(HashMap<String, NudgeVal>),
    AgentState {
        name: String,
        fields: HashMap<String, NudgeVal>,
    },
}

impl NudgeVal {
    /// Evaluates the truthiness of the value.
    pub fn is_truthy(&self) -> bool {
        match self {
            NudgeVal::Unit => false,
            NudgeVal::Bool(b) => *b,
            NudgeVal::Int(i) => *i != 0,
            NudgeVal::Float(f) => *f != 0.0 && !f.is_nan(),
            NudgeVal::Str(s) => !s.is_empty(),
            NudgeVal::List(xs) => !xs.is_empty(),
            NudgeVal::Map(m) => !m.is_empty(),
            NudgeVal::AgentState { .. } => true,
        }
    }

    /// User-facing type name for diagnostics.
    pub fn type_name(&self) -> &'static str {
        match self {
            NudgeVal::Unit => "unit",
            NudgeVal::Bool(_) => "bool",
            NudgeVal::Int(_) => "int",
            NudgeVal::Float(_) => "float",
            NudgeVal::Str(_) => "string",
            NudgeVal::List(_) => "list",
            NudgeVal::Map(_) => "map",
            NudgeVal::AgentState { .. } => "agent",
        }
    }

    /// Number comparison supporting mixed int / float equality.
    pub fn loose_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (NudgeVal::Int(a), NudgeVal::Float(b)) => (*a as f64) == *b,
            (NudgeVal::Float(a), NudgeVal::Int(b)) => *a == (*b as f64),
            (a, b) => a == b,
        }
    }
}

impl fmt::Display for NudgeVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NudgeVal::Unit => write!(f, "()"),
            NudgeVal::Bool(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            NudgeVal::Int(i) => write!(f, "{i}"),
            NudgeVal::Float(fl) => write!(f, "{fl}"),
            NudgeVal::Str(s) => write!(f, "{s}"),
            NudgeVal::List(xs) => {
                write!(f, "[")?;
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    if let NudgeVal::Str(s) = x {
                        write!(f, "\"{s}\"")?;
                    } else {
                        write!(f, "{x}")?;
                    }
                }
                write!(f, "]")
            }
            NudgeVal::Map(m) => {
                write!(f, "{{")?;
                let mut keys: Vec<&String> = m.keys().collect();
                keys.sort();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    let val = &m[*k];
                    if let NudgeVal::Str(s) = val {
                        write!(f, "\"{k}\": \"{s}\"")?;
                    } else {
                        write!(f, "\"{k}\": {val}")?;
                    }
                }
                write!(f, "}}")
            }
            NudgeVal::AgentState { name, fields } => {
                write!(f, "agent {name} {{")?;
                let mut keys: Vec<&String> = fields.keys().collect();
                keys.sort();
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {}", &fields[*k])?;
                }
                write!(f, "}}")
            }
        }
    }
}

/// Convert a serde_json::Value to NudgeVal recursively.
pub fn json_to_nudgeval(v: serde_json::Value) -> NudgeVal {
    match v {
        serde_json::Value::Null => NudgeVal::Unit,
        serde_json::Value::Bool(b) => NudgeVal::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                NudgeVal::Int(i)
            } else if let Some(f) = n.as_f64() {
                NudgeVal::Float(f)
            } else {
                NudgeVal::Float(0.0)
            }
        }
        serde_json::Value::String(s) => NudgeVal::Str(s),
        serde_json::Value::Array(arr) => {
            NudgeVal::List(arr.into_iter().map(json_to_nudgeval).collect())
        }
        serde_json::Value::Object(obj) => {
            let mut map = HashMap::new();
            for (k, val) in obj {
                map.insert(k, json_to_nudgeval(val));
            }
            NudgeVal::Map(map)
        }
    }
}
