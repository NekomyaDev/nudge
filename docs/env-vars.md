# Nudge Environment Variables Reference

Nudge components (`nudgec`, `nudge-runtime`, `nudge-agent`, LSP server) accept configuration flags and telemetry overrides via environment variables.

| Variable | Target Component | Type | Default | Description |
|:---|:---|:---|:---|:---|
| `NUDGE_LOG` | Compiler, Runtime | string | `info` | Logging verbosity: `error`, `warn`, `info`, `debug`, `trace`. |
| `NUDGE_REDACT_SECRETS` | Runtime, Trace | bool | `true` | Automatic redaction of API keys, bearer tokens, and secrets from traces and logs. |
| `NUDGE_PROVIDER` | Runtime | string | `fake` | Default LLM provider backend (`fake`, `openai`, `anthropic`, `antigravity`). |
| `NUDGE_CREDENTIALS_PATH` | Auth, Runtime | path | `~/.nudge/credentials.json` | Path to durable OAuth token and API key store. |
| `NUDGE_DISABLE_PROVIDER_ANTIGRAVITY` | Provider, Auth | bool | `0` | Kill-switch to fail-closed on Antigravity provider requests. |
| `NUDGE_BROWSER_KILL` | Browser, Runtime | bool | `0` | Kill-switch to immediately abort all automated browser operations. |
| `NUDGE_PROCESS_GRANTS` | Proc, Runtime | JSON / string | unrestricted | Security grant policy bounding executable sub-process paths and args. |
| `NUDGE_WASM_MAX_MEMORY_MB` | WASM Sandbox | int | `64` | Maximum allowable linear memory allocation limit for WASI tool guests. |
| `NUDGE_MAX_PAYLOAD_BYTES` | Runtime JSON | int | `10485760` (10MB) | Maximum payload byte size allowed when parsing incoming HTTP JSON bodies. |
| `NUDGE_HTTP_MAX_REDIRECTS` | Runtime HTTP | int | `5` | Maximum number of followed 3xx redirects before aborting with loop error. |
| `NUDGE_OFFLINE` | Compiler, Runtime | bool | `0` | Force offline execution: blocks live provider networks and forces replay verification. |
| `NUDGE_LSP_TRACE` | LSP Server | string | `off` | Verbose LSP server JSON-RPC protocol wire logging (`off`, `messages`, `verbose`). |
