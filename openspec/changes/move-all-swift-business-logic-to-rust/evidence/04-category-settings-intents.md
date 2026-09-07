# Category and settings intent cutover evidence

Task 4.4 routes settings and category UI changes through typed Rust commands
and renders the resulting bounded Rust projections.

## Proven invariants

- Product-setting UI changes are diffed into named, bounded intents; native
  code no longer replaces the complete Rust settings aggregate.
- Category import and its authority marker commit atomically and replay safely.
- Category replacement, exclusive podcast movement, and category settings use
  revision-checked Rust commands.
- Category projections expose no more than 4,096 podcast memberships and
  report both total membership and truncation.
- AppState category/settings fields are projection caches. Native persistence
  redacts them after the corresponding authority marker is verified.
## Verification results

- `cargo test -p pod0-application settings_intent`: 2 passed.
- `cargo test -p pod0-facade category_facade_tests`: 2 passed.
- `cargo test -p pod0-facade product_settings_facade_tests`: 1 passed.
- Podcastr simulator build through `xcodebuildmcp`: succeeded.
- Focused `ProductSettingsBridgeTests` and
  `SharedProjectionPersistenceBoundaryTests` through `xcodebuildmcp`: 9 passed.
- `scripts/check_core_binding_drift.sh`: generated bindings match Rust metadata.
- `scripts/check_kotlin_core_bindings.sh`: compile and runtime smoke passed.
- `python3 scripts/check_architecture_ownership.py`: 1,798 production files covered.
- `python3 scripts/check_rust_business_logic_boundary.py`: 550 native files scanned;
  exception set exact and non-growing.
- `python3 scripts/check_file_lengths.py`: passed with only recorded soft-limit debt.
