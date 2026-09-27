// `nudgec init` — scaffold a project from a template (agents' first run).
// Templates are the repo's own examples: what ships is what people get.

pub const TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "hello",
        "minimal LLM call with a typed schema — start here",
        include_str!("../../../examples/hello_llm.ndg"),
    ),
    (
        "triage",
        "batched decide{} + route{} confidence policy (decision models)",
        include_str!("../../../examples/triage-agent/triage-agent.ndg"),
    ),
    (
        "chatbot",
        "conversational agent with typed memory",
        include_str!("../../../examples/chatbot/chatbot.ndg"),
    ),
    (
        "classifier",
        "classify inputs into a typed label set",
        include_str!("../../../examples/classifier/classifier.ndg"),
    ),
    (
        "code-reviewer",
        "structured code review with scores",
        include_str!("../../../examples/code-reviewer/code-reviewer.ndg"),
    ),
    (
        "data-analyzer",
        "analyze tabular input with typed summaries",
        include_str!("../../../examples/data-analyzer/data-analyzer.ndg"),
    ),
    (
        "rag-agent",
        "retrieval-augmented answering with citations",
        include_str!("../../../examples/rag-agent/rag-agent.ndg"),
    ),
    (
        "research-agent",
        "multi-step research with tools and a report",
        include_str!("../../../examples/research-agent/research-agent.ndg"),
    ),
    (
        "translator",
        "translation with formatting guarantees",
        include_str!("../../../examples/translator/translator.ndg"),
    ),
    (
        "property-fuzz",
        "for_all property tests over generators",
        include_str!("../../../examples/property-fuzz/property-fuzz.ndg"),
    ),
];

pub fn list() {
    println!("templates:");
    for (name, desc, _) in TEMPLATES {
        println!("  {:<14} {}", name, desc);
    }
}

pub fn run(rest: &[String]) -> Result<(), String> {
    run_in(&std::path::PathBuf::from("."), rest)
}

pub fn run_in(base: &std::path::Path, rest: &[String]) -> Result<(), String> {
    let mut name: Option<String> = None;
    let mut template = String::new(); // empty = default "hello"
    let mut force = false;
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--list" => {
                list();
                return Ok(());
            }
            "--template" | "-t" => {
                i += 1;
                template = rest
                    .get(i)
                    .ok_or_else(|| "--template needs a value".to_string())?
                    .clone();
            }
            "--force" | "-f" => force = true,
            other => {
                if name.is_some() {
                    return Err(format!("unexpected argument '{other}'"));
                }
                name = Some(other.to_string());
            }
        }
        i += 1;
    }
    let name = name.ok_or_else(|| {
        "project name required — usage: nudgec init <name> [--template <t>] [--force]\n       nudgec init --list".to_string()
    })?;
    if template.is_empty() {
        template = "hello".to_string();
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        || name.is_empty()
    {
        return Err(format!(
            "invalid project name '{name}' — use lowercase letters, digits, '-' or '_'"
        ));
    }
    let (_, desc, src) = TEMPLATES
        .iter()
        .find(|(n, _, _)| *n == template)
        .ok_or_else(|| format!("unknown template '{template}' — run `nudgec init --list`"))?;

    let dir = base.join(&name);
    if dir.exists()
        && dir
            .read_dir()
            .map(|mut d| d.next().is_some())
            .unwrap_or(false)
    {
        if !force {
            return Err(format!(
                "directory '{name}' already exists and is not empty — use --force to overwrite"
            ));
        }
    } else {
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create '{name}': {e}"))?;
    }

    let program_path = dir.join(format!("{name}.ndg"));
    std::fs::write(&program_path, src).map_err(|e| format!("cannot write {name}.ndg: {e}"))?;
    let readme = format!(
        "# {name}\n\nScaffolded by `nudgec init --template {template}` — {desc}\n\n\
         The full example lives in [`examples/{tpl}`](../examples/{tpl}/) in the Nudge repo.\n\n\
         ## Commands\n\n```sh\n\
         nudgec check {name}.ndg    # type-check\n\
         nudgec build {name}.ndg    # compile to out/{name}.py\n\
         nudgec test  {name}.ndg    # run test blocks — $0, no API key\n\
         python3 out/{name}.py      # run (fake provider: deterministic, offline)\n\
         ```\n\n\
         Everything runs against a deterministic fake provider by default — no API key, no token spend.\n\
         Point a live LLM provider with `NUDGE_PROVIDER`/keys; for decision templates set\n\
         `NUDGE_DECISION_SERVERS` (see docs/decision.md).\n",
        name = name,
        tpl = template,
        desc = desc
    );
    std::fs::write(dir.join("README.md"), readme)
        .map_err(|e| format!("cannot write README.md: {e}"))?;
    println!("created {name}/ from template '{template}':");
    println!("  {name}.ndg");
    println!("  README.md");
    println!("next:");
    println!("  cd {name} && nudgec check {name}.ndg && nudgec build {name}.ndg");
    println!("  nudgec init --list    # see all templates");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("nudgec_init_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn templates_are_complete_programs_with_helpful_names() {
        assert!(TEMPLATES.len() >= 8);
        assert_eq!(TEMPLATES[0].0, "hello");
        for (name, desc, src) in TEMPLATES {
            assert!(!desc.is_empty(), "{name}: empty description");
            assert!(src.contains("fn "), "{name}: template has no fn");
            assert!(!src.contains('\r'), "{name}: CRLF line endings");
        }
    }

    #[test]
    fn init_scaffolds_files_from_a_template() {
        let base = tmpdir("scaffold");
        let rest = vec![
            "demo".to_string(),
            "--template".to_string(),
            "triage".to_string(),
        ];
        run_in(&base, &rest).unwrap();
        let program = base.join("demo/demo.ndg");
        assert!(program.exists(), "program file written");
        let readme = std::fs::read_to_string(base.join("demo/README.md")).unwrap();
        assert!(readme.contains("nudgec check demo.ndg"));
        assert!(readme.contains("decision models"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn init_rejects_bad_names_and_unknown_templates_and_filled_dirs() {
        let base = tmpdir("reject");
        assert!(run_in(&base, &["Bad Name".to_string()]).is_err());
        assert!(run_in(
            &base,
            &[
                "ok".to_string(),
                "--template".to_string(),
                "nope".to_string()
            ]
        )
        .is_err());
        assert!(run_in(&base, &[]).is_err());
        std::fs::create_dir_all(base.join("full")).unwrap();
        std::fs::write(base.join("full/something"), "x").unwrap();
        assert!(run_in(&base, &["full".to_string()]).is_err());
        // --force overwrites anyway
        assert!(run_in(&base, &["full".to_string(), "--force".to_string()]).is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }
}
