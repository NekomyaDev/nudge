# JEV-family conformance

The conformance suite in [`conformance/`](../conformance/) is **vendor-neutral**: it validates frozen-v1 NTF traces, and the decision cases exercise the `/v1/systemone` decision contract (Laya's `laya.serve`, the TypeSafe Jev API, Valen transports, anything wire-compatible).

## If you build a decision model or server

1. Serve or produce traces that include `decision.call` records with `model`, `provider`, `questions`, `answers`, `latency_ms`, `outcome` (see [docs/ntf-spec.md](ntf-spec.md)).
2. Run the suite against your validator or your server's emitted traces:

   ```sh
   python3 conformance/run.py --cmd your-validator --args check
   ```

3. All 25 cases pass? Open a PR adding your project to the list below.

## Compatible implementations

| Implementation | Transport | Status |
|:---|:---|:---|
| Laya 0.3.20 (`laya-serve`) | HTTP `/v1/systemone` | verified live (nudge e2e, 2026-09) |
| TypeSafe Jev API | HTTP `/v1/systemone` | contract-verified |
| [Valen](https://github.com/Liuziyu77/Valen) | JSONL subprocess (`valen.inference`) | contract-verified (mock e2e; live pending) |

*"Contract-verified" means the adapter validation rules pass against the documented wire contract; "verified live" means a real end-to-end run on this machine. Being listed is opt-in — open a PR.*
