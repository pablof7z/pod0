<!-- GSD:project-start source:PROJECT.md -->

## Project

**Pod0 — Voice-to-Rust Agent Cutover**

Pod0 is a native iOS podcast app that turns a listener's library into a searchable, conversational knowledge base. Its Rust kernel already owns nearly all durable product logic — library, playback policy, transcripts, workflows, and text-based Agent conversations. This GSD project scopes the final open increment of milestone M4: reconnecting **Voice Mode** to that same Rust-owned agent conversation authority (GitHub issue #142), plus the headless Rust host crates needed to validate that flow outside the iOS simulator, and the remaining physical-hardware playback validation from M1 (#84).

**Core Value:** Voice interactions use the exact same durable, cancellable, Rust-owned agent conversation as text — no `StubVoiceTurnDelegate` fallback, no second conversation authority, no lost or duplicated turns.

### Constraints

- **Tech stack**: Swift 6 (Xcode 26.6, Tuist 4.200.5), Rust 1.93.0 (pinned via `rust-toolchain.toml`), UniFFI for Swift/Kotlin bindings — fixed by existing architecture, no substitution.
- **Architecture**: one writer per domain; Rust owns durable state and policy, native code executes platform primitives only (AVFoundation, URLSession, Keychain, notifications) — long-term rule, see `README.md` and `.planning/codebase/ARCHITECTURE.md`.
- **Governance**: new or changed Rust ownership requires an ADR plus a migration/deletion link for the replaced Swift owner, per the M0 ownership contract (#55/#64).
- **Typography**: no serif fonts anywhere in the app (`AGENTS.md`).
- **File length**: soft limit 300 lines, hard limit 500 lines (`AGENTS.md`).
- **Android**: explicitly gated behind an M5 go decision — no M6 work in this project's scope.

<!-- GSD:project-end -->

<!-- GSD:stack-start source:codebase/STACK.md -->

## Technology Stack

## Languages

- Rust 1.93.0 - Core domain, application logic, storage, recall indexing, facades, and CLI
- Swift (UIKit/SwiftUI) - iOS native frontend, AVFoundation audio layer, platform integration
- Kotlin - Generated bindings via UniFFI (not currently active in build)
- YAML - GitHub Actions CI/CD workflows
- Shell - Build scripts and utilities

## Runtime

- Rust: Tokio async runtime 1.53.1 with multi-threaded features
- iOS: Swift concurrency (structured concurrency model)
- SQLite: Bundled SQLite via rusqlite, accessed through connection pooling
- Rust: Cargo (workspace-based)
- Swift: Xcode 26.6 with SwiftUI framework
- iOS build: Tuist 4.200.5 for project generation

## Frameworks

- UniFFI 0.32.0 - Rust-to-Swift/Kotlin foreign function interface with generated bindings
- AVFoundation - iOS native audio playback, session management, media controls
- SwiftUI - iOS native UI framework
- Tokio 1.53.1 - Async runtime with `rt`, `time` features; expanded features in specific crates
- Reqwest 0.12.28 - HTTP client with `blocking`, `json`, `rustls-tls` features
- Tungstenite 0.29.0 - WebSocket support
- Tokio-tungstenite 0.29.0 - Async WebSocket integration
- Rusqlite 0.39.0 - SQLite bindings with `backup` and `bundled` SQLite
- SQLite-vec 0.1.9 - Vector storage extension for similarity search
- Serde 1.0.228 - Serialization framework with derive macros
- Serde_json 1.0.150 - JSON encoding/decoding
- Askama 0.16.0 - Template rendering engine with serde integration
- Quick-xml 0.41.0 - XML parsing (for RSS feed handling)
- Rodio 0.21.1 - Audio playback library supporting flac, mp3, mp4, vorbis, wav
- Hound 3.5.1 - WAV file I/O
- macOS-specific: Rodio with `playback` feature enabled
- SHA2 0.10.9 - SHA-256 hashing
- K256 0.13.4 - ECDSA cryptography (arithmetic, schnorr, std features)
- Zeroize 1.9.0 - Secure memory clearing
- Keyring-core 1.0.0 - Native keyring access (macOS, Windows, Linux)
- Apple-native-keyring-store 1.0.2 - macOS Keychain integration
- Nostr 0.44.6 - Nostr protocol (decentralized event protocol)
- URL 2.5.7 - URL parsing and manipulation
- Tempfile 3.27.0 - Temporary file handling
- Time 0.3.47 - Date/time with formatting and parsing
- Regex 1.13.1 - Regular expressions
- Unicode-segmentation 1.12.0 - Unicode grapheme handling
- Libc 0.2.186 - C standard library bindings
- macOS: `apple-native-keyring-store`, `mac-usernotifications`, `objc2-foundation`
- Windows: `windows-native-keyring-store`, `notify-rust`
- Linux: `zbus-secret-service-keyring-store`, `notify-rust`
- All: `cap-std`, `cap-primitives` (capability-based permissions) for macOS/Linux

## Configuration

- Rust toolchain: `rust/rust-toolchain.toml` - Channel 1.93.0, minimal profile, clippy and rustfmt components
- UniFFI configuration: `rust/uniffi.toml` - Swift and Kotlin code generation with immutable records
- Dependency audit: `rust/deny.toml` - License checking, advisory scanning, allowed registries
- Cargo workspace: `rust/Cargo.toml` with 10 member crates
- Xcode project: `Podcastr.xcodeproj` (generated via Tuist)
- iOS minimum deployment target: iOS 26 (beta)
- `POD0_PODCAST_SEARCH_URL` - Podcast search endpoint (tests reference local HTTP endpoint)
- OpenAI-compatible API endpoint format: `http://address/v1`
- ElevenLabs TTS endpoint configuration via `ElevenLabsEndpoint` struct
- Provider secrets stored in iOS Keychain (OpenRouter API keys, TTS provider keys)

## Platform Requirements

- Xcode 26.6 (build 17F113) with iOS 26 simulator runtime
- Tuist 4.200.5 (locked in `.tool-versions`)
- Rust 1.93.0 via `rust-toolchain.toml`
- macOS development machine
- iOS 26+ (deployment target)
- SQLite database (bundled)
- Network connectivity for RSS feed fetching, LLM API calls, TTS generation, Nostr relay connections
- Native capabilities: AVFoundation audio, URLSession networking, Keychain secrets

## Key Dependencies Map

| Crate | Purpose | Key Dependencies |
|-------|---------|------------------|
| `pod0-domain` | Core domain models | sha2, serde, uniffi |
| `pod0-application` | Application logic | pod0-domain, regex, quick-xml, time, url, serde, unicode-segmentation |
| `pod0-storage` | Persistent storage | rusqlite, serde, sha2, url, time |
| `pod0-recall-index` | Vector similarity search | rusqlite, sqlite-vec, serde, sha2 |
| `pod0-facade` | UniFFI facade for iOS | uniffi, sha2, all core crates |
| `pod0-cli` | Command-line interface | tokio, reqwest, rustyline, all core crates |
| `pod0-portable-media` | Audio playback/streaming | rodio, hound, reqwest, tokio, url |
| `pod0-live-hosts` | Live capability adapters | tokio, reqwest, serde, zeroize |
| `pod0-system-hosts` | OS-level capabilities | keyring, notifications, cap-std/cap-primitives |
| `pod0-nostr-host` | Nostr protocol integration | nostr, tokio, k256, zeroize, sha2 |
| `pod0-tts-host` | Text-to-speech generation | reqwest, serde, tokio, zeroize |
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->

## Conventions

## Naming Patterns

- Modules and implementations: `snake_case.rs` (e.g., `effect_outbox.rs`, `listening_store.rs`)
- Test modules: `{name}_tests.rs` (e.g., `effect_outbox_tests.rs`, `library_store_tests.rs`)
- Error types: `{domain}_error.rs` (e.g., `listening_error.rs`)
- Models/schemas: `{domain}_model.rs` (e.g., `effect_outbox_model.rs`)
- `snake_case` for all functions and methods (e.g., `claim_next`, `apply_feed`, `open_authoritative`)
- Constructor methods: `open`, `new` (e.g., `LibraryStore::open_authoritative`)
- Builder/query methods: descriptive verbs with `_with_` for variants (e.g., `claim_next_with_identity`)
- `snake_case` for all local variables, parameters, and struct fields
- Constants: `UPPER_SNAKE_CASE` (e.g., `MIN_LEASE_MILLISECONDS`, `CURRENT_SCHEMA_VERSION`)
- `PascalCase` for structs, enums, and traits (e.g., `EffectLease`, `StorageError`)
- Error enums: `{Domain}Error` (e.g., `ListeningDomainError`, `EffectOutboxError`)

## Code Style

- Rust 2024 edition
- Rust 1.93+ required (see `rust/rust-toolchain.toml`)
- No explicit formatter config; uses Rust defaults
- Line wrapping: multi-line imports for clarity; long method chains allowed
- Clippy enabled via `./scripts/check_rust.sh`
- No tolerance for unsafe code: `#![forbid(unsafe_code)]` at crate level
- All public API surfaces checked via facade/binding generation
- `#![forbid(unsafe_code)]` declared in all crates (`pod0-domain`, `pod0-storage`, `pod0-facade`, etc.)
- Error handling through `Result<T, E>` with explicit error types

## Import Organization

- Multi-line imports for grouped related items (e.g., many types from one module)
- Grouped by source, not alphabetized within groups
- Example from `facade_exports.rs`:
- Barrel files use `pub use {module}::*` pattern
- `lib.rs` declares all modules via `mod` then re-exports via `pub use`

## Error Handling

- Use explicit enum variants for each error case, no generic error codes
- Enum variants are descriptive nouns: `Storage`, `InvalidLeaseDuration`, `StaleLease`
- Example from `effect_outbox_model.rs`:
- Implement `std::fmt::Display` for public errors with detailed messages
- Implement `std::error::Error` trait
- Example from `listening_error.rs`: each variant maps to a human-readable message explaining the invariant violation
- Use `Result<T, E>` everywhere; no panics in library code
- Domain/application layer errors use `uniffi::Error` derive for FFI
- Storage layer errors use custom Display implementations for detailed diagnostics

## Comments

- Doc comments (`///`) on types, fields, and public functions when the invariant is non-obvious
- Example: `/// Versioned comparison identity matching the current Swift store exactly: lowercase the complete absolute URL without trimming a trailing slash.`
- Line comments for subtle algorithm invariants or cross-layer constraints
- No comments for obvious code; names should be self-documenting
- Multi-line doc comments allowed when explaining complex invariants
- Link to related concepts or constraints (e.g., "This domain boundary resolves X during Y")

## Derive Macros

- `Clone` — always included
- `Debug` — always included
- `PartialEq`, `Eq` — for domain types and models
- `Copy` — for small value types (IDs, enums)
- `uniffi::Record` — for types exported to native bindings
- `uniffi::Enum` — for enums exported to bindings
- `uniffi::Error` — for public error types
- `serde::Serialize`, `serde::Deserialize` — for types persisted or transmitted (IDs, records)

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]

## Module Design

- `pub use` declarations in `lib.rs` for public API
- Single responsibility: one major type/domain per module
- Related support modules: `{name}_model`, `{name}_codec`, `{name}_read`, `{name}_write`
- `lib.rs` declares 100+ modules in order of dependency
- Tests in separate `#[cfg(test)] mod {name}_tests` blocks
- No test code mixed with implementation
- Mark small constructors and accessors as `#[must_use] pub const fn`
- Example: ID construction (`from_parts`, `from_bytes`) and conversions

## ID Types

- All cross-layer IDs use the `opaque_id!` macro in `pod0-domain/src/lib.rs`
- Two `u64` fields: `high` and `low`
- Serializable: derive `serde::Serialize`, `serde::Deserialize`
- Comparable: `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`
- Conversion methods: `from_parts(high, low)`, `from_bytes([u8; 16])`, `into_bytes()`
- No string representation; meaning remains domain-specific

## Architectural Constraints

- Workspace crates define shared versions centrally in root `Cargo.toml` under `[workspace.dependencies]`
- All crates inherit workspace `version`, `edition`, `rust-version`, `license`
- Private by default; only `pub` what crosses layer boundaries
- Internal implementation details stay `pub(crate)`

<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->

## Architecture

## System Overview

```text

```

## Component Responsibilities

| Component | Responsibility | File |
|-----------|----------------|------|
| **AppStateStore** | Main Swift state owner, projection adapter for Rust data, mutation gateway | `App/Sources/State/AppStateStore.swift` |
| **Persistence** | SQLite reads/writes for unmigrated Swift state and metadata snapshots | `App/Sources/State/Persistence.swift` |
| **Pod0Facade** | Typed UniFFI interface exposing commands, projections, events from Rust kernel | `rust/crates/pod0-facade/src/runtime.rs` |
| **FacadeState** | Rust-side runtime state holding app actor and all domain stores | `rust/crates/pod0-facade/src/runtime_state.rs` |
| **LibraryStore** | Rust storage for listening, library, subscriptions, episodes, downloads | `rust/crates/pod0-storage/src/listening_store.rs` |
| **TranscriptStore** | Rust storage for transcripts, chapters, semantic segments, artifacts | `rust/crates/pod0-storage/src/transcript_store.rs` |
| **EvidenceStore** | Rust storage for workflow artifacts, external operation evidence, receipts | `rust/crates/pod0-storage/src/evidence_store.rs` |
| **RecallIndex** | SQLite vec embeddings, semantic search index, hybrid retrieval | `rust/crates/pod0-recall-index/src/lib.rs` |
| **PlaybackState** | Transient native playback lifecycle (not authoritative) | `App/Sources/Features/Player/PlaybackState.swift` |
| **WorkflowClient** | Swift-side workflow coordinator, opportunity-driven host execution | `App/Sources/Workflows/WorkflowClient.swift` |

## Pattern Overview

- **One writer per domain:** Rust owns library, listening, playback policy, transcripts, chapters, notes, clips, agents, downloads, recall; Swift retains only unmigrated settings/categories and temporary UI state
- **Projection-based rendering:** Swift never writes Rust-owned state back; views render bounded projections from Rust, send commands back
- **Single typed facade contract:** One UniFFI surface with committed version, generated Swift/Kotlin bindings, CI drift detection
- **Durable workflows over polling:** Rust owns workflow state, fences, retry/block policy; Swift adapters execute native primitives (URLSession, AVFoundation, notifications) and return observations
- **Incremental vertical-slice migration:** Each domain cutover (library → playback → transcript → agent) follows complete cycle: Swift auth → Rust auth → Swift adapters only

## Layers

- Purpose: SwiftUI rendering, navigation, animation, accessibility
- Location: `App/Sources/Features/`, `App/Sources/Design/`
- Contains: Feature views, navigation stacks, sheet/sheet coordinators, transient UI state
- Depends on: AppStateStore projections, platform-specific views
- Used by: Scene/window
- Purpose: Observation-driven state updates, command dispatch to Rust, native lifecycle coordination
- Location: `App/Sources/State/`, `App/Sources/Workflows/`
- Contains: AppStateStore, Persistence, WorkflowClient, lifecycle handlers
- Depends on: Pod0Facade, native platform APIs (AVFoundation, URLSession, Keychain)
- Used by: Presentation, native entry points (AppDelegate, background tasks)
- Purpose: Durable state ownership, command/event processing, domain logic
- Location: `rust/crates/`
- Contains: Domain types, application actor, runtime, storage layers, migrations
- Depends on: SQLite, external host requests (via callback)
- Used by: AppStateStore commands, projections read, observations posted back to Swift
- Purpose: Execute platform primitives bounded by Rust contract
- Location: `App/Sources/Audio/`, `App/Sources/Voice/`, adapters in `App/Sources/Services/`
- Contains: AVFoundation, URLSession, Speech, Keychain, notifications, file I/O
- Depends on: Pod0Facade host request types, platform frameworks
- Used by: AppStateStore adapters, WorkflowClient, background task handlers

## Data Flow

### Primary Request Path (e.g., subscribe to feed)

- Entry: `App/Sources/Features/Library/LibraryView.swift`
- Command: `App/Sources/State/AppStateStore+Episodes.swift:subscribe()`
- Facade: `rust/crates/pod0-facade/src/runtime_commands.rs`
- Store: `rust/crates/pod0-storage/src/listening_store.rs`

### Playback Workflow

- Controller: `App/Sources/Audio/PlaybackController.swift`
- Facade: `rust/crates/pod0-facade/src/runtime_playback_commands.rs`
- Policy: `rust/crates/pod0-domain/src/playback_policy.rs`

### Transcript Workflow

- State: `App/Sources/State/AppStateStore+Transcript.swift`
- Workflow: `App/Sources/Workflows/WorkflowClient.swift`
- Storage: `rust/crates/pod0-storage/src/transcript_store.rs`
- Facade: `rust/crates/pod0-facade/src/runtime_transcript_workflow_commands.rs`

### Agent Conversation Flow

- State: `App/Sources/State/AppStateStore+PeerConversations.swift`
- Adapter: `App/Sources/Services/CoreAgentStreamingClient.swift`
- Facade: `rust/crates/pod0-facade/src/runtime_agent_commands.rs`
- Store: `rust/crates/pod0-storage/src/agent_store.rs`

### State Management

- **Monotonic revisions:** Each state snapshot tagged with revision for ordering
- **Serialized writes:** Background writer queues mutations to prevent interleaving
- **Projection updates never side-effect:** Reading Rust projection does not trigger persistence, notifications, indexing
- **Atomic cutover writes:** One explicit cleanup write per migration phase, then old authority is unreachable

## Key Abstractions

- Purpose: Type-safe command dispatch from Swift to Rust kernel
- Examples: `SubscribeCommand`, `PlaybackObservationCommand`, `AgentMessageCommand`
- Pattern: Sealed enum, mapped from Swift via generated bindings
- Location: `rust/crates/pod0-application/src/commands.rs`
- Purpose: Explicit requests for screen-shaped bounded data
- Examples: `LibraryProjection`, `PlaybackProjection`, `ConversationProjection`
- Pattern: Fetch-specific slices, never a full snapshot
- Location: `rust/crates/pod0-application/src/projections.rs`
- Purpose: Wrapper carrying command, identity, correlation, deadline, cancellation
- Contains: Command, CommandId, SubscriptionId (for reply routing), effect fence
- Pattern: Prevents command handlers from becoming distributed scheduler
- Location: `rust/crates/pod0-application/src/envelope.rs`
- Purpose: Bounded request for native execution (URLSession, AVFoundation, etc.)
- Examples: `FeedHostRequest`, `PlaybackHostRequest`, `TranscriptHostRequest`
- Pattern: Raw bytes in, raw bytes out; no policy in adapter
- Location: `rust/crates/pod0-application/src/host.rs`
- Purpose: @MainActor @Observable state owner and projection adapter
- Pattern: Methods dispatch commands, subscribe to observables for updates
- Constraint: No direct writes to Rust stores; all mutations via AppStateStore methods
- Location: `App/Sources/State/AppStateStore.swift` (300+ lines split into extensions)
- Purpose: SQLite authority for unmigrated Swift domains
- Stores: metadata snapshot, legacy episode rows (migration only), workflow jobs, artifacts
- Pattern: Serialized background writer, atomic transactions
- Location: `App/Sources/State/Persistence.swift`

## Entry Points

- Location: `App/Sources/AppMain.swift:PodcastrApp`
- Triggers: System startup
- Responsibilities: Load AppStateStore, set up environment objects, render RootView
- Location: `App/Sources/App/RootView+DeepLink.swift`
- Triggers: Spotlight, URL schemes, notifications
- Responsibilities: Route to specific feature, request conversation/episode
- Locations: `App/Sources/Workflows/WorkflowClient.swift`, `App/Sources/App/AppDelegate.swift`
- Triggers: BGProcessingTask, BGAppRefresh, CoreLocation, notification
- Responsibilities: Reconcile workflow state, process awaiting effects, refresh feeds
- Locations: Various in `App/Sources/Audio/`, `App/Sources/Voice/`, `App/Sources/Services/`
- Triggers: AVAudioSession interrupt, route change, URLSession completion, speech recognition
- Responsibilities: Notify facade of observation, execute bounded host responses

## Architectural Constraints

- **Threading:** Main App on @MainActor; Rust runtime single-threaded actor pattern; background writer on .utility
- **Global state:** AppStateStore (@MainActor @Observable singleton via environment); Persistence (shared instance); no module-level singletons
- **Circular imports:** None enforced; UniFFI boundary prevents circular references at FFI level
- **One writer per domain:** If new domain migrates from Swift to Rust, old Swift writer becomes unreachable by tests and lint ratchets
- **Projection-only return from Rust:** Mutations never leave Rust via FFI; only projections and events
- **Typed facade version:** Facade version increments on breaking changes; CI rejects drift from Rust metadata

## Anti-Patterns

### Polling from Swift for Rust State

### Direct Persistence Writes Outside State

### Caching Projection Data in Swift View State

### Transcript/Chapter Authority in Swift UI

### Workflow Retry Logic in Native Adapters

## Error Handling

- **Workflow failures:** Captured in `WorkflowProjection.failureState` enum, rendered in UI as bounded status
- **Host request failures:** Returned as `HostObservation::Failure` with error kind; Rust applies retry policy
- **Migration errors:** Logged to console, app starts in degraded mode if critical store cannot open
- **Validation errors:** Commands rejected before commit; agent permissions, command fingerprints validated pre-execution

## Cross-Cutting Concerns

<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->

## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->

## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:

- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->

<!-- GSD:profile-start -->

## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
