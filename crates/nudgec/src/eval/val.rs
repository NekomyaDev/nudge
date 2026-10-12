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
                    write!(f, "{k}: {}", fields[*k])?;
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

/// Parse a CLI argument string into a NudgeVal according to the CLI contract:
/// - "none" or "()" -> Unit
/// - JSON parse first:
///   - "42" -> Int(42)
///   - "3.5" -> Float(3.5)
///   - "true" / "false" -> Bool
///   - '["a","b"]' -> List
///   - '{"k":1}' -> Map
///   - "null" -> Unit
/// - Non-JSON strings -> Str(s)
pub fn parse_cli_arg(s: &str) -> NudgeVal {
    if s == "none" || s == "()" {
        return NudgeVal::Unit;
    }
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
        json_to_nudgeval(v)
    } else {
        NudgeVal::Str(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cli_arg_int() {
        assert_eq!(parse_cli_arg("42"), NudgeVal::Int(42));
        assert_eq!(parse_cli_arg("-10"), NudgeVal::Int(-10));
    }

    #[test]
    fn test_parse_cli_arg_float() {
        assert_eq!(parse_cli_arg("3.5"), NudgeVal::Float(3.5));
        assert_eq!(parse_cli_arg("-2.25"), NudgeVal::Float(-2.25));
    }

    #[test]
    fn test_parse_cli_arg_bool() {
        assert_eq!(parse_cli_arg("true"), NudgeVal::Bool(true));
        assert_eq!(parse_cli_arg("false"), NudgeVal::Bool(false));
    }

    #[test]
    fn test_parse_cli_arg_unit() {
        assert_eq!(parse_cli_arg("none"), NudgeVal::Unit);
        assert_eq!(parse_cli_arg("()"), NudgeVal::Unit);
        assert_eq!(parse_cli_arg("null"), NudgeVal::Unit);
    }

    #[test]
    fn test_parse_cli_arg_list() {
        assert_eq!(
            parse_cli_arg("[\"a\",\"b\"]"),
            NudgeVal::List(vec![NudgeVal::Str("a".into()), NudgeVal::Str("b".into())])
        );
        assert_eq!(
            parse_cli_arg("[1, 2]"),
            NudgeVal::List(vec![NudgeVal::Int(1), NudgeVal::Int(2)])
        );
    }

    #[test]
    fn test_parse_cli_arg_map() {
        let mut expected = HashMap::new();
        expected.insert("k".to_string(), NudgeVal::Int(1));
        assert_eq!(parse_cli_arg("{\"k\":1}"), NudgeVal::Map(expected));
    }

    #[test]
    fn test_parse_cli_arg_string_fallback() {
        assert_eq!(
            parse_cli_arg("urgent: double charged"),
            NudgeVal::Str("urgent: double charged".into())
        );
        assert_eq!(
            parse_cli_arg("test input"),
            NudgeVal::Str("test input".into())
        );
        assert_eq!(parse_cli_arg("hello"), NudgeVal::Str("hello".into()));
    }
}
