# 20/60 - Organizing modules

Today, a `.ndg` file is a compilation unit and its top-level `type`, `fn`,
and `tool` declarations share one namespace. The lexer reserves `use` and
`export` for forthcoming module support, but `import` and namespace declarations
are not yet accepted by the parser. Organize related functions with clear names
and separate source files until module loading lands.

Try:
  nudgec check modules_imports.ndg
