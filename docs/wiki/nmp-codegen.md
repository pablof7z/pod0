---
title: NMP Codegen
slug: nmp-codegen
topic: project-setup
summary: The entire Rust-to-Swift DTO boundary is machine-generated via swift-codegen with a CI drift gate (swift-bridge-codegen-drift job), structurally preventing the
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-12
updated: 2026-06-13
verified: 2026-06-12
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:rollout-2026-05-26T09-10-44-019e62e8-3112-7080-98ca-48ac46d8b8d2
---

# NMP Codegen

## Stale-Mirror Hazard

The entire Rust-to-Swift DTO boundary is machine-generated via swift-codegen with a CI drift gate (swift-bridge-codegen-drift job), structurally preventing the #371 stale-mirror hazard class. PodcastSettingsSnapshot generated code produces all 8 fields including per-field CodingKeys overrides with explicit snake_case raw values and byte-for-byte faithfulness to the prior hand-maintained body. The compat directory still exists with service, domain, identity, and utility shims; the plan's target of no UserIdentityStore / no compat stubs is not yet met. Comments are relay stubs, social graph is nostr_pending, RAG is substring search, wiki is placeholder generation, briefings are stubs, and voice has real iOS pieces but the Rust loop is still scaffolded; these must not be considered feature parity. Android cross-compile had invalid Rust literal suffixes (0jint, -1jint) introduced by #387; fixed to `0 as jint`/`-1 as jint` with a permanent CI cargo-check --target aarch64-linux-android job to catch cfg-gated breakage. New domain envelope Swift types must be placed in KernelDomainFrames.swift in App/Sources/Bridge/ (NOT in Generated/) because the per-domain envelope DTOs will be folded into the generator as a follow-up after the DTO surface settles. The Fable planner verified that nmp-codegen for .generated.swift mirrors is now executed as item D in cycle 4. (Previously: deferred to next cycle because DTOs were still settling.) Android wire-fixture tests (DomainFrameWireTest, 20 tests) lock the per-domain snake_case decode contract against future Rust renames, and fixed a broken SnapshotCodecTest reference left by the prior clobber-removal PR.

<!-- citations: [^c1691-142] [^c1691-105] [^rollo-225] [^rollo-226] [^c1691-45] [^c1691-58] [^c1691-73] [^c1691-104] [^c1691-119] [^c1691-140] [^c1691-158] [^c1691-180] [^c1691-194] [^c1691-207] [^c1691-268] [^c1691-286] -->
## Swift CodingKeys Guardrails

The swift-codegen generator structurally excludes the #371 freeze hazard by never emitting explicit snake_case CodingKeys for generated types; all Swift fields are camelCase and .convertFromSnakeCase handles the Rust↔Swift key mapping automatically. PodcastSettingsSnapshot is fully generated with per-field CodingKeys overrides including all 8 fields and ~15 explicit snake_case raw values, and the generated body is byte-identical to the prior hand-maintained version. The iOS NostrConversation DTO uses synthesized camelCase keys with no explicit CodingKeys (rootEventID maps via convertFromSnakeCase), guarded by a Swift decode test through the bridge seam. The swift-codegen CI drift gate runs cargo run --bin swift-codegen then git diff --exit-code App/Sources/Bridge/Generated/ and fails if any generated file has drifted from the committed version. The #371 freeze was caused by a Swift-side hand-written CodingKeys divergence, so the load-bearing test is a Swift XCTest decoding Rust-emitted golden JSON through KernelDecoding (.convertFromSnakeCase), not a Rust-only snake-to-snake round-trip. Swift golden decode fixture tests (KernelBridgeWireTests + PodcastUpdateChapterDecodeTests) cover all embedded types through the real KernelDecoding seam, so any Rust/Swift schema divergence fails CI instead of freezing the app. All 7 Swift domain envelope structs in KernelDomainFrames.swift use plain camelCase optional fields with NO explicit CodingKeys enum, relying entirely on .convertFromSnakeCase for the Rust→Swift snake_case mapping, which structurally prevents the #371 freeze hazard class. SocialDomainFrame in Swift has no explicit CodingKeys (uses .convertFromSnakeCase), guarding against the #371 freeze class.

<!-- citations: [^c1691-74] [^c1691-106] [^c1691-120] [^c1691-141] [^c1691-159] [^c1691-181] [^c1691-195] [^c1691-208] [^c1691-224] [^c1691-256] [^c1691-269] [^c1691-287] -->
