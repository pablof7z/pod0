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

Master is currently Swift-only. It does **not** yet contain a Rust workspace,
UniFFI bindings, Kotlin source, an Android application, or an active generic NMP
integration. Those enter through the staged roadmap below, not through the
deleted legacy Swift NMP surface.

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
```

`App/Widget` contains the native widget extension. `AppTests/Sources` contains
the iOS unit/integration suite.

## Build and run

Requirements:

- Xcode 26.4 or newer with an iOS 26 simulator runtime.
- Tuist 4.x.
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
python3 scripts/check_architecture_ownership.py
python3 scripts/check_ui_storage_boundary.py --self-test
python3 scripts/check_ui_storage_boundary.py
```

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
