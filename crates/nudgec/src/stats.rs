//! Code statistics reporter for Nudge files.
//! Analyzes AST nodes, line counts, effects, functions, and complexity.

use crate::ast::*;
use crate::json;
use std::collections::HashSet;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NudgeStats {
    pub file_path: String,
    pub total_lines: usize,
    pub blank_lines: usize,
    pub comment_lines: usize,
    pub code_lines: usize,
    pub function_count: usize,
    pub agent_count: usize,
    pub view_count: usize,
    pub type_alias_count: usize,
    pub test_count: usize,
    pub effect_count: usize,
    pub effects_used: Vec<String>,
    pub total_expressions: usize,
    pub total_statements: usize,
    pub cyclomatic_complexity: usize,
}

impl NudgeStats {
    pub fn to_json(&self) -> String {
        let effects_arr = self
            .effects_used
            .iter()
            .map(|e| format!("\"{}\"", json::escape(e)))
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "{{\n  \"file\": \"{}\",\n  \"lines\": {{\n    \"total\": {},\n    \"code\": {},\n    \"blank\": {},\n    \"comment\": {}\n  }},\n  \"declarations\": {{\n    \"functions\": {},\n    \"agents\": {},\n    \"views\": {},\n    \"type_aliases\": {},\n    \"tests\": {}\n  }},\n  \"effects\": {{\n    \"count\": {},\n    \"list\": [{}]\n  }},\n  \"ast\": {{\n    \"expressions\": {},\n    \"statements\": {},\n    \"cyclomatic_complexity\": {}\n  }}\n}}",
            json::escape(&self.file_path),
            self.total_lines,
            self.code_lines,
            self.blank_lines,
            self.comment_lines,
            self.function_count,
            self.agent_count,
            self.view_count,
            self.type_alias_count,
            self.test_count,
            self.effect_count,
            effects_arr,
            self.total_expressions,
            self.total_statements,
            self.cyclomatic_complexity,
        )
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Statistics for {}\n", self.file_path));
        out.push_str("----------------------------------------\n");
        out.push_str("Lines of Code:\n");
        out.push_str(&format!("  Total Lines:       {}\n", self.total_lines));
        out.push_str(&format!("  Code Lines:        {}\n", self.code_lines));
        out.push_str(&format!("  Comment Lines:     {}\n", self.comment_lines));
        out.push_str(&format!("  Blank Lines:       {}\n", self.blank_lines));
        out.push_str("Declarations:\n");
        out.push_str(&format!("  Functions:         {}\n", self.function_count));
        out.push_str(&format!("  Agents:            {}\n", self.agent_count));
        out.push_str(&format!("  Views:             {}\n", self.view_count));
        out.push_str(&format!("  Type Aliases:      {}\n", self.type_alias_count));
        out.push_str(&format!("  Tests:             {}\n", self.test_count));
        out.push_str("Effects:\n");
        out.push_str(&format!("  Unique Effects:    {}\n", self.effect_count));
        if !self.effects_used.is_empty() {
            out.push_str(&format!(
                "  List:              {}\n",
                self.effects_used.join(", ")
            ));
        }
        out.push_str("AST Complexity:\n");
        out.push_str(&format!(
            "  Expressions:       {}\n",
            self.total_expressions
        ));
        out.push_str(&format!("  Statements:        {}\n", self.total_statements));
        out.push_str(&format!(
            "  Cyclomatic Complexity: {}\n",
            self.cyclomatic_complexity
        ));
        out
    }
}

pub fn analyze(file_path: &str, src: &str, items: &[Item]) -> NudgeStats {
    let mut stats = NudgeStats {
        file_path: file_path.to_string(),
        cyclomatic_complexity: 1, // Base complexity
        ..Default::default()
    };

    // 1. Line statistics
    for line in src.lines() {
        stats.total_lines += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            stats.blank_lines += 1;
        } else if trimmed.starts_with("//") {
            stats.comment_lines += 1;
        } else {
            stats.code_lines += 1;
        }
    }

    // 2. Declarations and effects
    let mut effects = HashSet::new();

    for item in items {
        analyze_item(item, &mut stats, &mut effects);
    }

    let mut eff_vec: Vec<String> = effects.into_iter().collect();
    eff_vec.sort();
    stats.effect_count = eff_vec.len();
    stats.effects_used = eff_vec;

    stats
}

fn analyze_item(item: &Item, stats: &mut NudgeStats, effects: &mut HashSet<String>) {
    match item {
        Item::Fn {
            effects: effs,
            body,
            ..
        } => {
            stats.function_count += 1;
            for eff in effs {
                effects.insert(eff.clone());
            }
            for stmt in body {
                walk_stmt(stmt, stats);
            }
        }
        Item::Agent { fns, .. } => {
            stats.agent_count += 1;
            for agent_item in fns {
                analyze_item(agent_item, stats, effects);
            }
        }
        Item::View { body, .. } => {
            stats.view_count += 1;
            for stmt in body {
                walk_stmt(stmt, stats);
            }
        }
        Item::TypeAlias { .. } => {
            stats.type_alias_count += 1;
        }
        Item::Test { body, .. } => {
            stats.test_count += 1;
            for stmt in body {
                walk_stmt(stmt, stats);
            }
        }
        Item::Tool { .. }
        | Item::Window { .. }
        | Item::AuthProvider { .. }
        | Item::Account { .. } => {}
    }
}

fn walk_stmt(stmt: &Stmt, stats: &mut NudgeStats) {
    stats.total_statements += 1;

    match &stmt.kind {
        StmtKind::Let { value, .. } => {
            walk_expr(value, stats);
        }
        StmtKind::Assign { value, .. } => {
            walk_expr(value, stats);
        }
        StmtKind::StateWrite { value, .. } => {
            walk_expr(value, stats);
        }
        StmtKind::Assert(expr) => {
            walk_expr(expr, stats);
        }
        StmtKind::ExprStmt(expr) => {
            walk_expr(expr, stats);
        }
        StmtKind::ForAll { args, body, .. } => {
            stats.cyclomatic_complexity += 1;
            for arg in args {
                walk_expr(arg, stats);
            }
            for s in body {
                walk_stmt(s, stats);
            }
        }
        StmtKind::For { coll, body, .. } => {
            stats.cyclomatic_complexity += 1;
            walk_expr(coll, stats);
            for s in body {
                walk_stmt(s, stats);
            }
        }
        StmtKind::If {
            cond,
            then_body,
            else_body,
        } => {
            stats.cyclomatic_complexity += 1;
            walk_expr(cond, stats);
            for s in then_body {
                walk_stmt(s, stats);
            }
            if let Some(el) = else_body {
                for s in el {
                    walk_stmt(s, stats);
                }
            }
        }
    }
}

fn walk_expr(expr: &Expr, stats: &mut NudgeStats) {
    stats.total_expressions += 1;

    match &expr.kind {
        ExprKind::Block(stmts) => {
            for stmt in stmts {
                walk_stmt(stmt, stats);
            }
        }
        ExprKind::If { cond, then, else_ } => {
            stats.cyclomatic_complexity += 1;
            walk_expr(cond, stats);
            walk_expr(then, stats);
            if let Some(el) = else_ {
                walk_expr(el, stats);
            }
        }
        ExprKind::Binary { op, l, r } => {
            if *op == BinOp::And || *op == BinOp::Or {
                stats.cyclomatic_complexity += 1;
            }
            walk_expr(l, stats);
            walk_expr(r, stats);
        }
        ExprKind::Unary { x, .. } => {
            walk_expr(x, stats);
        }
        ExprKind::Call { func, args, kwargs } => {
            walk_expr(func, stats);
            for arg in args {
                walk_expr(arg, stats);
            }
            for (_, kw_expr) in kwargs {
                walk_expr(kw_expr, stats);
            }
        }
        ExprKind::Route { arms } => {
            for (_, body, cond) in arms {
                if let Some(c) = cond {
                    stats.cyclomatic_complexity += 1;
                    walk_expr(c, stats);
                }
                walk_expr(body, stats);
            }
        }
        ExprKind::DecideCall {
            state,
            options,
            questions,
        } => {
            stats.cyclomatic_complexity += questions.len();
            walk_expr(state, stats);
            for (_, opt_expr) in options {
                walk_expr(opt_expr, stats);
            }
        }
        ExprKind::ParMap {
            coll, body, kwargs, ..
        } => {
            walk_expr(coll, stats);
            walk_expr(body, stats);
            for (_, kw) in kwargs {
                walk_expr(kw, stats);
            }
        }
        ExprKind::ParAll(items) | ExprKind::ParRace(items) | ExprKind::ListLit(items) => {
            for item in items {
                walk_expr(item, stats);
            }
        }
        ExprKind::Merge { l, r } => {
            walk_expr(l, stats);
            walk_expr(r, stats);
        }
        ExprKind::RecordLit(fields) => {
            for (_, v) in fields {
                walk_expr(v, stats);
            }
        }
        ExprKind::Index { obj, idx } => {
            walk_expr(obj, stats);
            walk_expr(idx, stats);
        }
        ExprKind::Field { obj, .. } => {
            walk_expr(obj, stats);
        }
        ExprKind::Spawn(e) => {
            walk_expr(e, stats);
        }
        ExprKind::ComputerCall { args, kwargs, .. } => {
            for arg in args {
                walk_expr(arg, stats);
            }
            for (_, kw) in kwargs {
                walk_expr(kw, stats);
            }
        }
        ExprKind::LlmCall {
            prompt, options, ..
        } => {
            walk_expr(prompt, stats);
            for (_, opt) in options {
                walk_expr(opt, stats);
            }
        }
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Str(_)
        | ExprKind::Money(_, _)
        | ExprKind::Bool(_)
        | ExprKind::None
        | ExprKind::Ident(_)
        | ExprKind::Prompt { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile;

    #[test]
    fn test_stats_simple_fn() {
        let src = r#"
// A comment
fn add(a: int, b: int) -> int {
    if a > 0 and b > 0 {
        a + b
    } else {
        0
    }
}
"#;
        let items = compile::compile(src).expect("compile should succeed");
        let stats = analyze("test.ndg", src, &items);

        assert_eq!(stats.function_count, 1);
        assert_eq!(stats.agent_count, 0);
        assert_eq!(stats.comment_lines, 1);
        // Base 1 + if (1) + && (1) = 3
        assert_eq!(stats.cyclomatic_complexity, 3);

        let json = stats.to_json();
        assert!(json.contains("\"functions\": 1"));
        assert!(json.contains("\"cyclomatic_complexity\": 3"));
    }
}
