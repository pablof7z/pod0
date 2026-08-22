---
schema_version: 1
open_count: 2
waived_count: 0
fixed_count: 0
total_count: 2
last_updated: 2026-08-22T15:57:29.119Z
---

# Broken Windows Ledger

> Cross-phase defect register. With `workflow.windows_enforce` enabled, `/gsd-ship` blocks while `open_count > 0`.
> Waive with `gsd-tools windows waive <id> "<reason>"` (reason required).
> Mark fixed with `gsd-tools windows fixed <id>`.

| id | phase | kind | file | line | description | status | reason | recorded_at | resolved_at |
|----|-------|------|------|------|-------------|--------|--------|-------------|-------------|
| 1 | 01 | deviation | rust/deny.toml |  | cargo deny check: pod0-cli's rustyline dependency pulls clipboard-win/error-code (BSL-1.0), rejected by deny.toml's license allowlist. Pre-existing (pod0-cli was already a workspace member before phase 1 plan 01), not a dependency of the three newly-joined host crates. Needs a real policy decision (replace rustyline or broaden allowlist), not a mechanical fix. | open |  | 2026-08-22T15:57:29.035Z |  |
| 2 | 01 | unrun-verify | rust/crates/pod0-application/src/contract.rs |  | cargo test --workspace: 4 pre-existing failures (chapter/feed_fetch/recall/transcript cross-language fixture tests) — FACADE_CONTRACT_VERSION bumped to 55 in already-committed source but golden fixtures still say 54. Unrelated to the six host crates; needs cross-platform fixture regeneration outside this plan's scope. | open |  | 2026-08-22T15:57:29.119Z |  |

````json
[
  {
    "id": 1,
    "kind": "deviation",
    "phase": "01",
    "file": "rust/deny.toml",
    "line": null,
    "description": "cargo deny check: pod0-cli's rustyline dependency pulls clipboard-win/error-code (BSL-1.0), rejected by deny.toml's license allowlist. Pre-existing (pod0-cli was already a workspace member before phase 1 plan 01), not a dependency of the three newly-joined host crates. Needs a real policy decision (replace rustyline or broaden allowlist), not a mechanical fix.",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-08-22T15:57:29.035Z",
    "resolved_at": null
  },
  {
    "id": 2,
    "kind": "unrun-verify",
    "phase": "01",
    "file": "rust/crates/pod0-application/src/contract.rs",
    "line": null,
    "description": "cargo test --workspace: 4 pre-existing failures (chapter/feed_fetch/recall/transcript cross-language fixture tests) — FACADE_CONTRACT_VERSION bumped to 55 in already-committed source but golden fixtures still say 54. Unrelated to the six host crates; needs cross-platform fixture regeneration outside this plan's scope.",
    "status": "open",
    "reason": "",
    "recorded_at": "2026-08-22T15:57:29.119Z",
    "resolved_at": null
  }
]
````
