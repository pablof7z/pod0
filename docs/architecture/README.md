# Pod0 architecture decisions

This directory is the durable architecture source of truth for new work. The
current implementation remains Swift-first while domains move, one complete
vertical slice at a time, to the Pod0-owned Rust kernel.

The operating rule is:

> Native executes platform primitives; Rust owns durable product decisions.

## Accepted decisions

1. [ADR-0001: Native and shared ownership](adr/0001-native-and-shared-ownership.md)
2. [ADR-0002: Pod0 Rust kernel and NMP boundary](adr/0002-pod0-rust-kernel-and-nmp-boundary.md)
3. [ADR-0003: Typed UniFFI application facade](adr/0003-typed-uniffi-application-facade.md)
4. [ADR-0004: Persistence, schemas, and single-writer migration](adr/0004-persistence-and-single-writer-migration.md)
5. [ADR-0005: Android investment gate](adr/0005-android-investment-gate.md)
6. [ADR-0006: Shared Rust recall-index execution](adr/0006-shared-rust-recall-index.md)
7. [ADR-0007: Scheduled agent workflow and artifact ownership](adr/0007-scheduled-agent-workflow-and-artifact-ownership.md)

## Planning and enforcement

- [iOS-first shared-core roadmap](../../Plans/2026-07-18-ios-first-rust-nmp-roadmap.md)
- [Swift ownership inventory](ownership.md)
- [Native UI to durable-state boundary](ui-storage-boundary.md)
- [Architecture CI ratchets](ci-ratchets.md)
- [App-core schema migration policy](schema-migrations.md)
- [Transcript authority migration runbook](transcript-migration-runbook.md)
- [Product-proof metric dictionary](../validation/product-proof-metrics.md)
- [Cross-platform-sensitive PR checklist](../../.github/pull_request_template.md)
- [Ownership inventory issue](https://github.com/pablof7z/pod0/issues/64)
- [Architecture guardrail epic](https://github.com/pablof7z/pod0/issues/55)
- [First Rust-backed listening slice](https://github.com/pablof7z/pod0/issues/58)
- [Rust-backed saved clips slice](https://github.com/pablof7z/pod0/issues/93)
- [Canonical transcript contract](https://github.com/pablof7z/pod0/issues/94)
- [Transcript storage and staged import](https://github.com/pablof7z/pod0/issues/95)
- [Typed iOS transcript adapter](https://github.com/pablof7z/pod0/issues/96)
- [Transcript authority cutover](https://github.com/pablof7z/pod0/issues/97)

An ADR changes only through a later ADR that names the superseded decision.
Implementation convenience is not an implicit waiver.
