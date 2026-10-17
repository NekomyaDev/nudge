# Interactive Approval Station

An interactive human-in-the-loop review station built with Nudge's native typed UI primitives (`column`, `row`, `text`, `input`, `button`).

## Features

- **Typed UI Hierarchy:** Composes layout using `column` and `row` with configurable padding and gap.
- **Interactive State:** `ApprovalStation` manages editable input text, review findings, and approval decisions.
- **Zero-Dollar LLM Inspection:** Uses the deterministic fake provider by default for fast, offline testing.
- **Headless Unit Testing:** Tests the full UI lifecycle with `mount_headless()` and `dispatch_click()` in memory.

## Run

Check the program:

```sh
nudgec check main.ndg
```

Run interactively:

```sh
NUDGE_PROVIDER=fake nudgec run main.ndg
```

Run test suite:

```sh
NUDGE_PROVIDER=fake nudgec test main.ndg
```
