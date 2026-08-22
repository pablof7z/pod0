<!-- refreshed: 2026-08-22 -->
# Architecture

**Analysis Date:** 2026-08-22

## System Overview

```text
┌────────────────────────────────────────────────────────────────┐
│                      iOS Application Layer                      │
│                      (Swift / SwiftUI)                          │
├──────────────────┬──────────────────┬───────────────┬──────────┤
│   Presentation   │   Playback &     │   Agent &     │  Voice & │
│   (Features)     │   Audio (AV)     │   Knowledge   │   Speech │
│  `App/Sources/   │ `App/Sources/    │ `App/Sources/ │ `App/Src/│
│  Features/`      │ Audio/`          │ Agent/`       │ Voice/`  │
├──────────────────┴──────────────────┴───────────────┴──────────┤
│                        AppStateStore                             │
│                 (Observation, @MainActor)                        │
│              `App/Sources/State/AppStateStore`                   │
├──────────────────────────────────────────────────────────────────┤
│                     Application State Layer                       │
│                  (Workflows, Platform Adapters)                  │
│    `App/Sources/Workflows/` `App/Sources/Services/`              │
├────────────────────┬───────────────────┬────────────────────────┤
│     Persistence    │  Pod0Facade       │   Platform Native      │
│     (SQLite)       │  (Typed UniFFI)   │   (Keychain, Files)    │
│ `App/Sources/      │ Generated Swift   │                        │
│ State/Persistence` │ bindings from     │                        │
│                    │ Pod0Core          │                        │
├────────────────────┴───────────────────┴────────────────────────┤
│                      SQLite Stores                                │
│  ┌─────────────────┬──────────────────┬──────────────────────┐  │
│  │ app-core.sqlite │ Persistence.db   │ Recall Index         │  │
│  │ (Rust auth)     │ (Swift, adjunct)  │ (Vector embeddings)  │  │
│  └─────────────────┴──────────────────┴──────────────────────┘  │
├────────────────────────────────────────────────────────────────────┤
│                      Pod0 Rust Kernel                              │
│              (Domain, Application, Facade, Storage)                │
│                   `rust/crates/`                                   │
├────────────┬──────────────────┬──────────────┬──────────────────┤
│ pod0-      │ pod0-application │ pod0-storage │ pod0-facade      │
│ domain     │ (Runtime state & │ (Migration,  │ (UniFFI          │
│ (Values,   │  command/event   │ authoritative │ contract,        │
│ policies,  │  handling, Rust  │ stores,      │ projections,     │
│ identity)  │  application     │ transactions)│ observers)       │
│            │  actor)          │              │                  │
└────────────┴──────────────────┴──────────────┴──────────────────┘
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

**Overall:** Hybrid native-plus-kernel pattern with typed async FFI boundary

**Key Characteristics:**
- **One writer per domain:** Rust owns library, listening, playback policy, transcripts, chapters, notes, clips, agents, downloads, recall; Swift retains only unmigrated settings/categories and temporary UI state
- **Projection-based rendering:** Swift never writes Rust-owned state back; views render bounded projections from Rust, send commands back
- **Single typed facade contract:** One UniFFI surface with committed version, generated Swift/Kotlin bindings, CI drift detection
- **Durable workflows over polling:** Rust owns workflow state, fences, retry/block policy; Swift adapters execute native primitives (URLSession, AVFoundation, notifications) and return observations
- **Incremental vertical-slice migration:** Each domain cutover (library → playback → transcript → agent) follows complete cycle: Swift auth → Rust auth → Swift adapters only

## Layers

**Presentation Layer:**
- Purpose: SwiftUI rendering, navigation, animation, accessibility
- Location: `App/Sources/Features/`, `App/Sources/Design/`
- Contains: Feature views, navigation stacks, sheet/sheet coordinators, transient UI state
- Depends on: AppStateStore projections, platform-specific views
- Used by: Scene/window

**Application State Layer:**
- Purpose: Observation-driven state updates, command dispatch to Rust, native lifecycle coordination
- Location: `App/Sources/State/`, `App/Sources/Workflows/`
- Contains: AppStateStore, Persistence, WorkflowClient, lifecycle handlers
- Depends on: Pod0Facade, native platform APIs (AVFoundation, URLSession, Keychain)
- Used by: Presentation, native entry points (AppDelegate, background tasks)

**Pod0 Rust Kernel:**
- Purpose: Durable state ownership, command/event processing, domain logic
- Location: `rust/crates/`
- Contains: Domain types, application actor, runtime, storage layers, migrations
- Depends on: SQLite, external host requests (via callback)
- Used by: AppStateStore commands, projections read, observations posted back to Swift

**Native Platform Layer:**
- Purpose: Execute platform primitives bounded by Rust contract
- Location: `App/Sources/Audio/`, `App/Sources/Voice/`, adapters in `App/Sources/Services/`
- Contains: AVFoundation, URLSession, Speech, Keychain, notifications, file I/O
- Depends on: Pod0Facade host request types, platform frameworks
- Used by: AppStateStore adapters, WorkflowClient, background task handlers

## Data Flow

### Primary Request Path (e.g., subscribe to feed)

1. **User triggers action** (`Features/Library/SubscribeButton.swift`) → calls AppStateStore method
2. **AppStateStore.subscribe()** → dispatches typed command to Pod0Facade
3. **Pod0Facade command handler** → routes to application actor, applies domain logic
4. **Rust store commits** → writes to SQLite, publishes event
5. **Facade notifies subscribers** → sends projection update back to Swift
6. **AppStateStore receives update** → triggers @Observable update
7. **View re-renders** with new subscription state from projection

**File references:**
- Entry: `App/Sources/Features/Library/LibraryView.swift`
- Command: `App/Sources/State/AppStateStore+Episodes.swift:subscribe()`
- Facade: `rust/crates/pod0-facade/src/runtime_commands.rs`
- Store: `rust/crates/pod0-storage/src/listening_store.rs`

### Playback Workflow

1. **Native AVFoundation start** (`Audio/PlaybackController`) → creates observation
2. **AppStateStore.observePlayback()** → sends bounded observation to facade
3. **Facade applies playback policy** → updates queue, resume targets
4. **Rust emits projected playback state** → returned to Swift for next animation frame
5. **High-frequency playhead animation** stays in AVFoundation, never crosses FFI

**File references:**
- Controller: `App/Sources/Audio/PlaybackController.swift`
- Facade: `rust/crates/pod0-facade/src/runtime_playback_commands.rs`
- Policy: `rust/crates/pod0-domain/src/playback_policy.rs`

### Transcript Workflow

1. **Episode needs transcript** → AppStateStore requests transcript preparation
2. **WorkflowClient coordination** → stages Rust command for transcript fetch/parse
3. **Native URLSession adapter** → executes fetch request, returns raw bytes
4. **Rust ingests** → parses, creates segments, chapters, semantic spans
5. **Facade projects** → returns bounded transcript with speaker/segment/word projections
6. **Swift renders** → builds UI from projection, no durable transcript copy

**File references:**
- State: `App/Sources/State/AppStateStore+Transcript.swift`
- Workflow: `App/Sources/Workflows/WorkflowClient.swift`
- Storage: `rust/crates/pod0-storage/src/transcript_store.rs`
- Facade: `rust/crates/pod0-facade/src/runtime_transcript_workflow_commands.rs`

### Agent Conversation Flow

1. **User sends message** → AppStateStore.sendAgentMessage()
2. **Rust agent handler** → validates permissions, routes to model provider
3. **Swift native adapter** → executes URLSession to OpenRouter/provider
4. **Raw model response** → returned to Rust, parsed, tool calls extracted
5. **Rust tools** → transcript search, library query executed in Rust
6. **Tool results → model** → sent back to provider for continuation
7. **Final response stored** → Rust commits conversation, artifacts, usage
8. **Facade projects** → conversation thread, message, citations returned to Swift

**File references:**
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

**ApplicationCommand (Rust):**
- Purpose: Type-safe command dispatch from Swift to Rust kernel
- Examples: `SubscribeCommand`, `PlaybackObservationCommand`, `AgentMessageCommand`
- Pattern: Sealed enum, mapped from Swift via generated bindings
- Location: `rust/crates/pod0-application/src/commands.rs`

**ProjectionRequest (Rust):**
- Purpose: Explicit requests for screen-shaped bounded data
- Examples: `LibraryProjection`, `PlaybackProjection`, `ConversationProjection`
- Pattern: Fetch-specific slices, never a full snapshot
- Location: `rust/crates/pod0-application/src/projections.rs`

**CommandEnvelope (Rust):**
- Purpose: Wrapper carrying command, identity, correlation, deadline, cancellation
- Contains: Command, CommandId, SubscriptionId (for reply routing), effect fence
- Pattern: Prevents command handlers from becoming distributed scheduler
- Location: `rust/crates/pod0-application/src/envelope.rs`

**HostRequest (Rust):**
- Purpose: Bounded request for native execution (URLSession, AVFoundation, etc.)
- Examples: `FeedHostRequest`, `PlaybackHostRequest`, `TranscriptHostRequest`
- Pattern: Raw bytes in, raw bytes out; no policy in adapter
- Location: `rust/crates/pod0-application/src/host.rs`

**AppStateStore (Swift):**
- Purpose: @MainActor @Observable state owner and projection adapter
- Pattern: Methods dispatch commands, subscribe to observables for updates
- Constraint: No direct writes to Rust stores; all mutations via AppStateStore methods
- Location: `App/Sources/State/AppStateStore.swift` (300+ lines split into extensions)

**Persistence (Swift):**
- Purpose: SQLite authority for unmigrated Swift domains
- Stores: metadata snapshot, legacy episode rows (migration only), workflow jobs, artifacts
- Pattern: Serialized background writer, atomic transactions
- Location: `App/Sources/State/Persistence.swift`

## Entry Points

**App Launch:**
- Location: `App/Sources/AppMain.swift:PodcastrApp`
- Triggers: System startup
- Responsibilities: Load AppStateStore, set up environment objects, render RootView

**Deep Linking:**
- Location: `App/Sources/App/RootView+DeepLink.swift`
- Triggers: Spotlight, URL schemes, notifications
- Responsibilities: Route to specific feature, request conversation/episode

**Background Tasks:**
- Locations: `App/Sources/Workflows/WorkflowClient.swift`, `App/Sources/App/AppDelegate.swift`
- Triggers: BGProcessingTask, BGAppRefresh, CoreLocation, notification
- Responsibilities: Reconcile workflow state, process awaiting effects, refresh feeds

**Native Platform Callbacks:**
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

**What happens:** Code calls facade query method repeatedly in a loop or Timer to wait for a workflow
**Why it's wrong:** Rust owns workflows; Swift is an observer. Polling becomes a second writer of completion policy.
**Do this instead:** Subscribe to facade observable, let Rust emit event when workflow completes. Use `CoreWakeReason` for timer-triggered work instead of Swift polling. Reference: `rust/crates/pod0-facade/src/runtime_core_wakes.rs`

### Direct Persistence Writes Outside State

**What happens:** Feature code calls `Persistence.save()` directly instead of going through AppStateStore
**Why it's wrong:** Breaks mutation boundary enforcement; AppStateStore.mutateState() is the only allowed direct write location
**Do this instead:** Add method to AppStateStore, dispatch command to Rust if needed, let AppStateStore handle persistence. Enforced by `AppStateMutationBoundaryTests`. Reference: `App/Sources/State/`

### Caching Projection Data in Swift View State

**What happens:** View fetches projection once, stores it in @State, renders without re-fetching on facade changes
**Why it's wrong:** Projection becomes stale when Rust state updates; view displays out-of-date data
**Do this instead:** Always derive view state from AppStateStore environment or @State that observes facade observable. Reference: `App/Sources/Features/Library/LibraryView.swift`

### Transcript/Chapter Authority in Swift UI

**What happens:** Feature code holds a mutable reference to a transcript artifact downloaded by native adapters
**Why it's wrong:** Swift becomes a second writer; next migration or restart loses consistency
**Do this instead:** Request transcript projection from facade, render from bounded projection. Swift reads only via Rust-selected immutable artifact. Reference: `rust/crates/pod0-facade/src/runtime_transcript_workflow_commands.rs`

### Workflow Retry Logic in Native Adapters

**What happens:** URLSession adapter catches error, decides to retry, resubmits request
**Why it's wrong:** Rust owns retry policy; adapter becomes a second coordinator
**Do this instead:** Adapter returns raw observation (success/failure/timeout); Rust examines and schedules retry via `CoreWakeReason::FeedFetchRetry`. Reference: `rust/crates/pod0-facade/src/runtime_feed_leased_observations.rs`

## Error Handling

**Strategy:** Typed bounded failure states in projections, no throwing across FFI

**Patterns:**
- **Workflow failures:** Captured in `WorkflowProjection.failureState` enum, rendered in UI as bounded status
- **Host request failures:** Returned as `HostObservation::Failure` with error kind; Rust applies retry policy
- **Migration errors:** Logged to console, app starts in degraded mode if critical store cannot open
- **Validation errors:** Commands rejected before commit; agent permissions, command fingerprints validated pre-execution

Reference: `rust/crates/pod0-facade/src/runtime_failure.rs`, `rust/crates/pod0-domain/src/listening_error.rs`

## Cross-Cutting Concerns

**Logging:** Native code uses `os.log` with subsystem identifiers; Rust logs via `tracing` in tests, silent in production (logs would expose PII)

**Validation:** Command validation at Rust boundary (fingerprint mismatch, permission denied, schema violation); no validation in Swift adapters

**Authentication:** Keychain holds provider secrets; Rust never accesses; native adapter passes provider-supplied keys to host requests

---

*Architecture analysis: 2026-08-22*
