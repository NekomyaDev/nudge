# Algebraic Effect Handlers in Nudge

Nudge uses algebraic effects and handlers to decouple agent intent (e.g. calling an LLM, making an HTTP request, querying memory, or performing OS actions) from runtime execution.

## Resolution Lifecycle

When a program performs an operation declared in an `effect`, execution yields control to the nearest enclosing `handler`. The handler evaluates the operation and optionally resumes the calling computation with a value.

```mermaid
sequenceDiagram
    autonumber
    actor Program as Nudge Program
    participant Runtime as Nudge Runtime (Stack)
    participant Handler as Effect Handler
    participant Provider as External Provider / IO

    Program->>Runtime: perform Effect.op(args)
    activate Runtime
    Runtime->>Runtime: Capture stack frame & pause execution
    Runtime->>Handler: Dispatch op(args) with resumption handle
    activate Handler
    Handler->>Provider: Execute sandboxed operation
    activate Provider
    Provider-->>Handler: Return result / telemetry
    deactivate Provider
    Handler->>Runtime: resume(val)
    deactivate Handler
    Runtime->>Runtime: Restore captured stack frame
    Runtime-->>Program: Return evaluated value
    deactivate Runtime
```

## Semantics

1. **Explicit Effect Declarations**: All side-effecting operations are typed under `effect Name { op(args): ReturnType }`.
2. **Fail-Closed Resolution**: If no enclosing handler covers an executed effect, `nudge-runtime` halts with an unhandled effect diagnostic (`E0301`).
3. **Resumption Mechanism**: Handlers can resume the calling stack once (`resume(val)`), replace it with a fallback, or terminate early upon budget or security constraint violations.
