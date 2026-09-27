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
}
