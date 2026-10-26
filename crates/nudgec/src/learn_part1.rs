//! Part 1 of the `nudgec learn` curriculum: core language and type system.
//!
//! This module deliberately documents the syntax that ships today. Where the
//! wider curriculum names a future construct, the lesson gives the checked,
//! current-language equivalent instead of presenting non-compiling code.

use crate::learn::Lesson;

pub const LESSONS_PART1: &[Lesson] = &[
    Lesson {
        name: "variables_mut",
        title: "9/22 - Variables and mutable state",
        body: r#"`let` creates an immutable local binding. Add `mut` only when a value
must be reassigned with `=`. This keeps ordinary data flow explicit and lets
the checker reject accidental mutation.

Try:
  nudgec check variables_mut.ndg
  nudgec build variables_mut.ndg"#,
        program: Some(r#"fn count_items() -> int {
    let mut total: int = 0
    total = total + 1
    total = total + 2
    total
}

fn main() -> int {
    count_items()
}
"#),
    },
    Lesson {
        name: "primitives",
        title: "10/22 - Primitive types",
        body: r#"Nudge has `int`, `float`, `string`, `bool`, and `none` (the unit-like
no-value type). `()` is also accepted in type position as an alias for `none`.
Numeric operations widen `int` to `float` when necessary. There are no implicit
string-to-number cast builtins yet, so make conversions at an input boundary.

Try:
  nudgec check primitives.ndg"#,
        program: Some(r#"fn describe(enabled: bool) -> string {
    let count: int = 3
    let ratio: float = count / 2
    let label: string = "items"
    if enabled { label + ": " + "ready" } else { "disabled" }
}

fn log_complete() -> none {
    none
}

fn main() -> string {
    describe(true)
}
"#),
    },
    Lesson {
        name: "functions",
        title: "11/22 - Functions, parameters, and return types",
        body: r#"Define a function with `fn`, typed parameters, and `->` followed by its
return type. The final expression in the body is its result. Call sites may use
positional or named arguments. Default parameter values are not part of the
current Nudge grammar, so model a default with a small wrapper function.

Try:
  nudgec check functions.ndg"#,
        program: Some(r#"fn greet(name: string, prefix: string) -> string {
    prefix + ", " + name
}

fn friendly_greet(name: string) -> string {
    greet(name, prefix = "Hello")
}

fn main() -> string {
    friendly_greet("Nudge")
}
"#),
    },
    Lesson {
        name: "operators",
        title: "12/22 - Arithmetic, logical, and comparison operators",
        body: r#"Use `+`, `-`, `*`, `/`, and `%` for numeric arithmetic. Comparisons such
as `>=` produce booleans. Logical operators are written `and`, `or`, and `!`
(the compiler also displays them as `and` and `or` in diagnostics). Division of
two integers produces a `float`.

Try:
  nudgec check operators.ndg"#,
        program: Some(r#"fn eligible(age: int, has_ticket: bool) -> bool {
    let remaining = (age + 2) * 3 % 5
    age >= 18 and has_ticket and !(remaining < 0)
}

fn main() -> bool {
    eligible(21, true)
}
"#),
    },
    Lesson {
        name: "string_ops",
        title: "13/22 - String operations",
        body: r#"Use `+` to concatenate strings and `len(value)` to count characters.
`llm\"\"\"...{name}...\"\"\"` prompts support interpolation holes, while ordinary
string literals currently do not. A dedicated `substring` builtin is not yet
implemented, so keep slicing at a host/tool boundary for now.

Try:
  nudgec check string_ops.ndg"#,
        program: Some(r#"fn welcome(name: string) -> string {
    let message = "Welcome, " + name
    let size = len(message)
    if size > 0 { message + "!" } else { "" }
}

fn main() -> string {
    welcome("Ada")
}
"#),
    },
    Lesson {
        name: "records",
        title: "14/22 - Records and type aliases",
        body: r#"A record groups named fields. Name a reusable record shape with
`type Name = { field: Type }`, then construct it with `{ field: value }` and
read fields with dot notation. Inline record types are valid too.

Try:
  nudgec check records.ndg"#,
        program: Some(r#"type User = { name: string, age: int }

fn birthday(user: User) -> User {
    { name: user.name, age: user.age + 1 }
}

fn main() -> string {
    let user: User = { name: "Ada", age: 36 }
    birthday(user).name
}
"#),
    },
    Lesson {
        name: "lists",
        title: "15/22 - Lists, indexing, and collection loops",
        body: r#"Write a list type as `[T]` and a list literal as `[value, ...]`.
Access an element with `items[index]`, and iterate with `for item in items`.
For concurrent transformation, Nudge also provides `par map`; a sequential
`map`/`filter` builtin is not yet part of the language.

Try:
  nudgec check lists.ndg"#,
        program: Some(r#"fn sum(numbers: [int]) -> int {
    let mut total: int = 0
    for number in numbers {
        total = total + number
    }
    total
}

fn main() -> int {
    let values: [int] = [4, 8, 15]
    let first = values[0]
    sum(values) + first
}
"#),
    },
    Lesson {
        name: "options_results",
        title: "16/22 - Optional values and error-shaped data",
        body: r#"The current optional type syntax is `T?`, the checked equivalent of an
`Option<T>`. It accepts either a value or `none`; use `if` to select a fallback.
Generic `Option<T>`, `Result<T, E>`, `unwrap`, and `if let` are planned language
features. Until then, model rich success or failure data with a named record.

Try:
  nudgec check options_results.ndg"#,
        program: Some(r#"type Lookup = { ok: bool, value: string? }

fn display(result: Lookup) -> string {
    if result.ok { "found" } else { "missing" }
}

fn main() -> string {
    let missing: string? = none
    let result: Lookup = { ok: false, value: missing }
    display(result)
}
"#),
    },
    Lesson {
        name: "pattern_matching",
        title: "17/22 - Branching before match expressions",
        body: r#"Nudge's current checked control-flow form is `if ... else ...`.
`match`, exhaustiveness checking, pattern destructuring, and guards are future
syntax, so this lesson uses an explicit boolean condition that compiles today.
Keep each branch returning the same type, just as a future `match` must.

Try:
  nudgec check pattern_matching.ndg"#,
        program: Some(r#"fn status(is_admin: bool, active: bool) -> string {
    if is_admin and active {
        "admin"
    } else {
        "member"
    }
}

fn main() -> string {
    status(false, true)
}
"#),
    },
    Lesson {
        name: "loops",
        title: "18/22 - Collection loops",
        body: r#"Use `for item in list` to process every list element. Nudge also supports
`for (item, index) in list` when the index is useful. `while`, `break`, and
`continue` are not currently parsed, which intentionally keeps ordinary loops
finite and collection-shaped.

Try:
  nudgec check loops.ndg"#,
        program: Some(r#"fn weighted_total(numbers: [int]) -> int {
    let mut total: int = 0
    for (number, index) in numbers {
        total = total + number + index
    }
    total
}

fn main() -> int {
    weighted_total([10, 20, 30])
}
"#),
    },
    Lesson {
        name: "purity_effects",
        title: "19/22 - Purity and tracked effects",
        body: r#"A function without `uses` is pure. Add `uses LLM`, `uses Decision`, or
other declared effects when it performs that operation. Effects propagate
through calls, so callers must declare the effects they expose. Keep pure
formatting and validation helpers separate from effectful boundaries.

Try:
  nudgec check purity_effects.ndg"#,
        program: Some(r#"fn label(name: string) -> string {
    "Hello, " + name
}

fn ask(topic: string) -> string uses LLM {
    llm"""Reply with one concise fact about {topic}."""
    with { model: "fake:default", budget: 0.01 USD }
}

fn main() -> string uses LLM {
    label(ask("Nudge"))
}
"#),
    },
    Lesson {
        name: "modules_imports",
        title: "20/22 - Organizing modules",
        body: r#"Today, a `.ndg` file is a compilation unit and its top-level `type`, `fn`,
and `tool` declarations share one namespace. The lexer reserves `use` and
`export` for forthcoming module support, but `import` and namespace declarations
are not yet accepted by the parser. Organize related functions with clear names
and separate source files until module loading lands.

Try:
  nudgec check modules_imports.ndg"#,
        program: Some(r#"type UserName = string

fn format_user(name: UserName) -> string {
    "user: " + name
}

fn main() -> string {
    format_user("Ada")
}
"#),
    },
    Lesson {
        name: "assertions_contracts",
        title: "21/22 - Assertions and lightweight contracts",
        body: r#"`assert` checks an invariant in a function or test. Use it near boundaries
to document assumptions and protect important properties. Dedicated
`precondition` and `postcondition` keywords are not currently in the grammar,
so assertions are the portable contract mechanism.

Try:
  nudgec check assertions_contracts.ndg
  nudgec test assertions_contracts.ndg"#,
        program: Some(r#"fn non_empty_length(value: string) -> int {
    assert len(value) > 0
    len(value)
}

fn main() -> int {
    non_empty_length("Nudge")
}

test "a known label has a positive length" {
    assert non_empty_length("Nudge") > 0
}
"#),
    },
    Lesson {
        name: "comments_docs",
        title: "22/22 - Comments and documentation",
        body: r#"Use `//` for source comments. A `///` line is lexed as a regular line
comment today, so it is a useful convention for public API documentation but
is not yet extracted into generated docs. Keep comments focused on intent and
use Markdown in your repository documentation for longer guides.

Try:
  nudgec check comments_docs.ndg"#,
        program: Some(r#"/// Produce a greeting for a caller.
// Keep the implementation pure and easily testable.
fn greet(name: string) -> string {
    "Hello, " + name
}

fn main() -> string {
    greet("reader")
}
"#),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part1_lesson_programs_type_check() {
        for lesson in LESSONS_PART1 {
            let Some(source) = lesson.program else { continue };
            let tokens = crate::lexer::lex(source).unwrap();
            let items = crate::parser::parse(tokens).unwrap();
            let errors = crate::check::check(&items);
            assert!(
                errors.is_empty(),
                "lesson '{}' does not compile: {:?}",
                lesson.name,
                errors.iter().map(|error| &error.msg).collect::<Vec<_>>()
            );
        }
    }
}
