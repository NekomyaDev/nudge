# Nudge Style Guide & Naming Conventions

> **Standard:** NDG-STYLE-1.0  
> **Target Version:** Nudge v1.4.0+  
> **Authors:** NekomyaDev  

This style guide establishes idiomatic coding standards, naming conventions, and file organization for Nudge programs.

---

## 1. Naming Conventions

| Language Construct | Casing Convention | Examples | Notes |
|:---|:---|:---|:---|
| **Functions** | `snake_case` | `calculate_total`, `load_draft` | Verbs or action-oriented phrases |
| **Variables & Locals** | `snake_case` | `user_name`, `entry_count` | Descriptive nouns |
| **Types & Records** | `PascalCase` | `FileEntry`, `ReviewState` | Domain nouns |
| **Effects** | `PascalCase` | `LLM`, `Workspace`, `Process`, `UI` | Capabilities tracked by the compiler |
| **Agents** | `PascalCase` | `FileManagerApp`, `ResearchAgent` | Autonomous entities with state |
| **Views** | `PascalCase` | `MainWindow`, `InspectorPanel` | Reactive UI composable components |
| **Constants** | `SCREAMING_SNAKE_CASE` | `MAX_RETRIES`, `DEFAULT_TIMEOUT_MS` | Immutable globals or bounds |

---

## 2. Formatting & Indentation

- **Indentation:** Use **4 spaces** per indent level (never tabs).
- **Braces:** Use cuddled 1TBS / K&R style: opening brace on the same line.
```nudge
fn process_item(item: Item) -> Result uses Workspace {
    if item.is_valid {
        workspace.write(item.path, item.data)
    } else {
        log_error("Invalid item")
    }
}
```
- **Line Length:** Target a maximum line length of 100 characters.
- **Auto-Formatting:** Run `nudgec fmt <file.ndg>` to standardize indentation and whitespace.

---

## 3. Effects & Purity

1. **Explicit Effect Declarations:** Functions performing I/O, network calls, or LLM interactions must declare their effects via `uses`:
```nudge
fn ask_assistant(prompt: string) -> string uses LLM {
    llm"""{prompt}""" with { model: "anthropic:sonnet-4.6", budget: 0.05 USD }
}
```
2. **Attenuate Capabilities:** Keep effect lists minimal. Do not declare `WorkspaceDelete` if only `Workspace` reading is required.

---

## 4. Documentation Comments

- Use triple slashes (`///`) for function and type documentation.
- Document preconditions, effects, and expected return types.
```nudge
/// Formats a file size in bytes to a human-readable string (KB, MB, GB).
fn format_file_size(bytes: int) -> string {
    if bytes < 1024 {
        str.format("{0} B", bytes)
    } else {
        str.format("{0} KB", bytes / 1024)
    }
}
```
EOF
