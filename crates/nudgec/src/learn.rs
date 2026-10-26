// `nudgec learn` - a pure terminal tutorial runner.
// All lesson metadata, explanations, and executable code live natively in
// `docs/learn/<name>.ndg` as idiomatic Nudge source with doc-comments.
// Rust contains zero hardcoded lesson strings.

pub struct Lesson {
    pub name: &'static str,
    /// Raw `.ndg` source containing doc comments and executable code.
    pub source: &'static str,
}

impl Lesson {
    pub const fn from_source(name: &'static str, source: &'static str) -> Self {
        Self { name, source }
    }

    /// Extracts the lesson title declared in the first doc comment line (`/// # <Title>`).
    pub fn title(&self) -> &str {
        for line in self.source.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("/// # ") {
                return rest.trim();
            }
        }
        self.name
    }

    /// Extracts the documentation body from doc comments (`///`).
    pub fn body(&self) -> String {
        let mut out = Vec::new();
        let mut skipped_title = false;
        for line in self.source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("///") {
                let text = trimmed.strip_prefix("///").unwrap();
                let text = text.strip_prefix(' ').unwrap_or(text);
                if !skipped_title && text.starts_with("# ") {
                    skipped_title = true;
                    continue;
                }
                out.push(text);
            } else if !trimmed.is_empty() {
                break;
            }
        }
        out.join("\n").trim().to_string()
    }

    /// Extracts the executable Nudge program portion (excluding leading doc comments).
    pub fn program(&self) -> Option<String> {
        let mut out = Vec::new();
        let mut found_code = false;
        for line in self.source.lines() {
            let trimmed = line.trim();
            if !found_code {
                if trimmed.starts_with("///") || trimmed.is_empty() {
                    continue;
                }
                found_code = true;
            }
            out.push(line);
        }
        if found_code && !out.is_empty() {
            Some(out.join("\n").trim_end().to_string())
        } else {
            None
        }
    }
}

// Generated at build time directly from docs/learn/*.ndg
include!(concat!(env!("OUT_DIR"), "/learn_curriculum.rs"));

pub fn all_lessons() -> &'static [Lesson] {
    LESSONS
}

pub fn run(rest: &[String]) -> Result<(), String> {
    let lessons = LESSONS;
    if rest.is_empty() {
        println!(
            "nudge learn - complete interactive language curriculum ({} lessons)\n",
            lessons.len()
        );
        for l in lessons {
            println!("  nudgec learn {:<22} {}", l.name, l.title());
        }
        println!("\nstart with: nudgec learn hello");
        return Ok(());
    }
    let name = rest[0].as_str();
    let lesson = lessons
        .iter()
        .find(|l| l.name == name)
        .ok_or_else(|| format!("unknown lesson '{name}' — run `nudgec learn` for the list"))?;

    println!("=== {} ===\n", lesson.title());
    let body = lesson.body();
    if !body.is_empty() {
        println!("{}", body);
    }
    if let Some(program) = lesson.program() {
        println!("\n--- {}.ndg ---\n{}", name, program);
        println!("\nsave it as {name}.ndg and run the Try commands above.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lesson_programs_type_check_against_the_shipping_compiler() {
        for l in LESSONS {
            let tokens = crate::lexer::lex(l.source)
                .unwrap_or_else(|e| panic!("lesson '{}' failed to lex: {e:?}", l.name));
            let items = crate::parser::parse(tokens)
                .unwrap_or_else(|e| panic!("lesson '{}' failed to parse: {e:?}", l.name));
            let errs = crate::check::check(&items);
            assert!(
                errs.is_empty(),
                "lesson '{}' does not compile: {:?}",
                l.name,
                errs.iter().map(|e| &e.msg).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn total_curriculum_contains_one_hundred_forty_unique_lessons_and_helpful_titles() {
        assert_eq!(LESSONS.len(), 140);
        let mut names: Vec<_> = LESSONS.iter().map(|l| l.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 140, "every lesson name must be unique");

        for (i, l) in LESSONS.iter().enumerate() {
            let expected_prefix = format!("{}/140 - ", i + 1);
            let title = l.title();
            assert!(
                title.starts_with(&expected_prefix),
                "lesson '{}' title '{}' should start with '{expected_prefix}'",
                l.name,
                title
            );
            let body = l.body();
            assert!(body.contains("Try:") || body.contains("know the whole loop"));
        }
    }

    #[test]
    fn run_lists_and_shows_lessons() {
        assert!(run(&[]).is_ok());
        assert!(run(&["hello".to_string()]).is_ok());
        assert!(run(&["primitives".to_string()]).is_ok());
        assert!(run(&["structured_schemas".to_string()]).is_ok());
        assert!(run(&["decide_syntax".to_string()]).is_ok());
        assert!(run(&["a2a_agent_card".to_string()]).is_ok());
        assert!(run(&["compilation_targets".to_string()]).is_ok());
        assert!(run(&["a2a_task_lifecycle".to_string()]).is_ok());
        assert!(run(&["state_persistence".to_string()]).is_ok());
        assert!(run(&["mcp_resource_reading".to_string()]).is_ok());
        assert!(run(&["mastery_capstone".to_string()]).is_ok());
        assert!(run(&["ui_reactive_state".to_string()]).is_ok());
        assert!(run(&["formal_preconditions".to_string()]).is_ok());
        assert!(run(&["bytecode_opcodes".to_string()]).is_ok());
        assert!(run(&["grand_master_architect".to_string()]).is_ok());
        assert!(run(&["nope".to_string()]).is_err());
    }
}
