//! A2A agent-card export (design §9, v1.0): `nudgec a2a <file.ndg>` emits
//! `out/<name>.agent.json` — one card per `agent` block, or a single card
//! wrapping the top-level fns when the file declares none. Cards follow the
//! A2A agent-card shape (name/description/version/url/capabilities/skills).

use crate::ast::*;
use crate::json::Json;

fn ty_name(t: &TypeExpr) -> String {
    match t {
        TypeExpr::Named(n) => n.clone(),
        TypeExpr::List(inner) => format!("[{}]", ty_name(inner)),
        TypeExpr::Record(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", ty_name(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypeExpr::Refine(inner, name, _) => format!("{} @{name}(…)", ty_name(inner)),
    }
}

fn skill(name: &str, params: &[Param], ret: &TypeExpr, effects: &[String]) -> Json {
    let sig = format!(
        "{}({}) -> {}{}",
        name,
        params
            .iter()
            .map(|p| format!("{}: {}", p.name, ty_name(&p.ty)))
            .collect::<Vec<_>>()
            .join(", "),
        ty_name(ret),
        if effects.is_empty() {
            String::new()
        } else {
            format!(" uses {}", effects.join(", "))
        },
    );
    Json::Obj(vec![
        ("id".into(), Json::str(name)),
        ("name".into(), Json::str(name)),
        ("description".into(), Json::str(format!("Nudge fn `{sig}`"))),
        (
            "tags".into(),
            Json::Arr(if effects.is_empty() {
                vec![Json::str("pure")]
            } else {
                effects
                    .iter()
                    .map(|e| Json::str(e.to_lowercase()))
                    .collect()
            }),
        ),
    ])
}

fn card(name: &str, skills: Vec<Json>) -> Json {
    Json::Obj(vec![
        ("name".into(), Json::str(name)),
        (
            "description".into(),
            Json::str(format!(
                "Nudge agent '{name}' — compiled from .ndg (https://github.com/NekomyaDev/nudge)"
            )),
        ),
        ("version".into(), Json::str("1.0.0")),
        ("url".into(), Json::str("http://localhost:9999/")),
        (
            "capabilities".into(),
            Json::Obj(vec![
                ("streaming".into(), Json::Bool(false)),
                ("pushNotifications".into(), Json::Bool(false)),
                ("stateTransitionHistory".into(), Json::Bool(true)),
            ]),
        ),
        (
            "defaultInputModes".into(),
            Json::Arr(vec![Json::str("text")]),
        ),
        (
            "defaultOutputModes".into(),
            Json::Arr(vec![Json::str("text")]),
        ),
        ("skills".into(), Json::Arr(skills)),
    ])
}

/// `(file_stem, card)` pairs — one per agent block, or one for the file.
pub fn cards(items: &[Item], file_stem: &str) -> Vec<(String, Json)> {
    let mut out = Vec::new();
    for item in items {
        if let Item::Agent { name, fns, .. } = item {
            let skills = fns
                .iter()
                .filter_map(|f| match f {
                    Item::Fn {
                        name,
                        params,
                        ret,
                        effects,
                        ..
                    } => Some(skill(name, params, ret, effects)),
                    _ => None,
                })
                .collect();
            out.push((name.to_lowercase(), card(name, skills)));
        }
    }
    if out.is_empty() {
        let skills = items
            .iter()
            .filter_map(|f| match f {
                Item::Fn {
                    name,
                    params,
                    ret,
                    effects,
                    ..
                } => Some(skill(name, params, ret, effects)),
                _ => None,
            })
            .collect();
        out.push((file_stem.to_string(), card(file_stem, skills)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::dumps;
    use crate::lexer::lex;
    use crate::parser::parse;

    #[test]
    fn agent_block_exports_an_a2a_card() {
        let src = "agent Researcher {\n    state {\n        round: int = 0,\n    }\n    fn step(q: string) -> string uses LLM {\n        llm\"\"\"go {q}\"\"\" with { model: \"m\" }\n    }\n}";
        let items = parse(lex(src).unwrap()).unwrap();
        let cs = cards(&items, "demo");
        assert_eq!(cs.len(), 1);
        let (stem, c) = &cs[0];
        assert_eq!(stem, "researcher");
        let s = dumps(c);
        assert!(s.contains("\"name\": \"Researcher\""), "{s}");
        assert!(s.contains("\"version\": \"1.0.0\""), "{s}");
        assert!(s.contains("\"stateTransitionHistory\": true"), "{s}");
        assert!(s.contains("\"id\": \"step\""), "{s}");
        assert!(s.contains("step(q: string) -> string uses LLM"), "{s}");
        assert!(s.contains("\"tags\": [\"llm\"]"), "{s}");
        // the emitted card is valid JSON
        assert!(crate::json::parse(&s).unwrap().is_obj());
    }

    #[test]
    fn a_file_without_agents_exports_one_card_for_its_fns() {
        let src = "fn run(q: string) -> [string] uses Tool {\n    web(q)\n}";
        let items = parse(lex(src).unwrap()).unwrap();
        let cs = cards(&items, "research_agent");
        assert_eq!(cs.len(), 1);
        let s = dumps(&cs[0].1);
        assert!(s.contains("\"name\": \"research_agent\""), "{s}");
        assert!(s.contains("run(q: string) -> [string] uses Tool"), "{s}");
        assert!(s.contains("\"defaultInputModes\": [\"text\"]"), "{s}");
    }
}
