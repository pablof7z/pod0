# Pod0

Pod0 is a native iOS podcast application that turns a listener's library into
a searchable, conversational knowledge base. The product is designed to feel
calm while listening and become alive when a listener asks, recalls, clips, or
creates from what they heard.

The current application name and Xcode scheme are `Podcastr`; Pod0 is the
product/repository name used by the architecture and roadmap.

## What is on master

- Podcast discovery, RSS subscriptions, OPML import/export, library and episode
  detail.
- Native AVFoundation playback, queueing, resume state, downloads, media
  controls, route/interruption handling, and a widget snapshot.
- Publisher and generated transcripts with diarized/timed segments, chapters,
  local semantic indexing, hybrid retrieval, highlights, notes, and clips.
- An OpenRouter-backed tool-calling agent with transcript/library tools, voice
  input, TTS-generated episodes, and provider settings stored through Keychain.
- Durable SQLite workflow jobs with leases, fencing, retry/block states,
  versioned artifacts, background opportunities, and process-reconstruction
  tests.

Master also contains the additive Pod0 Rust kernel, one typed UniFFI facade,
generated Swift and Kotlin bindings, deterministic Apple packaging, and the
versioned app-core SQLite migration/backup mechanism. The facade is linked into
iOS for compile/runtime qualification but Rust owns no user data yet; Swift
remains authoritative until the first complete vertical-slice cutover. There
is no Android application. Generic NMP is pinned behind the Pod0 adapter and is
not linked into the facade while security issue #85 is open.

## Architecture

The long-term rule is:

> Native executes platform primitives; Rust owns durable product decisions.

Today, `AppStateStore` is the Swift application-state owner. `Persistence` is
SQLite-authoritative: it stores a versioned metadata snapshot, per-episode JSON
rows, persistence generation, workflow jobs, and artifact metadata. Legacy JSON
is migration input only. Provider secrets remain in Keychain.

The migration is incremental. SwiftUI, AVFoundation, audio sessions/routes,
media controls, BGTask/URLSession entry points, notifications, Keychain,
biometrics, widgets, and platform integrations remain native. Stable durable
library, playback-policy, transcript, knowledge, workflow, agent, and
Pod0-specific Nostr behavior moves by complete vertical slice to a Pod0-owned
Rust kernel over generic NMP.

Authoritative engineering sources:

- [Current architecture overview](docs/architecture.md)
- [Accepted ADRs](docs/architecture/README.md)
- [Swift ownership inventory](docs/architecture/ownership.md)
- [iOS-first shared-core roadmap](Plans/2026-07-18-ios-first-rust-nmp-roadmap.md)
- [Live GitHub milestones](https://github.com/pablof7z/pod0/milestones)

The older [`docs/spec`](docs/spec/README.md) corpus is historical product/design
research. It is not evidence that a feature or architecture exists on master.

## Source map

```text
App/Sources/
├── App/          composition, root navigation, native lifecycle
├── Audio/        AVFoundation, audio session, media controls
├── Podcast/      current Swift podcast/feed models and parsing
├── Domain/       current durable Swift records
├── State/        AppStateStore and SQLite-authoritative persistence
├── Workflows/    durable jobs, leases, fencing, artifacts, recovery
├── Transcript/   transcript model, parsers, provider/native adapters
├── Knowledge/    chunking, embeddings, SQLiteVec/FTS retrieval
├── Agent/        tool schemas, validation, generated artifacts, skills
├── Services/     platform adapters and temporary application services
├── Features/     SwiftUI presentation plus tracked temporary controllers
├── Voice/        Apple audio/speech capture
└── Design/       SF typography, haptics, animation, native materials

rust/              Pod0 domain, application, facade, and isolated NMP adapter
Generated/Pod0Core generated Swift and Kotlin sources from one UniFFI artifact
BindingsSmoke/     generated-binding runtime qualification harnesses
```

`App/Widget` contains the native widget extension. `AppTests/Sources` contains
the iOS unit/integration suite.

## Build and run

Requirements:

- Xcode 26.4 or newer with an iOS 26 simulator runtime.
- Tuist 4.x.
- Rust 1.93.0 through the committed `rust-toolchain.toml`.
- An Apple Developer account for signed device/TestFlight builds.

Generate the project:

```bash
./ci_scripts/bootstrap_project.sh
```

For agent-driven simulator work, use `xcodebuildmcp` and the `Podcastr`
workspace/scheme. For an ordinary local Xcode session, open
`Podcastr.xcworkspace` after bootstrap.

Provider credentials are configured in **Settings → Intelligence → Providers**.
Keys are stored in Keychain; non-secret connection/model settings live in app
state.

## Verification

Architecture checks:

```bash
python3 scripts/check_architecture.py --self-test
```

Shared-core and generated-binding checks:

```bash
./scripts/check_rust.sh
./scripts/check_core_binding_drift.sh
./scripts/check_kotlin_core_bindings.sh
./scripts/check_core_portability.sh
```

The portability gate uses Rust 1.93.0, cargo-ndk 4.1.2, Android NDK
26.3.11579264, and Android API 23. It checks the complete workspace for
Android arm64, links facade libraries for Android arm64 and x86_64, and builds
the same Apple device/simulator XCFramework consumed by iOS. A green Android
compile is readiness evidence only; it does not open the gated Android product
phase or supersede the M5 validation decision.

iOS tests:

```bash
./ci_scripts/bootstrap_project.sh
./ci_scripts/run_tests.sh
```

Durable process-reconstruction qualification:

```bash
./scripts/test-workflow-process-reconstruction.sh
```

Every user-facing iPhone change also updates
`App/Resources/whats-new.json`. Repository-wide typography and file-length
rules live in [AGENTS.md](AGENTS.md).

## Delivery

Pushes to `master` run the test workflow and TestFlight workflow on the
self-hosted macOS runner. TestFlight signing and App Store Connect secrets are
managed by `ci_scripts/archive_and_upload.sh` and
`ci_scripts/set_github_secrets.sh`; do not store credentials in the repository.

## Screenshots

Current simulator captures are under [`docs/images`](docs/images), including
onboarding, library, discovery, show/episode detail, mini-player, Now Playing,
and agent surfaces. Captures are illustrative; master and executable tests are
authoritative for current behavior.
