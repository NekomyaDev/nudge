// The recipe book (docs/recipes/) — small, copy-pasteable patterns.
// Every recipe is embedded here and type-checked by a test against the
// compiler shipping it: a recipe that stops compiling breaks CI, so the
// book can never drift from the grammar (same rule as `nudgec learn`).

#[allow(dead_code)] // referenced by the drift-lock test; future: `nudgec recipe`
const RECIPES: &[(&str, &str)] = &[
    (
        "typed-extraction",
        include_str!("../../../docs/recipes/typed-extraction.ndg"),
    ),
    (
        "fallback-model",
        include_str!("../../../docs/recipes/fallback-model.ndg"),
    ),
    (
        "human-escalation",
        include_str!("../../../docs/recipes/human-escalation.ndg"),
    ),
    (
        "injection-guard",
        include_str!("../../../docs/recipes/injection-guard.ndg"),
    ),
    (
        "cost-capped-call",
        include_str!("../../../docs/recipes/cost-capped-call.ndg"),
    ),
    ("par-map", include_str!("../../../docs/recipes/par-map.ndg")),
];

#[cfg(test)]
mod tests {
    use super::RECIPES;

    #[test]
    fn every_recipe_type_checks() {
        for (name, src) in RECIPES {
            let tokens = crate::lexer::lex(src).unwrap();
            let items = crate::parser::parse(tokens).unwrap();
            let errs = crate::check::check(&items);
            assert!(
                errs.is_empty(),
                "recipe '{name}' does not compile: {:?}",
                errs.iter().map(|e| &e.msg).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn recipes_dir_files_match_recipes_constant() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let recipes_dir = manifest_dir.join("../../docs/recipes");
        let mut on_disk = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&recipes_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("ndg") {
                    if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                        on_disk.push(stem.to_string());
                    }
                }
            }
        }
        on_disk.sort();
        let mut in_const: Vec<String> = RECIPES.iter().map(|(n, _)| (*n).to_string()).collect();
        in_const.sort();
        assert_eq!(
            on_disk, in_const,
            "recipes list in RECIPES constant must match files on disk in docs/recipes/*.ndg"
        );
    }
}
