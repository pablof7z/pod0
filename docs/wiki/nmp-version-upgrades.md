---
title: NMP Version Upgrades
slug: nmp-version-upgrades
summary: "Process for upgrading NMP versions: pin bump, three-layer verification, changelog, and stable base commit."
tags:
  - nmp
  - upgrade
  - version
  - cargo
  - verification
volatility: warm
confidence: medium
created: 2026-05-29
updated: 2026-06-04
verified: 2026-05-29
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:8bfa1b91-b40c-44b3-acb9-245b36f4c841
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
---

# NMP Version Upgrades

> Process for upgrading NMP versions: pin bump, three-layer verification, changelog, and stable base commit.

## Pin Management

NMP is consumed via a git revision pin in `Cargo.toml`. The four pins (for nmp-core, nmp-ffi, and their dependencies in the podcast crate) must all point at the same commit. Before bumping pins, the app must be checked for usage of any C-ABI symbols that were renamed in the new version (e.g., `timeline_insert_events` → `timeline_insert_event_batch`). To upgrade, change the git revision while keeping the package version field unchanged when the upstream tagged commit has not updated its own version string, regenerate `Cargo.lock`, and run `cargo check --workspace` for the host target to surface compile-time breakages. Only the rev changes if the version is unchanged upstream. NMP v0.2.4 is the minimum required version, providing the `make_active` parameter on account creation and `nmp_app_sign_event_for_return` for synchronous signing needed by Blossom HTTP auth. The v0.2.0 upgrade bumps all 4 crate pins from version 0.1.0 (rev ec15edef) to version 0.2.0 (rev ae7b0048) with no C-ABI breaking changes. The v0.2.1 upgrade introduces `AppRelay` / `configured_relays` and builder relay APIs, pinned at the nmp-v0.2.1 tag. The NMP dependency pin must be at version 0.2.2 with rev 6a0c4fda. (Previously: pinned at nmp-v0.2.1 tag.)

<!-- citations: [^14943-51] [^14943-117] [^c43d5-2] [^c43d5-4] [^c43d5-24] -->
## Three-Layer Verification

`cargo check --workspace` on the host target is insufficient to prove an upgrade is clean. The minimum verification is three layers:
1. Host `cargo check --workspace` — compile-time Rust breakages
2. iOS simulator static lib build: `cargo build -p nmp-app-podcast --target aarch64-apple-ios-sim`
3. iOS app build via Xcode (must link against the freshly-rebuilt lib)

All three layers must succeed before the upgrade is considered verified. A live boot smoke-test (launch, confirm kernel initializes, library projects) adds confidence. <!-- [^14943-52] -->

## Commit and Test

The upgrade pin bump is committed on `main` before any adoption work begins. This provides a stable base commit for worktree branches. The smoke test should confirm the app launches, the kernel initializes without panics, no `store_open_failure` fires spuriously, and logs show no FFI errors. A fresh simulator device may be needed if the existing device is corrupted. <!-- [^14943-53] -->

## Changelog Generation

When upgrading NMP, a changelog must be produced summarizing everything new between the old and new pins. This includes:
- New mandatory features (e.g., `store_open_failure`)
- New optional capabilities (e.g., `active_account_handle`)
- New infrastructure (e.g., typed FlatBuffers sidecar for ADR-0037)
- Any breaking changes to the FFI surface or generated types

`resolved_profiles` from NMP v0.2.0 is not optional tech debt — it removes a proven-broken merge pattern and should be adopted in the same window as the pin bump, not deferred.

The changelog is written to `docs/plan/nmp-v<version>-upgrade.md` (parameterized by the target version, e.g., `nmp-v0.2.0-upgrade.md`) and referenced from `docs/plan.md`.

<!-- citations: [^14943-54] [^14943-118] [^8bfa1-4] -->
## See Also
- [[nmp-integration-rules|NMP Integration Rules]] — related guide
- [[ios-build-pipeline|iOS Build Pipeline]] — related guide

