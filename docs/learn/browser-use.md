# In-House Real Web Browser Engine & Automation

Nudge v1.6 introduces a zero-dependency, in-house web browser engine implemented entirely within the Nudge ecosystem and pure `.ndg` prelude code.

Unlike legacy browser automation frameworks that rely on external binary processes (Chromium, Firefox, Playwright, Puppeteer), Nudge features an in-language browser engine where HTML tokenization, DOM tree construction, CSS selector resolution, form state mutations, sandboxed JavaScript evaluation, box-model layout, and offscreen screenshot compositing are executed directly inside the Nudge runtime and prelude.

---

## 1. The `Browser` Effect

Browser operations are typed and checked by the Nudge compiler. Any routine performing browser navigation, DOM interaction, or network fetch must explicitly declare the `Browser` effect:

```nudge
fn search_docs(query: string) -> [string] uses Browser, LLM {
    // browser operations are scoped and permitted
}
```

Functions omitting `uses Browser` that attempt browser interactions trigger compile error `E0301`.

---

## 2. Security Policy & Strict Confinement

Every browser automation routine runs under a configurable policy:
- **Host Allowlist**: Confined to declared hosts or domains.
- **Scheme Enforcement**: Strictly permits `http:`, `https:`, and workspace-confined `file:` schemes; rejects `javascript:`, `data:`, `vbscript:`.
- **Redirect Limits**: Hard limits on maximum redirection chains (default: 20).
- **Default Locale**: Strictly neutral `en-US` locale and UTC timezone, preventing host environmental fingerprint leakage.

```nudge
let policy = {
    allow: ["docs.nudge.dev", "api.nudge.dev"],
    workspace_root: "/app/workspace",
    max_redirects: 10
}
```

---

## 3. High-Level Flow: Navigate, Inspect & Act

```nudge
// 1. Fetch web page
let resp = browser.fetch("https://docs.nudge.dev")

// 2. Build DOM Tree in pure .ndg
let tokens = tokenize_html(resp.body)
let tree = build_dom_tree(tokens)

// 3. Generate token-efficient accessibility snapshot
let snap = make_snapshot("page-1", resp.url, "Nudge Docs", 1, tree)

// 4. Query elements using CSS selectors
let search_input = query_selector(tree, "input[name=\"search\"]")
let submit_btn = query_selector(tree, "button.search-btn")

// 5. Fill input and click
let updated_tree = fill_form_field(tree, search_input.ref, "effects")
let click_res = execute_click_target(updated_tree, submit_btn.ref, snap)
```

---

## 4. Deterministic $0 Replay

Every live browser session emits standard NTF v1 records (`browser.snapshot` and `browser.act`). During development, regression testing, or scheduled runs:

```bash
NUDGE_REPLAY=traces/search_run.jsonl nudge run search.ndg
```

Replay executes with **zero network requests** and **zero LLM cost**, reading snapshots directly from recorded state IDs. Any structural deviation between live DOM and recorded baseline is detected by `compute_drift` without triggering side-effecting actions.
