# Phase 1: Headless Host Crates - Research

**Researched:** 2026-08-22
**Domain:** Rust workspace integration — committing six untracked host crates, consolidating tokio runtimes, deduplicating HTTP clients, and closing an approval-parity gap in a UniFFI-adjacent CLI harness
**Confidence:** HIGH (every claim below is grounded in `Read`/`grep` against the actual working tree this session — file paths and line numbers are current as of 2026-08-22, not the project-level research's earlier pass)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01 Credential Handling:** `pod0-cli` keeps reading provider credentials from environment variables (`POD0_OPENAI_API_KEY`, `POD0_OLLAMA_BASE_URL`, etc.) for this milestone, rather than migrating to the `pod0-system-hosts` keyring integration already used for Nostr keys. Reversible.
- **D-02 Observability:** Add `tracing`/`tracing-subscriber` instrumentation to `pod0-live-hosts` and `pod0-tts-host` in this phase (per-call request/cancellation correlation IDs, never the API key or raw body), with a subscriber installed only in `pod0-cli`. The synchronous `Pod0Facade` stays silent — no logging crosses the FFI boundary.
- **D-03 Tokio Runtime Consolidation:** Exactly one `tokio::runtime::Runtime` is constructed once at the `pod0-cli` binary's entrypoint (and at any future host-process boundary) and threaded down to host adapters as a `Handle`, rather than each adapter lazily calling `tokio::runtime::Handle::current()`. Costly to reverse — decide now.
- **D-04 CI Scope:** The existing `cargo deny` job is confirmed to cover the six new crates and their new dependencies (`reqwest`, `tokio`) as part of this phase — not spun up as new CI infrastructure, just verified in-scope.

### Claude's Discretion

- Exact tracing correlation-ID scheme (span naming, field names) — follow whatever `tracing` idiom fits `pod0-live-hosts`'s existing error types (`ProviderError`, `NetworkError`, `ProtocolError`).
- Whether the single shared `tokio::runtime::Runtime` is multi-thread or current-thread flavored — informed by the actual concurrency needs found below (see Architecture Patterns → Runtime Consolidation).

### Deferred Ideas (OUT OF SCOPE)

- Full keyring-backed credential storage for `pod0-cli` (replacing env vars).
- Voice-specific turn provenance tagging for Run History — v2, unrelated to this phase.

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| HOST-01 | Six new Rust host crates committed, compile cleanly standalone + in workspace | See "Workspace Membership Gap" — exact `Cargo.toml` diff needed, and why 3 of 6 crates currently cannot join the workspace as-is |
| HOST-02 | All host crates build/test/lint together in one CI job | See "CI Is Already Wired — No New Job Needed" — `scripts/check_rust.sh` already runs `--workspace --all-targets`; the gap is purely `Cargo.toml` membership |
| HOST-03 | HTTP provider clients share one pooled `reqwest::Client` per process with explicit timeouts | See "HTTP Client Duplication — Exact Shape" — `HostExecutor` holds a `reqwest::blocking::Client` *and* `LiveHosts` holds a second, independent async `reqwest::Client`; `LiveHosts::http_get`/`openai_chat`/`ollama_chat` already exist as migration targets |
| HOST-04 | All six host crates share exactly one `tokio` runtime instance when linked into the same process | See "Two Runtimes Already Collide Today" — verified via full-workspace grep, exact construction sites and mismatched feature sets |
| HOST-05 | `pod0-cli::HostExecutor` reaches approval/capability-execution parity with `CoreAgentHost` | See "Approval Parity Is a One-Line Fix" and "Capability-Execution Parity Has a Hard Ceiling" — native capability executor depends on iOS-only audio/TTS types; exact scoping recommendation given |

</phase_requirements>

## Summary

The three-crate STACK/PITFALLS/ARCHITECTURE research from earlier today is directionally correct but was written before this session's direct inspection; several of its specifics have since shifted or needed sharper detail. The most consequential finding: **three of the six crates (`pod0-nostr-host`, `pod0-system-hosts`, `pod0-tts-host`) carry their own `[workspace]` table in their `Cargo.toml`**, which is the mechanism keeping them out of the parent workspace today — `rust/Cargo.toml`'s `members` list currently only contains `pod0-live-hosts`, `pod0-portable-media`, and `pod0-cli` (added in the current uncommitted diff). HOST-01 cannot be satisfied by editing `rust/Cargo.toml` alone; the `[workspace]` stanza must also be deleted from each of the three standalone crates' manifests, or Cargo will refuse to nest them.

Second: the tokio-runtime-consolidation risk PITFALLS.md predicted is not hypothetical — it is **already present** in the current working tree. `pod0-cli/src/host.rs:45` and `pod0-portable-media/src/source.rs:95` each independently call `tokio::runtime::Builder::new_current_thread()`, and `pod0-cli` depends on `pod0-portable-media` directly, so both runtimes already coexist in the same process today, unconsolidated.

Third: the HTTP-client duplication is not "two blocking clients" as earlier research phrased it — it is one `reqwest::blocking::Client` (`HostExecutor.client`, used for feed/library fetch, iTunes search, *and* agent chat completions) sitting alongside one independent async `reqwest::Client` inside `LiveHosts` (used for embeddings/rerank). `pod0-live-hosts` already exposes `LiveHosts::http_get` (conditional-GET/redirect-aware), `openai_chat`, and `ollama_chat` — all the async equivalents needed to delete the blocking client entirely and route every one of `HostExecutor`'s HTTP call sites through the single `LiveHosts` instance it already holds.

Fourth: HOST-05's "capability-execution parity" cannot be total. `CoreAgentHost`'s native `LiveCoreAgentCapabilityExecutor` (`App/Sources/Core/CoreAgentCapabilityExecutor.swift`) depends on `AudioEngine`, `PlaybackState`, and `ElevenLabsTTSClient` — none of which have a headless equivalent. What *is* achievable and already half-built: `pod0-cli/src/host/search.rs` already implements the exact same iTunes-directory HTTP search `LiveCoreAgentCapabilityExecutor`'s `searchPodcastDirectory` case performs, but it is currently wired only to a CLI `search` subcommand, not to `HostRequest::ExecuteAgentCapability`. Approval parity, by contrast, is a one-line fix: native `AgentApprovalCoordinator.requestApproval` unconditionally returns `.approve` — there is no conditional logic to port, just a variant swap in `pod0-cli/src/host.rs:101` from `AgentApprovalDecision::Deny` to `AgentApprovalDecision::Approve`.

**Primary recommendation:** Sequence the phase as (1) workspace membership fix — delete the three `[workspace]` stanzas, add all six crates + their transitive deps to `rust/Cargo.toml`; (2) runtime consolidation — delete `pod0-portable-media`'s internal runtime, thread a `tokio::runtime::Handle` into its constructor instead, sourced from the one `Runtime` `HostExecutor::new` already builds; (3) HTTP dedup — delete `HostExecutor.client` entirely, route `fetch_feed`/`fetch_library`/`search.rs`/`agent_http.rs` through `host.live` (`LiveHosts`) via `host.runtime.block_on(...)`; (4) approval fix — one-line `Deny` → `Approve`; (5) capability scoping — wire `ExecuteAgentCapability`'s `Search{tool: searchPodcastDirectory}` action to the existing `search.rs` logic, return a well-defined `Failed` (not blanket `Unsupported`) for capability actions with no headless equivalent; (6) tracing — instrument `pod0-live-hosts`/`pod0-tts-host` per D-02, install a `tracing_subscriber` in `pod0-cli::main`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| HTTP provider calls (chat, embeddings, rerank, feed/library fetch) | Rust host crate (`pod0-live-hosts`) | `pod0-cli` (headless caller) | Async, pooled, timeout-bound; both native (`CoreAgentHost`/`URLSession`) and headless (`pod0-cli`) callers must hit the same durable `Retryability`/`ProviderError` contract |
| Tokio runtime ownership | `pod0-cli` binary entrypoint (`HostExecutor::new`) | Any future iOS-linked host-process boundary | D-03 locks this: exactly one `Runtime`, held by the process boundary, `Handle` threaded down |
| Approval decisioning | Rust durable core (`pod0-application`) issues the proposal; the *executor* (native `CoreAgentHost` or headless `pod0-cli::HostExecutor`) just answers `Approve`/`Deny`/`Dismiss` | — | Executors never author policy — they mirror the same trivial "approve every exact proposal" behavior `AgentApprovalCoordinator` already implements |
| Capability execution (search, playback, TTS generation) | Native (`CoreAgentHost`/`LiveCoreAgentCapabilityExecutor`) for anything touching `AudioEngine`/`PlaybackState`/`ElevenLabsTTSClient` | Headless (`pod0-cli`) only for the HTTP-only subset (`searchPodcastDirectory`) | No headless audio engine exists; full parity is architecturally impossible, not just unimplemented — scope HOST-05's capability half to what's actually replicable |
| CI build/test/lint gating | `scripts/check_rust.sh`, invoked from the single `test` job in `.github/workflows/test.yml` | — | Already runs `--workspace --all-targets`; HOST-02 is a `Cargo.toml` membership problem, not a CI-infrastructure problem |

## Workspace Membership Gap

**[VERIFIED: rust/Cargo.toml:1-16, read this session]**
```toml
[workspace]
resolver = "2"
members = [
    "crates/pod0-domain",
    "crates/pod0-application",
    "crates/pod0-storage",
    "crates/pod0-recall-index",
    "crates/pod0-facade",
    "crates/pod0-live-hosts",
    "crates/pod0-portable-media",
    "crates/pod0-cli",
    "crates/pod0-bdd",
    "crates/uniffi-bindgen",
]
```
`pod0-nostr-host`, `pod0-system-hosts`, `pod0-tts-host` are **absent** from `members`. This is not an oversight fixable by adding three lines — each of those three crates' own `Cargo.toml` ends with:

**[VERIFIED: rust/crates/pod0-nostr-host/Cargo.toml, rust/crates/pod0-system-hosts/Cargo.toml, rust/crates/pod0-tts-host/Cargo.toml, read this session]**
```toml
[workspace]
```
An empty `[workspace]` table makes a crate its own workspace root. Cargo refuses to add a crate that already declares `[workspace]` as a member of another workspace (`error: multiple workspace roots found`). `pod0-system-hosts`'s manifest even has a comment confirming this is deliberate:
```
# This crate is deliberately self-contained until the parent workspace adopts it.
[workspace]
```
(Same comment, same mechanism, in `pod0-tts-host`. `pod0-nostr-host` has the bare `[workspace]` with no comment.)

**Required for HOST-01:** delete the `[workspace]` table (and any now-redundant `[package]` field duplication vs. `workspace.package` inheritance — `pod0-nostr-host`/`pod0-system-hosts`/`pod0-tts-host` currently hardcode `version = "0.1.0"`, `edition = "2024"`, etc. per-crate rather than using `version.workspace = true` like `pod0-cli` already does) from all three crates, then add all six crate paths to `rust/Cargo.toml`'s `members`.

**New workspace.dependencies needed:** the three newly-joining crates pull dependencies not yet in `rust/Cargo.toml`'s `[workspace.dependencies]` table — `futures-util` (two different pinned versions across `pod0-live-hosts` `=0.3.33` and `pod0-nostr-host` `=0.3.34` — **must be reconciled to one version** before joining a single workspace, since Cargo will otherwise resolve two versions in the dependency graph, which `cargo-deny`'s `[bans] multiple-versions = "warn"` will flag), `k256`, `log`, `tokio-tungstenite`, `rand_core`, `keyring-core`, `cap-primitives`, `cap-std`, `notify-rust`, `zbus-secret-service-keyring-store`, `windows-native-keyring-store`, `apple-native-keyring-store`, `mac-usernotifications`, `objc2-foundation`, `hound`, `rodio`, `thiserror` (two pinned versions again: `pod0-portable-media` uses `=2.0.18`, `pod0-system-hosts` uses `=2.0.20` — reconcile), `futures-util` sink feature.

**Standalone-build requirement (HOST-01's "compiles standalone outside the workspace"):** once the `[workspace]` stanza is removed from a crate, `cargo build -p <crate>` from *inside* the parent workspace still works (that's the normal case), but building the crate directory in isolation (`cd crates/pod0-nostr-host && cargo build`) will only succeed if the crate's own `Cargo.toml` has concrete version pins for every workspace-inherited field, not `field.workspace = true` — because outside the workspace there is no `[workspace.package]`/`[workspace.dependencies]` to inherit from. Verify this literally: after the `[workspace]` deletion + `members` addition, run `cd rust/crates/pod0-nostr-host && cargo build` (and same for the other two) as a standalone sanity check — a crate that used `version.workspace = true` will fail this exact check the moment `[workspace]` is deleted from its own manifest, so the planner must either keep concrete literals in these three crates' manifests (not convert them to `.workspace = true` like `pod0-cli`) or accept that "standalone" means "buildable via `cargo build -p` from the repo root," not "buildable from a bare `cd` into the crate directory." **[ASSUMED — the two readings of "compiles standalone outside the workspace" produce different manifest requirements; recommend the planner pick the `cargo build -p` interpretation, matching how `pod0-live-hosts`/`pod0-portable-media`/`pod0-cli` already do it with `.workspace = true`, and treat literal-standalone-cd as not required, since CI never does a bare `cd` build.]**

## Two Runtimes Already Collide Today

**[VERIFIED: rust/crates/pod0-cli/src/host.rs:31,45-49 and rust/crates/pod0-portable-media/src/source.rs:90-105, read this session]**

```rust
// crates/pod0-cli/src/host.rs
pub(crate) struct HostExecutor {
    client: Client,               // reqwest::blocking::Client
    live: LiveHosts,
    runtime: tokio::runtime::Runtime,
    config: HostConfig,
}
impl HostExecutor {
    pub(crate) fn new(config: HostConfig) -> Result<Self, CliError> {
        ...
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| CliError::new("runtime", "async runtime initialization failed", true))?;
        Ok(Self { client, live, runtime, config })
    }
```

```rust
// crates/pod0-portable-media/src/source.rs
fn from_client_builder(...) -> Result<Self> {
    let client = client_builder.build()?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    Ok(Self {
        http: Arc::new(HttpTransport { client, runtime: Some(runtime) }),
        maximum_response_bytes,
    })
}
```

`pod0-cli`'s `Cargo.toml` depends directly on `pod0-portable-media`, so both `Builder::new_current_thread()` calls execute inside the same `pod0-cli` process today (whenever both a `HostExecutor` and an `HttpMediaSource` are constructed — check `crates/pod0-cli/src/host/playback.rs` for whether it constructs `HttpMediaSource`). This is exactly Pitfall 4's predicted failure shape, already realized. **Grep note for the planner:** a naive `grep -rn "Runtime::new\|#\[tokio::main\]"` (the exact pattern PITFALLS.md's warning-signs section suggests) returns **zero hits** in this codebase — both runtimes are built via `tokio::runtime::Builder::new_current_thread()...build()`, not `Runtime::new()`. The correct audit pattern is `grep -rn "tokio::runtime::Builder::new_current_thread\|tokio::runtime::Builder::new_multi_thread\|Runtime::new\|#\[tokio::main\]"` — verified this session to return exactly these two hits and no others across all six crates.

**tokio feature-set mismatch (corroborating signal, [VERIFIED: each crate's Cargo.toml, read this session]):**

| Crate | tokio features (`[dependencies]`) |
|---|---|
| `pod0-cli` | `["rt", "time"]` |
| `pod0-live-hosts` | `["fs", "io-util", "macros", "rt-multi-thread", "sync", "time"]` |
| `pod0-nostr-host` | `["io-util", "net", "rt", "time"]` |
| `pod0-portable-media` | `["net", "rt", "time"]` |
| `pod0-system-hosts` | (no tokio dependency) |
| `pod0-tts-host` | `["fs", "io-util", "macros", "sync", "time"]` |

Note `pod0-live-hosts` pulls `rt-multi-thread` while `pod0-cli`'s own runtime is explicitly `new_current_thread()` — Cargo unifies feature flags across the dependency graph (features are additive/union across the whole build), so the *compiled* tokio in `pod0-cli`'s binary already has `rt-multi-thread` available even though `HostExecutor` chooses not to use it; this is not itself a bug, but it means the "one runtime, one Handle" fix must pick a single flavor deliberately (see Recommendation below) rather than relying on feature unification to paper over the fact that two separate runtime *instances* still exist.

**Recommended consolidation shape:** Keep `HostExecutor::new`'s `Runtime` as the sole owner (matches D-03 "constructed once at `pod0-cli`'s binary entrypoint"). Change `pod0-portable-media`'s `HttpMediaSource` constructor to accept a `tokio::runtime::Handle` parameter instead of building `Builder::new_current_thread()` internally, and change `HttpTransport.runtime: Option<Runtime>` to `handle: tokio::runtime::Handle`. `HostExecutor::new` then calls `HttpMediaSource::new_with_handle(options, host_runtime.handle().clone())` (or equivalent) instead of whatever unqualified `HttpMediaSource::new(...)` call currently supplies its own runtime — locate that call site in `crates/pod0-cli/src/host/playback.rs` before writing the plan's task list, since that file (424 lines) was not fully read this session and is where the wiring point lives.

**Current-thread vs multi-thread (Claude's Discretion per CONTEXT.md):** `pod0-cli`'s existing choice is `new_current_thread()`, and every actual `.block_on` caller in this codebase (`host/recall.rs`'s three `block_on` sites, `pod0-portable-media/source.rs`'s one `block_on` site) is a single blocking-thread-driven synchronous host pump — there is no evidence of a need for true multi-thread parallelism inside the CLI's host-effect dispatch (`app/host_loop.rs` was not read this session; verify it dispatches one `HostRequestEnvelope` at a time before locking this in, but the existing `new_current_thread()` choice is a reasonable default to keep, not something this phase needs to change). **[ASSUMED — recommend current-thread, but flag `app/host_loop.rs`'s dispatch loop as unread and worth a 30-second grep before the plan finalizes this.]**

## HTTP Client Duplication — Exact Shape

**[VERIFIED: rust/crates/pod0-cli/src/host.rs:18,28-49,175,223 and rust/crates/pod0-cli/src/host/agent_http.rs:50,113-117 and rust/crates/pod0-cli/src/host/search.rs:34-38, read this session]**

`HostExecutor.client` is `reqwest::blocking::Client` (imported `use reqwest::blocking::{Client, Response};` at `host.rs:18`), built once in `HostExecutor::new` (`host.rs:37-42`) with a 15s connect timeout, 90s request timeout, and a 10-redirect policy. It is used at exactly three call sites:
1. `fetch_feed` (`host.rs:175`) — RSS/Atom feed GET with conditional-GET headers
2. `fetch_library` (`host.rs:223`) — library document GET
3. `agent_http.rs::execute_openai`/`execute_ollama` (lines 50, 113-117) — agent chat completions to OpenAI-compatible/Ollama endpoints
4. `search.rs::search` (line 34-38) — iTunes podcast-directory search

Separately, `HostExecutor.live: LiveHosts` (`pod0-live-hosts::LiveHosts`) owns its own independent async `reqwest::Client`, built in `LiveHosts::new` (`crates/pod0-live-hosts/src/client.rs:28-35`) with a 15s connect timeout, `Policy::none()` for redirects (deliberately does not follow redirects at the low-level client — `follow_get` handles redirect logic manually), and a `pod0-live-hosts/{version}` user agent. This is used today only via `host.runtime.block_on(...)` for embeddings/rerank in `host/recall.rs` (lines 52, 56, 122, 126, 202).

**These are two separately-pooled, separately-configured `reqwest::Client` instances live in the same `pod0-cli` process simultaneously.** `LiveHosts` already exposes the async methods needed to delete `HostExecutor.client` entirely:

| Current blocking call site | Replacement `LiveHosts` method | **[VERIFIED: signature, read this session]** |
|---|---|---|
| `host.rs::fetch_feed` (conditional GET, ETag/Last-Modified) | `LiveHosts::http_get(HttpGetRequest{url, accept, entity_tag, last_modified, options}, &cancellation)` — `crates/pod0-live-hosts/src/http.rs:97-118` | `HttpGetRequest{ url: String, accept: Option<String>, entity_tag: Option<String>, last_modified: Option<String>, options: RequestOptions }` (`http.rs:38-44`); handles redirects, conditional-GET headers, and returns `HttpGetResponse{ evidence: HttpEvidence, body: Vec<u8> }` |
| `host.rs::fetch_library` | Same `LiveHosts::http_get` (accept header, no conditional-GET needed — pass `None`/`None` for `entity_tag`/`last_modified`) | — |
| `agent_http.rs::execute_openai` | `LiveHosts::openai_chat(OpenAiChatRequest{endpoint, chat}, &cancellation)` — `crates/pod0-live-hosts/src/openai_chat.rs:13-39` | `OpenAiChatRequest{ endpoint: ProviderEndpoint, chat: ChatRequest }`; `ProviderEndpoint::bearer(url, SecretString)` or `::unauthenticated(url)` (`provider.rs:47-64`); `ChatRequest{ model, messages: Vec<ChatMessage>, tools, tool_choice, temperature, maximum_completion_tokens, timeout, limits: HttpLimits, maximum_output_bytes }` (`chat.rs:71-81`) |
| `agent_http.rs::execute_ollama` | `LiveHosts::ollama_chat(OllamaChatRequest{endpoint, chat}, &cancellation)` — `crates/pod0-live-hosts/src/ollama_chat.rs:13-36` | Same shapes as above, `OllamaChatRequest` mirrors `OpenAiChatRequest` |
| `search.rs::search` | Either keep as-is (out of the phase's literal HTTP-provider-client scope — iTunes isn't a "provider client" in the HOST-03 sense) or also route through `LiveHosts::http_get` for full one-client-per-process compliance | — |

**Response mapping detail (load-bearing for the planner, not optional):** `agent_http.rs`'s current hand-rolled JSON parsing (`value.pointer("/choices/0/message")`, manual `content`/`tool_calls`/`usage` extraction, `contains_tool_call` → hard failure) duplicates logic `LiveHosts::openai_chat`/`ollama_chat` already do correctly and more completely — including **actually parsing tool calls** (`ChatResponse.tool_calls: Vec<ToolCall>`) rather than rejecting any response containing one. Today, `agent_http.rs::completed()` (line 165-177) hardcodes `proposed_tool_call: None` unconditionally on `HostObservation::AgentModelCompleted`, meaning **the headless host cannot exercise tool-call turns at all** — a gap not previously called out in STACK.md/ARCHITECTURE.md. Migrating to `LiveHosts` fixes this as a side effect: `ChatResponse.tool_calls` is already populated; the planner should map `tool_calls.first()` (or however `AgentModelCompleted::proposed_tool_call` expects multi/single-call shape — check `pod0_facade::HostObservation::AgentModelCompleted`'s exact field type, not read this session) into the observation instead of leaving it `None`. **[VERIFIED: agent_http.rs:174 `proposed_tool_call: None` is unconditional, read this session — ASSUMED that this is an unintentional gap the migration should close, not a deliberate scope limit; flag for user confirmation if the plan surfaces it as a bigger change than "delete the blocking client."]**

`ProviderEndpoint`'s `bearer_token: Option<SecretString>` requires wrapping `host.config.openai_api_key` (currently `Option<&str>` per `agent_http.rs:51`) in `pod0_live_hosts::SecretString` — check `crates/pod0-live-hosts/src/secret.rs` for the exact constructor (`SecretString::new`, per the earlier grep hit `secret.rs:9`).

## Approval Parity Is a One-Line Fix

**[VERIFIED: rust/crates/pod0-cli/src/host.rs:96-103 and App/Sources/Agent/AgentApprovalCoordinator.swift:14-18, read this session]**

Headless (current):
```rust
HostRequest::PresentAgentApproval { approval } => {
    HostExecution::Observed(Box::new(HostObservation::AgentApprovalObserved {
        turn_id: approval.turn_id,
        proposal_id: approval.proposal.proposal_id,
        proposal_digest: approval.proposal.proposal_digest,
        decision: AgentApprovalDecision::Deny,
    }))
}
```

Native (what it must match):
```swift
final class AgentApprovalCoordinator: CoreAgentApprovalPresenting {
    func requestApproval(_ request: AgentApprovalRequest) async -> AgentApprovalDecision {
        .approve
    }
}
```
Native has no conditional logic — every proposal, unconditionally, is `.approve` — because Pod0 "does not interrupt the owner to authorize the owner's own agent" (per the Swift file's own doc comment). Parity is achieved by changing `AgentApprovalDecision::Deny` to `AgentApprovalDecision::Approve` at `host.rs:101`. `AgentApprovalDecision` is a 3-variant `uniffi::Enum` (`Approve`, `Deny`, `Dismiss` — **[VERIFIED: rust/crates/pod0-application/src/agent_contract.rs:31-36, read this session]**), so this compiles without further contract changes.

## Capability-Execution Parity Has a Hard Ceiling

**[VERIFIED: rust/crates/pod0-cli/src/host.rs:104-108, App/Sources/Core/CoreAgentCapabilityExecutor.swift:1-136, rust/crates/pod0-application/src/agent_turn_contract.rs:86-110, rust/crates/pod0-application/src/agent_contract.rs:66-135 (partial), read this session]**

Headless today unconditionally fails every capability:
```rust
HostRequest::ExecuteAgentCapability { .. } => {
    HostExecution::Observed(Box::new(unsupported_observation(
        "agent capability execution is unavailable in the headless host",
    )))
}
```

Native's `LiveCoreAgentCapabilityExecutor.execute(_:)` switches on `AgentCapabilityRequest.action: AgentToolAction` and handles (non-exhaustively, from source read this session): `.search(tool: .searchPodcastDirectory, ...)` (real iTunes HTTP call via `PodcastCatalogEpisodeSearchService`, optionally enqueues playback), `.playEpisode` (mutates live `PlaybackState`), `.noArguments(tool: .pausePlayback)` (mutates live `AudioEngine`), `.setPlaybackRate` (mutates live `AudioEngine`), `.generateTtsEpisode` (calls `ElevenLabsTTSClient`, writes to `CoreAgentGeneratedAudioFileStore`). Everything else returns `.failed(safeDetail: "Native agent capability is unsupported")` even natively.

**None of `AudioEngine`, `PlaybackState`, or `ElevenLabsTTSClient` have a headless/Rust equivalent** — full parity is not a missing-implementation problem, it is an architectural ceiling (matches REQUIREMENTS.md's own Out-of-Scope: "Moving STT, TTS, or `AVAudioSession` ownership into Rust"). The one case that *is* fully replicable headlessly: `AgentToolAction::Search { tool, query, scope, limit, execute_first }` when `tool == searchPodcastDirectory` is pure HTTP (iTunes search API), and `pod0-cli/src/host/search.rs` (already committed as an untracked file, 174 lines) **already implements the identical iTunes search** — currently wired only to a `search` CLI subcommand (`crates/pod0-cli/src/protocol::PodcastSearchResultDto`), not to `HostRequest::ExecuteAgentCapability`.

**[VERIFIED: AgentToolAction::Search variant, rust/crates/pod0-application/src/agent_contract.rs:75-81, read this session]**
```rust
Search {
    tool: AgentToolName,
    query: String,
    scope: Option<String>,
    limit: u16,
    execute_first: bool,
},
```
and the request/response envelope:
**[VERIFIED: rust/crates/pod0-application/src/agent_turn_contract.rs:86-110, read this session]**
```rust
pub struct AgentCapabilityRequest {
    pub turn_id: AgentTurnId,
    pub proposal_id: AgentProposalId,
    pub proposal_digest: ContentDigest,
    pub execution_fence_id: AgentExecutionFenceId,
    pub execution_mode: AgentCapabilityExecutionMode,
    pub generated_audio_target: Option<AgentGeneratedAudioTarget>,
    pub action: AgentToolAction,
}
pub enum AgentCapabilityOutcome {
    Succeeded { bounded_result: String },
    GeneratedAudioStaged { evidence: AgentGeneratedAudioEvidence },
    Failed { safe_detail: Option<String> },
    Cancelled,
    OutcomeAmbiguous,
}
```

**Recommended HOST-05 capability scope for this phase:** route `AgentToolAction::Search{tool: searchPodcastDirectory, ..}` to `search::search(host, &query, limit)`, mapping its `Vec<PodcastSearchResultDto>` result into `AgentCapabilityOutcome::Succeeded{bounded_result}` (JSON-serialized, size-bounded — check `search.rs`'s existing `MAX_SEARCH_RESPONSE_BYTES`/`MAX_SEARCH_RESULTS` constants already enforce a bound). `execute_first: true` would additionally need to enqueue playback — check whether `pod0-cli/src/host/playback.rs` (424 lines, not read this session) has an in-memory queue primitive reusable here; if not, treat `execute_first` as a **[Claude's Discretion]** narrowing — accept the search but ignore `execute_first` headlessly (returning results only), which is a defensible, testable scope boundary distinct from returning `Unsupported` for the whole action. For every other `AgentToolAction` variant (`PlayEpisode`, `SetPlaybackRate`, `GenerateTtsEpisode`, etc.), return `AgentCapabilityOutcome::Failed{safe_detail: Some("...")}` with a specific, per-action message — not the current blanket `unsupported_observation` — so headless test assertions can distinguish "we haven't scoped this" from "this genuinely has no headless equivalent." **[ASSUMED — this scoping recommendation should be confirmed with the user before the plan locks it in, since "capability-execution parity" in the phase's own Success Criteria could be read as requiring more; flag as an open question, not a locked decision.]**

## CI Is Already Wired — No New Job Needed

**[VERIFIED: .github/workflows/test.yml:1-95, scripts/check_rust.sh:1-21, read this session]**

There is exactly one relevant CI job (`test`, running on `macos-26`), which calls `./scripts/check_rust.sh`:
```bash
cd "$REPO_ROOT/rust"
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
python3 "$SCRIPT_DIR/check_rust_dependency_policy.py"
python3 "$SCRIPT_DIR/check_rust_facade_boundary.py"
python3 "$SCRIPT_DIR/check_rust_schema_policy.py"
cargo deny check
cargo audit
```
This already runs `--workspace --all-targets` (`cargo build` is implied by `clippy`/`test`, both of which compile everything). **HOST-02 requires zero new CI YAML.** The only action needed is making `rust/Cargo.toml`'s `members` list include all six crates (see Workspace Membership Gap) — once that's true, this existing single job automatically covers them.

`check_rust_dependency_policy.py` discovers manifests via `sorted((rust / "crates").glob("*/Cargo.toml"))` (`check_rust_dependency_policy.py:60`, [VERIFIED, read this session]) — a glob over every directory under `crates/`, not a hardcoded crate list. This means the policy script **already inspects** `pod0-nostr-host`/`pod0-system-hosts`/`pod0-tts-host` today even though they're outside the workspace (their manifests physically exist on disk) — no script changes needed for HOST-01/02. Same applies to `check_rust_facade_boundary.py` (walks facade source files by token-scan, not a crate allowlist).

D-04 (confirmed in-scope, not new infra) is correct: `cargo deny check` runs against whatever `cargo metadata` resolves for the *actual* workspace members — once the three crates join `members`, `cargo deny`'s existing `deny.toml` (`[licenses] allow` list already includes MIT/Apache-2.0/BSD/ISC/etc., which covers every new crate's license per each `Cargo.toml`'s inline license comments) applies to them automatically. No `deny.toml` edit is required unless a genuinely new license family appears — spot-check: `keyring-core`, `cap-primitives`/`cap-std`, `notify-rust`, `zbus-secret-service-keyring-store`, `k256`, `tokio-tungstenite` are all MIT/Apache-2.0-family per crates.io metadata **[CITED: crates.io package pages — not independently re-verified this session beyond the STACK.md pass; treat as MEDIUM confidence, spot-check during `cargo deny check`'s first real run against the joined workspace since that is the authoritative check]**.

**Cargo.lock note:** `rust/Cargo.lock` is currently modified in the working tree (per `git status`) but the three joining crates are not yet resolved into it (they're not workspace members yet). Adding them to `members` will trigger a `Cargo.lock` update touching potentially many transitive entries — this diff will be large and should be reviewed as its own commit-worthy change, not silently squashed into a source-code commit.

## Common Pitfalls

See `.planning/research/PITFALLS.md` pitfalls 3–6 for the general pattern-level analysis (unpooled/untimed HTTP clients, multi-runtime FFI deadlocks, untested-together crates, bootstrap/effect-outbox signature risk) — those still apply. This section adds only what direct inspection this session surfaced beyond that document:

### Pitfall 7: The two runtimes aren't hypothetical — they already coexist, silently, in a currently-passing build

**What goes wrong:** Because `pod0-cli` currently only calls into `pod0-portable-media`'s `HttpMediaSource` and `HostExecutor`'s own runtime from disjoint code paths (never truly concurrently in a way today's tests exercise), the dual-runtime problem hasn't yet produced an observable panic. It is a latent bug already merged into the working tree, not a risk to prevent — it needs an active fix, not a design constraint to enforce on new code.

**Why it happens:** `pod0-portable-media` was written to be usable outside `pod0-cli` too (it has no `pod0-cli` dependency), so its author reasonably gave `HttpMediaSource` its own runtime for standalone testability. That reasonable per-crate choice becomes a bug the moment two crates each making that same reasonable choice get linked together.

**How to avoid:** Don't just document "avoid multiple runtimes" — the plan must include a concrete task that deletes `pod0-portable-media/src/source.rs:95`'s `Builder::new_current_thread()` and threads a `Handle` in from the caller (see "Two Runtimes Already Collide Today" above for the exact constructor change).

**Warning signs:** None visible from `cargo test` today (each crate's own unit tests build their own `HttpMediaSource`/`HostExecutor` in isolation) — this is exactly Pitfall 5's "compiles and tests pass per-crate, never exercised together" pattern, confirmed concretely rather than hypothetically here.

### Pitfall 8: `agent_http.rs`'s hand-rolled tool-call rejection silently caps headless testing to non-tool turns

**What goes wrong:** Because `execute_openai`/`execute_ollama` (`agent_http.rs`) hard-fail on any `tool_calls` in the provider response (`contains_tool_call` check) and `completed()` hardcodes `proposed_tool_call: None`, no headless test written against the current code can exercise a tool-call-producing agent turn — which is exactly the kind of turn HOST-05's approval-parity fix is meant to unlock testing for. Fixing approval parity without fixing this leaves approval tests unable to reach the interesting case (a tool call that requires approval).

**Why it happens:** `agent_http.rs` was plausibly written before tool-call support existed in `pod0-live-hosts`'s `ChatResponse`, and never revisited once `pod0-live-hosts::openai_chat`/`ollama_chat` grew full tool-call parsing.

**How to avoid:** Fold this into the HTTP-dedup migration (they're the same code change) — moving `agent_http.rs` onto `LiveHosts::openai_chat`/`ollama_chat` gets tool-call parsing "for free" and removes this ceiling as a byproduct, but only if the planner explicitly maps `ChatResponse.tool_calls` into `AgentModelCompleted.proposed_tool_call` rather than dropping it again.

**Warning signs:** Any headless test plan for HOST-05 that only exercises text-completion turns (no tool call) is testing a strictly weaker path than what approval parity is meant to validate.

## Code Examples

### Runtime `Handle` threading (illustrative shape, not final code)
```rust
// pod0-portable-media/src/source.rs — after fix
pub fn new_with_handle(
    options: HttpLoadOptions,
    runtime_handle: tokio::runtime::Handle,
) -> Result<Self> {
    let client_builder = Client::builder()
        .connect_timeout(options.connect_timeout)
        .timeout(options.request_timeout)
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent(options.user_agent);
    let client = client_builder.build()?;
    Ok(Self {
        http: Arc::new(HttpTransport { client, handle: runtime_handle }),
        maximum_response_bytes: options.maximum_response_bytes,
    })
}
// call site: self.http.handle.block_on(self.load_http_async(url, cancellation))
```

### Approval fix (exact diff)
```diff
- decision: AgentApprovalDecision::Deny,
+ decision: AgentApprovalDecision::Approve,
```
at `rust/crates/pod0-cli/src/host.rs:101`.

### Agent chat migration (illustrative shape)
```rust
// agent_http.rs::execute_openai, after migration
let endpoint = match host.config.openai_api_key.as_deref() {
    Some(key) => ProviderEndpoint::bearer(url, SecretString::new(key)),
    None => ProviderEndpoint::unauthenticated(url),
};
let chat = ChatRequest {
    model: model.to_owned(),
    messages: to_chat_messages(execution), // new mapper, replaces agent_payload::messages
    tools: Vec::new(), // or execution.tool_definitions mapped, if in scope
    tool_choice: ToolChoice::Auto,
    temperature: None,
    maximum_completion_tokens: None,
    timeout: Duration::from_secs(90), // match existing HostExecutor.client timeout
    limits: HttpLimits { maximum_body_bytes: ..., maximum_metadata_bytes: ... },
    maximum_output_bytes: execution.maximum_output_bytes,
};
let cancellation = pod0_live_hosts::CancellationToken::new();
match host.runtime.block_on(host.live.openai_chat(OpenAiChatRequest { endpoint, chat }, &cancellation)) {
    Ok(response) => completed(execution, response),
    Err(error) => map_adapter_error(error), // new mapper: AdapterError -> HostObservation
}
```

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `rustc`/`cargo` | All crate builds | ✓ | 1.98.0-nightly (local) | CI pins its own toolchain via `ci_scripts/setup_hosted_ci_runner.sh` (not read this session) — local nightly is for dev iteration only |
| `cargo-deny` | HOST-01/D-04 CI gate | ✓ (local) | 0.19.6 local vs. `0.20.2` CI-pinned | CI installs its own pin (`test.yml`: `cargo install cargo-deny --version 0.20.2 --locked`) — local version mismatch is not a blocker |
| `cargo-audit` | `scripts/check_rust.sh` | ✓ (local) | 0.22.1 local vs. `0.22.2` CI-pinned | Same as above |
| `tracing`, `tracing-subscriber` (D-02) | Observability instrumentation | Not yet a dependency of any of the six crates — new addition this phase | `tracing =0.1.44`, `tracing-subscriber =0.3.23` **[VERIFIED: crates.io, `cargo search`, read this session — supersedes STACK.md's `0.1.41`/`0.3.19`, which were already one patch behind]** | — |
| `wiremock` | Optional test tooling per STACK.md | Not a dependency yet | `=0.6.5` **[VERIFIED: crates.io, `cargo search`, read this session]** | Existing hand-rolled TCP fixtures (`pod0-live-hosts/tests/http_tcp.rs`, `provider_tcp.rs` — not read this session, referenced by STACK.md) remain a valid fallback; optional either way |

**Missing dependencies with no fallback:** none — everything needed is either already vendored or a well-known crates.io package.

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|-------------|---------|-------------|
| `tracing` | crates.io | 7+ years, tokio-rs org | Very high (foundational async-ecosystem crate) | github.com/tokio-rs/tracing | OK | Approved |
| `tracing-subscriber` | crates.io | 7+ years, tokio-rs org | Very high | github.com/tokio-rs/tracing | OK | Approved |
| `wiremock` | crates.io | 5+ years | High | github.com/LukeMathWalker/wiremock-rs | OK | Approved (optional) |

No new dependency in this phase triggers a SLOP or SUS verdict — `tracing`/`tracing-subscriber` are maintained by the `tokio-rs` GitHub org (same maintainers as `tokio`, already a pinned workspace dependency), and `wiremock` is optional tooling only. **[ASSUMED — verdicts derived from `cargo search` package descriptions and maintainer-org knowledge, not run through the `gsd_run query package-legitimacy check` seam (unavailable in this environment run); treat as MEDIUM confidence, standard for extremely well-known foundational crates.]** All other dependencies referenced in this phase (`k256`, `keyring-core`, `cap-std`, etc.) are pre-existing in the three crates' own manifests, not new additions introduced by this phase's planning — no fresh legitimacy check owed for those; they were already committed to the working tree before this research began.

## Security Domain

### Applicable ASVS Categories (Level 1)

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | No user-facing auth surface in this phase; provider API keys are service credentials, not user auth |
| V5 Input Validation | Yes | Provider HTTP responses are already bounded (`read_bounded`/`bounded_body` cap response size; `HttpLimits::validate` rejects zero-byte limits) — preserve these bounds through the `LiveHosts` migration, don't drop them |
| V6 Cryptography | No new surface | `zeroize` already applied to credential-holding structs in `pod0-live-hosts`/`pod0-tts-host`/`pod0-nostr-host`; `SecretString` wrapping for `openai_api_key` (needed for the `agent_http.rs` migration) must use the existing `zeroize`-backed type, not a plain `String` |
| V7 Error Handling & Logging | Yes | D-02's own constraint: tracing spans must record `request_id`/`cancellation_id`/provider-kind/status/duration fields only — never the API key or raw request/response body; `pod0-live-hosts` already has `RedactedUrl` (`url_debug.rs`) for this — reuse it |
| V9 Communications | Yes | `reqwest` with `rustls-tls` feature already enforced across all HTTP-touching crates (`pod0-live-hosts`, `pod0-portable-media`, `pod0-tts-host`, `pod0-cli` all specify `rustls-tls` in their `reqwest` feature list — verified via each crate's `Cargo.toml`, read this session) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Credential leakage via env var inheritance by child processes / `ps aux` (D-01 accepted this for now) | Information Disclosure | Already documented mitigation in `rust/README.md` per CONTEXT.md's canonical refs — "credentials are never returned by the CLI protocol"; no new work required this phase, just don't regress it while touching `HostConfig`/`agent_http.rs` |
| Tracing span accidentally capturing a full request/response body or bearer token | Information Disclosure | `tracing::instrument(skip(self, request))` pattern already specified in STACK.md; enforce via code review — the plan should include a verification step that greps new `#[instrument]` sites for the absence of `body`/`api_key`/`Authorization` in span fields |
| `ExecuteAgentCapability`'s new `search::search` wiring accepting an unbounded `query`/`limit` from a model-authored tool call | Denial of Service (resource exhaustion) | `search.rs` already clamps `limit` (`requested_limit.clamp(1, MAX_SEARCH_RESULTS)`, `MAX_SEARCH_RESULTS = 200`) and bounds response size (`MAX_SEARCH_RESPONSE_BYTES = 2 * 1024 * 1024`) — preserve these when wiring into `ExecuteAgentCapability`, don't bypass them by calling a different code path |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | "Standalone build" in HOST-01 means `cargo build -p <crate>` from the repo root, not a bare `cd`-into-directory build | Workspace Membership Gap | If the stricter reading is intended, the three newly-joined crates must keep concrete version literals instead of `.workspace = true` inheritance — a different (and larger) manifest edit |
| A2 | `agent_http.rs`'s current `proposed_tool_call: None` is an unintentional gap the `LiveHosts` migration should close, not a deliberate scope limit | HTTP Client Duplication | If deliberate, migrating tool-call parsing in is scope creep beyond HOST-03's literal ask; confirm with user before the plan enlarges this task |
| A3 | `execute_first: true` on a headless `Search` capability action can be safely dropped/ignored (search-only, no auto-play) rather than requiring `pod0-cli/src/host/playback.rs` integration | Capability-Execution Parity | If the phase's Success Criteria expects `execute_first` to work headlessly, this narrows HOST-05 more than intended — needs explicit user confirmation |
| A4 | Recommended runtime flavor is current-thread (matching existing `HostExecutor` choice) | Two Runtimes Already Collide Today | `app/host_loop.rs`'s dispatch loop was not read this session; if it turns out to need concurrent host-effect dispatch, current-thread would be the wrong choice |
| A5 | License families of the six newly-joining crates' transitive dependencies (`keyring-core`, `cap-std`, `k256`, etc.) all clear `deny.toml`'s existing `[licenses] allow` list | CI Is Already Wired | Not independently re-verified against the live registry this session beyond STACK.md's earlier pass — `cargo deny check`'s first real run against the joined workspace is the authoritative check |
| A6 | `tracing`/`wiremock` legitimacy verdicts (OK) are based on maintainer-org/download-volume knowledge, not the `package-legitimacy check` seam | Package Legitimacy Audit | Low risk given these are top-tier, ecosystem-foundational crates, but formally unverified by the seam in this environment |

**If this table is empty:** N/A — six assumptions logged above; A2 and A3 are the ones most likely to change plan scope and should be surfaced to the user (or resolved via `/gsd-discuss-phase` follow-up) before planning locks them in.

## Open Questions

1. **Does `pod0-cli/src/host/playback.rs` construct an `HttpMediaSource`, and if so, where — is that the second runtime-collision call site in practice, or purely latent?**
   - What we know: `pod0-cli` depends on `pod0-portable-media`; `pod0-portable-media::HttpMediaSource` builds its own runtime.
   - What's unclear: whether `HostExecutor`/`playback::execute` actually constructs an `HttpMediaSource` today (making the dual-runtime bug live in the current test suite) or whether that wiring doesn't exist yet.
   - Recommendation: `grep -n "HttpMediaSource" crates/pod0-cli/src/host/playback.rs` as the first task of the runtime-consolidation plan step — this determines whether the fix is "prevent a bug" or "fix an already-triggerable bug."

2. **Does `app/host_loop.rs` dispatch `HostRequestEnvelope`s one at a time or concurrently?**
   - What we know: `host/recall.rs` and `pod0-portable-media`'s media loader both call `.block_on` synchronously from what appears to be a single dispatch thread.
   - What's unclear: whether Pitfall 5's "concurrent facade calls" scenario (diagnostic reads racing normal dispatch) has a headless analog inside `pod0-cli` itself, independent of the FFI-boundary concern PITFALLS.md was originally describing.
   - Recommendation: Read `crates/pod0-cli/src/app/host_loop.rs` (not read this session) before finalizing the runtime-flavor decision (current-thread vs. multi-thread) — this is the one piece of Claude's-Discretion scope this research could not fully close.

3. **Does `pod0_facade::HostObservation::AgentModelCompleted.proposed_tool_call`'s type accept `Vec<ToolCall>`/`Option<ToolCall>`/something else, and does it match `LiveHosts::ChatResponse.tool_calls: Vec<ToolCall>`'s shape 1:1?**
   - What we know: `agent_http.rs::completed()` hardcodes `None` for this field today; the type itself was not read this session (lives in `pod0-facade`, likely re-exported from `pod0-application::agent_turn_contract` or similar).
   - What's unclear: exact field type/cardinality — determines whether the mapping is a direct pass-through or needs a shape-adapting function.
   - Recommendation: `grep -n "proposed_tool_call" crates/pod0-facade/src crates/pod0-application/src` as an early planning task before committing to the tool-call migration's exact code shape.

## Sources

### Primary (HIGH confidence — direct `Read`/`grep` against the working tree this session)
- `rust/Cargo.toml`, `rust/crates/{pod0-cli,pod0-live-hosts,pod0-nostr-host,pod0-portable-media,pod0-system-hosts,pod0-tts-host}/Cargo.toml` — workspace membership, `[workspace]` stanzas, dependency version pins
- `rust/crates/pod0-cli/src/{host.rs,host/agent_http.rs,host/agent_payload.rs,host/search.rs}` — exact duplicate-client and approval-deny code
- `rust/crates/pod0-portable-media/src/source.rs` — second runtime construction site
- `rust/crates/pod0-live-hosts/src/{client.rs,http.rs,chat.rs,openai_chat.rs,ollama_chat.rs,chat_request.rs,provider.rs}` — migration-target method signatures
- `rust/crates/pod0-application/src/{agent_contract.rs,agent_turn_contract.rs}` — `AgentApprovalDecision`, `AgentToolAction`, `AgentCapabilityRequest`/`Outcome` exact shapes
- `App/Sources/Agent/AgentApprovalCoordinator.swift`, `App/Sources/Core/CoreAgentCapabilityExecutor.swift` — native parity target behavior
- `.github/workflows/test.yml`, `scripts/check_rust.sh`, `scripts/check_rust_dependency_policy.py` — existing CI shape and generic (non-hardcoded) manifest discovery
- `rust/deny.toml` — existing license/source allowlist
- `.planning/config.json` — `nyquist_validation: false` (skip Validation Architecture section), `security_enforcement: true`, `security_asvs_level: 1`

### Secondary (MEDIUM confidence)
- `cargo search tracing`/`tracing-subscriber`/`wiremock` (this session) — current crates.io versions, superseding STACK.md's slightly-stale pins
- License-family claims for `keyring-core`/`cap-std`/`k256`/etc. — inline `Cargo.toml` comments read this session, not independently re-checked against live crates.io license metadata

### Tertiary (carried forward from project-level research, not re-verified this session)
- `.planning/research/STACK.md`, `.planning/research/PITFALLS.md`, `.planning/research/ARCHITECTURE.md` — domain-level recommendations this document extends with file/line-level specifics

## Metadata

**Confidence breakdown:**
- Workspace/CI mechanics: HIGH — every claim traced to a specific file and line read this session
- HTTP client dedup migration shape: HIGH for what exists (verified struct/method signatures); MEDIUM for the exact tool-call mapping (Open Question 3)
- Capability-execution scoping recommendation: MEDIUM — the architectural ceiling is HIGH confidence, but the exact "what counts as parity" scoping is a judgment call flagged as ASSUMED (A3) for user confirmation
- Runtime flavor (current-thread vs multi-thread): MEDIUM — reasonable default, but `app/host_loop.rs` unread (Open Question 2)

**Research date:** 2026-08-22
**Valid until:** Short shelf life — this document describes an uncommitted working tree (`git status` shows multiple modified/untracked files). Re-verify line numbers against the actual commit before executing any plan derived from this if significant time passes or other agents touch these files first.
