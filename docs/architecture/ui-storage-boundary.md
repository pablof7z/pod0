# Native UI to durable-state boundary

Feature and application-shell code may render projections, hold transient
presentation state, dispatch typed intents, and execute platform capabilities.
It may not open durable repositories, inspect workflow databases, or bypass the
current application/domain command boundary.

Run the policy and its negative fixture with:

```bash
python3 scripts/check_ui_storage_boundary.py --self-test
python3 scripts/check_ui_storage_boundary.py
```

[`ui-storage-boundary.json`](ui-storage-boundary.json) contains exact prohibited
symbols and exact-file exceptions. Every exception has an owning GitHub issue
and deletion target. A new exception is architecture work and must not be added
solely to make CI green.

The current exceptions expose real migration seams rather than approved final
architecture: direct `TranscriptStore` and `ChatHistoryStore` use. Issues #59
and #60 remove them through observable projections and typed commands. Recall
and search no longer carry exceptions: they consume the typed shared-core
projection, and UI access to `RecallCapabilityService` is prohibited.

Generated UniFFI bindings are not presentation code and will live outside the
scanned feature roots. Hand-authored native adapters remain subject to the
ownership inventory and may execute capabilities, but cannot become another
durable writer.
