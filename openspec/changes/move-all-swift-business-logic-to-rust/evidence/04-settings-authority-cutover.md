# Settings authority cutover evidence

Task 4.2 imports legacy AppState settings once and removes portable settings
authority from native persistence and iCloud.

## Proven invariants

- Settings data and the `product_settings` authority marker commit atomically.
- Clean and populated imports survive reopen with their exact portable values.
- An interrupted marker insert rolls back the settings row and retries safely.
- An unmarked preexisting settings row fails closed without overwrite.
- Local and remote updates are rejected before authority exists.
- Equal-counter remote conflicts converge by stable writer identity and retain
  redaction-safe evidence.
- AppState persistence keeps device-local credential metadata but redacts
  portable settings after cutover.
- iCloud supplies versioned observations; only a Rust-merged projection is
  rendered or mirrored back.

## Verification commands

- `cargo test -p pod0-application settings_transition -- --nocapture`
- `cargo test -p pod0-storage product_settings -- --nocapture`
- `cargo test -p pod0-facade product_settings -- --nocapture`
- `scripts/check_swift_core_bindings.sh`
- `scripts/check_kotlin_core_bindings.sh`
- `python3 scripts/check_rust_business_logic_boundary.py`
- `python3 scripts/check_file_lengths.py`

The iOS simulator build remains blocked during this task because the local NMP
package is intentionally absent. It is not restored; task 10.6 deletes that
dependency and all remaining Nostr/NMP surfaces.
