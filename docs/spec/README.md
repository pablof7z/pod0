# Historical product and design research

Everything under `docs/spec` is a historical research/design corpus created
before the July 2026 product-surface narrowing and shared-core architecture
decisions. It preserves useful product intent and rejected/unfinished ideas,
but it is **not** an implementation inventory or current architecture source.

In particular, files here may describe deleted friend/comment/wiki/feedback
surfaces, a complete Swift Nostr subsystem, UserDefaults JSON persistence,
serif typography, old file paths, or features that never shipped. Those claims
must not be used as evidence that code exists on `master`.

Resolve conflicts in this order:

1. `AGENTS.md` repository rules.
2. Code, tests, and generated project state on `master`.
3. [`docs/architecture`](../architecture/README.md) ADRs and ownership rules.
4. The active [roadmap](../../Plans/2026-07-18-ios-first-rust-nmp-roadmap.md)
   and GitHub issues/milestones.
5. This historical corpus as product/research context only.

No serif fonts are permitted despite historical visual briefs. Deleted Swift
NMP surfaces are not to be restored; future Nostr work uses the Pod0-owned Rust
kernel over generic NMP.
