# Security Policy

Nudge runs LLM agents that touch files, processes, and credentials. We treat
security bugs in the language, runtime, and toolchain as high priority.

## Reporting a Vulnerability

**Do NOT open a public GitHub issue for security vulnerabilities.** Instead,
report it responsibly:

- Open a private security advisory on GitHub (Security → Advisories → Report a
  vulnerability), or
- Email **elaport0880@gmail.com** with:
  1. Description of the vulnerability
  2. Steps to reproduce (a minimal `.ndg` program + the environment that
     triggers it, if you can)
  3. Potential impact
  4. Suggested fix (if any)

## Response Time

- We acknowledge receipt within 48 hours.
- We provide a more detailed response within 7 days.
- We work on a fix and coordinate the disclosure timeline, and credit the
  reporter (unless anonymity is requested).

## Supported Versions

Security fixes land on `main` (the only supported line today).

## Security Model (what we defend)

Nudge's runtime is built fail-closed. Known boundaries worth knowing:

- **Process execution (`proc.*`)** is denied by default; it requires an explicit
  `NUDGE_PROCESS_GRANTS` policy. Grants match on the resolved executable, not
  just a basename, and children run confined to the workspace root. `cmd` and
  its `argv` are passed separately with no shell.
- **Filesystem access (`workspace.*`, `fs.watch`)** is confined to a workspace
  root; path traversal and symlink escapes are denied.
- **Replay** fails closed on traces missing `request_hash` (set
  `NUDGE_REPLAY_ALLOW_UNHASHED=1` to opt into legacy unvalidated FIFO — never do
  this with untrusted traces).
- **Traces** redact secrets (format patterns + env-var values) before they are
  written. `NUDGE_TRACE_REDACT=0` disables this and is a security boundary —
  only use it on traces you will never share.
- **OAuth** state and PKCE verifiers come from the OS CSPRNG and fail closed if
  it is unavailable; credential files are written `0600`.
- **Provider access** that we cannot verify stays fail-closed (e.g. the
  Antigravity integration is opt-in via `NUDGE_ENABLE_ANTIGRAVITY=1`).

Report anything that breaks these boundaries.
