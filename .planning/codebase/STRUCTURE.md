# Codebase Structure

**Analysis Date:** 2026-08-22

## Directory Layout

```
pod0/
├── App/                          # iOS app source and widget
│   ├── Sources/
│   │   ├── App/                  # Root composition, lifecycle, navigation
│   │   ├── Audio/                # AVFoundation, playback, media controls
│   │   ├── Agent/                # Agent host, permissions, categories
│   │   ├── Core/                 # Core types and helpers for app layer
│   │   ├── Design/               # SF typography, haptics, themes, components
│   │   ├── Domain/               # Unmigrated Swift domain types
│   │   ├── Features/             # Feature views by vertical (Library, Player, etc.)
│   │   ├── Knowledge/            # Embedding/reranking provider execution
│   │   ├── NMP/                  # Nostr media publishing adapter
│   │   ├── Services/             # Platform adapters (URLSession, speech, etc.)
│   │   ├── State/                # AppStateStore, Persistence, mutations
│   │   ├── Transcript/           # Transcript parsing, models, adapters
│   │   ├── Voice/                # Apple audio/speech capture
│   │   └── Workflows/            # Workflow coordination, durable jobs
│   ├── Widget/                   # App widget extension
│   ├── Resources/                # Assets, images, strings
│   └── Support/                  # Build configuration
├── rust/                         # Rust kernel workspace
│   ├── Cargo.toml               # Workspace manifest
│   ├── crates/
│   │   ├── pod0-domain/         # Domain types, values, policies
│   │   ├── pod0-application/    # Application actor, commands, events
│   │   ├── pod0-storage/        # SQLite stores, migrations, transactions
│   │   ├── pod0-facade/         # UniFFI bridge, runtime, projections
│   │   ├── pod0-recall-index/   # Vector index for semantic search
│   │   ├── pod0-live-hosts/     # Test host adapters
│   │   ├── pod0-portable-media/ # Media format handling
│   │   ├── pod0-cli/            # CLI tool for testing
│   │   ├── pod0-bdd/            # BDD test harness
│   │   └── uniffi-bindgen/      # Swift/Kotlin binding generator
│   ├── schema/                   # SQLite schema, migrations
│   └── apple/                    # Apple framework definitions
├── AppTests/                     # iOS unit/integration tests
├── BindingsSmoke/                # Generated binding smoke tests
├── Generated/Pod0Core/           # Generated Swift bindings from UniFFI
├── Fixtures/                     # Test fixtures, seed data
├── Config/                       # Build configuration, Swift packages
├── Derived/                      # Generated build artifacts (not committed)
├── docs/                         # Architecture docs, specs, runbooks
│   ├── architecture/             # ADRs, ownership, schema migrations
│   ├── bdd/                      # Behavior-driven test specs
│   ├── spec/                     # Historical product specs
│   ├── validation/               # Validation results
│   └── wiki/                     # General documentation
├── Plans/                        # Long-term roadmaps
├── scripts/                      # Build, test, verification scripts
├── ci_scripts/                   # CI pipeline scripts
├── Project.swift                 # Tuist project definition
├── README.md                     # Getting started
├── AGENTS.md                     # Project guidelines for Claude
└── CLAUDE.md                     # Local Claude instructions
```

## Directory Purposes

**App/Sources/App:**
- Purpose: Root app composition, navigation architecture, lifecycle handlers
- Contains: `PodcastrApp` (entry point), `RootView` (main UI), `AppDelegate` (foreground/background), deep linking, sidebar
- Key files: `AppMain.swift` (entry), `RootView.swift` (layout), `AppDelegate.swift` (lifecycle)

**App/Sources/State:**
- Purpose: Application state ownership, mutation boundary, persistence
- Contains: `AppStateStore` (@Observable, @MainActor), `Persistence` (SQLite), mutation extension per domain
- Key files: `AppStateStore.swift` (state owner), `Persistence.swift` (SQLite I/O), `AppStateStore+*.swift` (domain methods)

**App/Sources/Features:**
- Purpose: Feature presentation and navigation
- Contains: Library, Player, Home, EpisodeDetail, Search, Recall, Settings, Agent, Clips, Voice features
- Pattern: Each feature is a folder with views, state, presenters; no cross-feature data sharing
- Key files: `LibraryView.swift`, `PlayerView.swift`, `HomeView.swift`, `SettingsView.swift`

**App/Sources/Workflows:**
- Purpose: Durable workflow coordination, native adapter execution
- Contains: `WorkflowClient` (coordinator), desired-state planner, effect routers, native host implementations
- Key files: `WorkflowClient.swift` (orchestrator), `DesiredStatePlanner.swift` (next action logic)

**App/Sources/Audio:**
- Purpose: AVFoundation integration, playback control, media controls
- Contains: `PlaybackController` (AVPlayer wrapper), audio session, route/interrupt handling, media controls
- Key files: `PlaybackController.swift` (wrapper), `AudioSessionManager.swift` (session control)

**App/Sources/Services:**
- Purpose: Platform adapter implementations
- Contains: URLSession feed fetcher, speech recognizer, file I/O, Keychain reader, notification builder
- Key files: Named per service (e.g., `FeedServiceImpl.swift`, `SpeechRecognitionService.swift`)

**rust/crates/pod0-domain:**
- Purpose: Immutable domain types, value objects, business logic
- Contains: Listening, Episodes, Subscriptions, Playback policy, Transcripts, Notes, Clips, Agents, Download identity
- Pattern: Pure Rust structs, no I/O, thoroughly tested
- Key files: `listening.rs`, `playback_policy.rs`, `transcript_artifact.rs`, `notes.rs`

**rust/crates/pod0-application:**
- Purpose: Runtime state management, command/event handling
- Contains: Application actor, command envelopes, projection requests, host request types, clock trait
- Pattern: Single async actor, no mutable state outside actor lock
- Key files: `lib.rs` (actor definition), `commands.rs` (command dispatch), `projections.rs` (query types)

**rust/crates/pod0-storage:**
- Purpose: SQLite stores, migrations, transactional writes, evidence persistence
- Contains: LibraryStore (listening/library), TranscriptStore, EvidenceStore, ScheduledAgentStore, migrations
- Pattern: Authoritative for Rust domains, one write path per store
- Key files: `listening_store.rs`, `transcript_store.rs`, `evidence_store.rs`, schema files

**rust/crates/pod0-facade:**
- Purpose: UniFFI bridge, public API contract, runtime state holder
- Contains: Pod0Facade (entry point), runtime state, command routing, projection response building
- Pattern: One `Pod0Facade` instance per app process, Arc<Mutex<>> state, subscriptions routed back to Swift
- Key files: `runtime.rs` (state & open), `facade_exports.rs` (public API), `runtime_*.rs` (workflow implementations)

**rust/crates/pod0-recall-index:**
- Purpose: Vector embeddings, semantic search, hybrid retrieval
- Contains: Recall index open/close, search API, dimension constants
- Key files: `lib.rs` (index operations)

**App/Sources/Design:**
- Purpose: Design system, theme, components, haptics
- Contains: AppTheme (colors, typography), SF font helpers, animations, haptics, glass surfaces
- Constraint: **No serif fonts ever** — only SF/system fonts
- Key files: `AppTheme+*.swift`, `GlassSurface.swift`, `Haptics.swift`

**AppTests/Sources:**
- Purpose: Unit and integration tests for iOS layer
- Contains: State mutation boundary tests, feature logic tests, integration tests with mock stores
- Key files: `AppStateMutationBoundaryTests.swift` (enforces State/AppStateStore boundary)

**Generated/Pod0Core:**
- Purpose: Generated Swift bindings from UniFFI
- Generated by: `cargo build --release` in Rust workspace, checked into git
- Pattern: Do not edit; regenerate from `rust/crates/pod0-facade/src/lib.rs` if facade changes
- Files: `Pod0Core.swift` (large, auto-generated)

**docs/architecture:**
- Purpose: Architecture decisions, ownership inventory, schema runbooks
- Key files: `README.md` (index), `ownership.json` (Swift file classification), `schema-migrations.md` (policy)

**scripts:**
- Purpose: Build verification, architecture checking, bindings validation
- Key files: `check_architecture.py`, `check_rust.sh`, `check_core_binding_drift.sh`

## Key File Locations

**Entry Points:**
- iOS app start: `App/Sources/AppMain.swift`
- Main UI: `App/Sources/App/RootView.swift`
- App delegate: `App/Sources/App/AppDelegate.swift`
- Rust facade: `rust/crates/pod0-facade/src/runtime.rs`

**Configuration:**
- Tuist project: `Project.swift`
- Rust workspace: `rust/Cargo.toml`
- Swift packages: `Config/SwiftPackages/Package.resolved`
- Xcode build: `.xcode-version`, `.xcode-build-version`

**Core Logic:**
- State owner: `App/Sources/State/AppStateStore.swift`
- Persistence: `App/Sources/State/Persistence.swift`
- Workflows: `App/Sources/Workflows/WorkflowClient.swift`
- Rust domain: `rust/crates/pod0-domain/src/lib.rs`
- Rust app: `rust/crates/pod0-application/src/lib.rs`
- Rust storage: `rust/crates/pod0-storage/src/lib.rs`

**Testing:**
- iOS tests: `AppTests/Sources/`
- Binding smoke tests: `BindingsSmoke/Kotlin/`, generated Swift equivalent linked in iOS
- Rust workspace tests: each crate's `#[cfg(test)]` modules
- Fixtures: `Fixtures/CoreImport/` (Swift library import proof)

## Naming Conventions

**Files:**
- Swift: PascalCase.swift (e.g., `AppStateStore.swift`, `PlaybackController.swift`)
- Rust: snake_case.rs (e.g., `listening_store.rs`, `playback_policy.rs`)
- Test files: `*_tests.rs` (Rust), `*Tests.swift` (Swift)
- Extensions: `Filename+Extension.swift` (e.g., `AppStateStore+Episodes.swift`)

**Directories:**
- Swift features: PascalCase (e.g., `Features/Library`, `Features/Player`)
- Rust crates: lowercase-with-hyphens (e.g., `pod0-facade`, `pod0-storage`)
- Workflow modules: snake_case (e.g., `runtime_playback_tests.rs`)

**Types:**
- Swift: PascalCase (e.g., `AppStateStore`, `PlaybackState`, `LibraryView`)
- Rust: PascalCase (e.g., `ListeningCommand`, `PlaybackPolicy`, `LibraryStore`)
- Protocols/Traits: PascalCase ending in -able/-ible (e.g., `Sendable`, `Observable`)

**Functions:**
- Swift: camelCase (e.g., `subscribe()`, `observePlayback()`)
- Rust: snake_case (e.g., `observe_playback()`, `apply_policy()`)

## Where to Add New Code

**New Feature (UI Vertical):**
- Folder: Create `App/Sources/Features/FeatureName/`
- Views: `App/Sources/Features/FeatureName/FeatureView.swift`
- State: Add method to `AppStateStore+FeatureName.swift` or use existing patterns
- Tests: `AppTests/Sources/Features/FeatureNameTests.swift`

**New Component/Module (Shared):**
- Shared design components: `App/Sources/Design/ComponentName.swift`
- Shared services: `App/Sources/Services/ServiceName.swift`
- Domain types: `App/Sources/Domain/TypeName.swift` (unmigrated only; new types go to Rust)

**Utilities:**
- Helper functions: `App/Sources/Design/StringExtensions.swift` or create new extension file
- Protocol conformances: Same file as type, or `Filename+Protocol.swift` if large

**Rust Domain/Logic:**
- New domain type: `rust/crates/pod0-domain/src/typename.rs`
- New store method: `rust/crates/pod0-storage/src/storename.rs`
- New facade command: Add `CommandName` variant to `rust/crates/pod0-application/src/commands.rs`, implement handler in `rust/crates/pod0-facade/src/runtime_*.rs`
- New migration: Add versioned migration in `rust/crates/pod0-storage/src/schema_*.rs`, register in `LibraryStore` startup

**Tests:**
- iOS unit tests: Co-located in `AppTests/Sources/` mirroring `App/Sources/` structure
- Rust tests: Module-level `#[cfg(test)] mod tests { ... }` in same file, or separate `*_tests.rs`
- Integration tests: Rust BDD harness in `rust/crates/pod0-bdd/`

## Special Directories

**Generated/**
- Purpose: Generated build products
- Generated by: Tuist (`tuist generate`), Rust build, UniFFI bindgen
- Committed: Generated/Pod0Core/ (Swift bindings checked in)
- Pattern: `Generated/Pod0Core/` must be regenerated and committed on any UniFFI change

**Derived/**
- Purpose: Xcode build intermediates
- Generated by: Xcode build system
- Committed: No

**.pi/**
- Purpose: Planning/investigation artifacts
- Committed: No (temporary working files)

**Fixtures/**
- Purpose: Seed data, test bundles, compatibility fixtures
- Contains: Test episode metadata, OPML imports, legacy Swift library imports
- Pattern: Immutable test data referenced by unit/integration tests

**Plans/**
- Purpose: Long-term roadmaps, phase breakdowns, design proposals
- Key files: `2026-07-18-ios-first-rust-nmp-roadmap.md` (authoritative sequencing)

---

*Structure analysis: 2026-08-22*
