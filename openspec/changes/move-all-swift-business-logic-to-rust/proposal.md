## Why

Pod0's accepted architecture requires Rust to be the sole owner of product and business decisions, but the current Swift exception ratchet only checks eleven files already labelled temporary and misses active policy inside files labelled shared or native. The result is multiple live native decision points, durable stores, effect bypasses, and semantic mappings that can diverge across platforms and cannot satisfy the final #219 zero-exception gate.

## What Changes

- Move all remaining product decisions into typed Rust domain/application owners, including authorization, admission, validation, identity, sequencing, retry/backoff/fallback, cancellation, cross-domain consequences, retention, selection, and semantic outcome classification.
- Cut over agent and voice prompts, model selection, provider authorization, approvals, tool execution, scheduled work, model usage, generated media, and publication orchestration to Rust-owned transitions and durable effect/internal-command intents.
- Move transcript parsing, normalization, speaker identity, provider-phase truth, and failure interpretation to Rust; leave native transcript providers as bounded byte, audio, credential, and Apple Speech capabilities.
- Move settings, categories, category policy, usage accounting, library/search projections, playback policy, imports, and export/share preparation into Rust-owned state and projections.
- Replace direct Swift provider, network, file, media, notification, and publication dispatch with exact leased capability requests and correlated raw observations. Platform frameworks and secret custody remain native.
- Delete dormant or migration-complete Swift policy instead of porting it, including retired feed parsing, review prompting, owner-question infrastructure, obsolete workflow stores, and superseded migration adapters.
- Reconcile the agent permission matrix with the requirement that product-mutating tools enter their owning Rust domains as durable internal commands.
- Strengthen the ownership inventory and architecture checks so business logic cannot hide inside `shared_rust_now` or `native_by_design`, and drive the explicit native-policy exception set to zero.
- **BREAKING**: remove obsolete Swift domain writers, stores, policy APIs, and provider-neutral orchestration contracts after verified one-time migration; update the UniFFI/native-host contracts to Rust-authored requests and raw native observations.

## Capabilities

### New Capabilities

- `rust-business-logic-authority`: Defines Rust as the sole owner of product state, decisions, semantic outcomes, and cross-domain consequences across every Pod0 domain.
- `native-capability-execution`: Defines the exact leased request/raw observation boundary for platform frameworks, credentials, provider transport, files, media, notifications, and publication transport.
- `business-logic-boundary-enforcement`: Defines complete inventory, zero-exception, negative-fixture, migration-retirement, and cross-platform enforcement requirements.

### Modified Capabilities

None. The repository currently has no main OpenSpec capabilities.

## Impact

- Rust domain, application, storage, facade, activity journal, internal-command outbox, external-effect outbox, migrations, projections, and UniFFI/Kotlin bindings.
- Architecture inventories, permission/conformance matrices, exception manifests, negative fixtures, crash/replay tests, and iOS/Kotlin binding validation.
- GitHub migration issues #204 and #213-#219 remain the authority for domain cutover and final proof expectations.
