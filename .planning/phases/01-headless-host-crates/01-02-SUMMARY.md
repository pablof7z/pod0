---
phase: 01-headless-host-crates
plan: 02
subsystem: infra
tags: [tokio, reqwest, tracing, pod0-cli, pod0-live-hosts, pod0-portable-media, pod0-tts-host]

requires:
  - phase: 01-headless-host-crates
    provides: "13-member rust/Cargo.toml workspace (all six host crates joined, green build/test/clippy/deny/audit)"
provides:
  - "Exactly one tokio::runtime::Runtime constructed in the pod0-cli process; pod0-portable-media's MediaLoader is driven by a shared Handle when constructed from pod0-cli, via a multi-thread runtime (not current-thread) so the cross-thread Handle::block_on used by the pod0-host-pump worker thread actually makes progress"
  - "HostExecutor holds no reqwest::blocking::Client; fetch_feed/fetch_library/agent_http/search all route through the one pooled, timeout-bound LiveHosts client"
  - "A provider response's tool call is parsed into AgentModelCompleted.proposed_tool_call instead of being unconditionally dropped, unblocking headless tool-call-turn testing"
  - "pod0-live-hosts (8 public async methods) and pod0-tts-host (TtsClient::generate) emit tracing spans (status/duration_ms/outcome, never body or credentials) on every outbound HTTP call; pod0-cli installs the one process-wide tracing_subscriber"
affects: [01-03]

actuals:
  tokens: 17479
  tasks: 3
  commits: 3

tech-stack:
  added: ["tracing =0.1.44", "tracing-subscriber =0.3.23"]
  patterns:
    - "Single shared multi-thread tokio::runtime::Runtime (worker_threads(1)) at the pod0-cli process boundary, threaded down as a Handle to consumers on other threads — not current-thread, because a current-thread runtime's I/O/timer driver only makes progress on the thread that owns it; Handle::block_on from any other thread hangs forever (reproduced independently, confirmed by a fixed minimal repro before and after switching flavors)"
    - "A caller-owned Runtime is still driven via Runtime::block_on directly (not through a cloned Handle), even where a Handle field also exists on the same struct, when that runtime's own thread-affinity requirement (current-thread, driven cross-thread) must be preserved for existing standalone callers"
    - "pod0-live-hosts's tracing_support module centralizes span-outcome recording (status/duration_ms/outcome) and error-kind labeling for every public async client method, so # of instrumented call sites never duplicates the AdapterError match arms"

key-files:
  created:
    - rust/crates/pod0-live-hosts/src/tracing_support.rs
  modified:
    - rust/crates/pod0-portable-media/src/source.rs
    - rust/crates/pod0-cli/src/host.rs
    - rust/crates/pod0-cli/src/host/playback.rs
    - rust/crates/pod0-cli/src/host/agent_http.rs
    - rust/crates/pod0-cli/src/host/agent_payload.rs
    - rust/crates/pod0-cli/src/host/search.rs
    - rust/crates/pod0-cli/src/main.rs
    - rust/crates/pod0-cli/Cargo.toml
    - rust/crates/pod0-cli/tests/live_agent.rs
    - rust/Cargo.toml
    - rust/crates/pod0-live-hosts/Cargo.toml
    - rust/crates/pod0-live-hosts/src/{http,openai_chat,ollama_chat,embeddings,rerank,transcription,download,lib}.rs
    - rust/crates/pod0-tts-host/Cargo.toml
    - rust/crates/pod0-tts-host/src/client.rs

key-decisions:
  - "HostExecutor's shared runtime switched from current-thread to multi-thread (worker_threads(1)) — a plan deviation, not optional: current-thread's Handle::block_on hangs forever when called from a different thread than the one that built the Runtime, which is exactly the real pod0-cli shape (HostExecutor::new runs on the Shell-constructing thread; every HostExecutor::execute — including the new playback path — runs on the separate pod0-host-pump worker thread). Verified independently with a minimal reproduction before committing to the fix."
  - "pod0-portable-media's MediaLoader::new (owned-Runtime path) still drives block_on via Runtime::block_on directly, not the newly-added cloned Handle field, to preserve its own pre-existing cross-thread-safe behavior for external/standalone callers and its own test suite — only the new new_with_handle (caller-owned) path uses Handle::block_on, which is safe because pod0-cli's runtime is now multi-thread."
  - "Tracing span fields are limited to status/duration_ms/outcome with the span name itself (derived from the fn name) conveying the call kind — no explicit 'provider' field and no URL field at all, which sidesteps needing RedactedUrl entirely rather than adding a URL field and then redacting it."

requirements-completed: [HOST-03, HOST-04]

coverage:
  - id: D1
    description: "Exactly one tokio Runtime is constructed in the pod0-cli process (HostExecutor::new); pod0-portable-media's MediaLoader is driven by that Runtime's Handle when used from pod0-cli's playback path, not a second Builder::new_current_thread()"
    requirement: HOST-04
    verification:
      - kind: integration
        ref: "cd rust && cargo test -p pod0-cli -p pod0-portable-media --all-targets --locked"
        status: pass
    human_judgment: false
  - id: D2
    description: "HostExecutor holds no reqwest::blocking::Client; fetch_feed, fetch_library, agent_http's execute_openai/execute_ollama, and search::search all route through the one pooled LiveHosts client with explicit timeouts"
    requirement: HOST-03
    verification:
      - kind: integration
        ref: "grep -c 'reqwest::blocking' crates/pod0-cli/src/host.rs (returns 0)"
        status: pass
      - kind: integration
        ref: "cd rust && cargo test -p pod0-cli --all-targets --locked"
        status: pass
    human_judgment: false
  - id: D3
    description: "A provider response's tool call is parsed into AgentModelCompleted.proposed_tool_call instead of being dropped or unconditionally rejected by the CLI-level contains_tool_call guard"
    verification:
      - kind: integration
        ref: "cd rust && cargo test -p pod0-cli --test live_agent"
        status: pass
    human_judgment: false
  - id: D4
    description: "pod0-live-hosts's 8 public async LiveHosts methods and pod0-tts-host's TtsClient::generate emit tracing spans (status/duration_ms/outcome) with no body/bearer_token/api_key/Authorization ever recorded as a span field; pod0-cli installs the sole process-wide subscriber; pod0-facade gains no tracing dependency"
    verification:
      - kind: integration
        ref: "cd rust && cargo build --workspace --all-targets && grep -rn '#\\[tracing::instrument' crates/pod0-live-hosts/src crates/pod0-tts-host/src/client.rs"
        status: pass
      - kind: integration
        ref: "grep -rniE 'field\\(.*(body|bearer_token|api_key|Authorization)' crates/pod0-live-hosts/src crates/pod0-tts-host/src (only pre-existing ProviderError Debug fields body_bytes/body_truncated match, which store length/flag not content)"
        status: pass
    human_judgment: false

duration: 45min
completed: 2026-08-22
status: complete
---

# Phase 1 Plan 2: Runtime Consolidation, HTTP Client Dedup, Tracing Summary

**One shared multi-thread tokio runtime and one pooled LiveHosts HTTP client per pod0-cli process, tool-call proposals surfaced instead of dropped, and tracing spans on every outbound HTTP call in pod0-live-hosts/pod0-tts-host.**

## Performance

- **Duration:** ~45 min
- **Started:** 2026-08-22 (approx.)
- **Completed:** 2026-08-22T19:35:28+03:00
- **Tasks:** 3 completed
- **Files modified:** 22 (across the three task commits)

## Accomplishments
- Consolidated the two colliding tokio runtimes (`pod0-cli::HostExecutor::new` and `pod0-portable-media`'s `HttpMediaSource`/`MediaLoader`) into one shared `Handle`, threaded from `HostExecutor` through `playback::execute`/`HostPlayer::new` into a new `MediaLoader::new_with_handle` constructor — while discovering and fixing a real cross-thread deadlock this consolidation would otherwise have introduced (see Deviations).
- Deleted `HostExecutor`'s duplicate `reqwest::blocking::Client` entirely; `fetch_feed`, `fetch_library`, `agent_http::execute_openai`/`execute_ollama`, and `search::search` all now issue HTTP through the single pooled, timeout-bound `LiveHosts` client the process already held, preserving conditional-GET (ETag/Last-Modified/304) and response-size bounds.
- As a direct consequence of migrating `agent_http.rs` onto `LiveHosts::openai_chat`/`ollama_chat`, removed the CLI-level `contains_tool_call` early-rejection and wired `ChatResponse.tool_calls` into `AgentModelCompleted.proposed_tool_call` — a provider-proposed tool call is now surfaced to the durable core instead of being silently dropped, unblocking headless tool-call-turn testing ahead of Plan 3's approval-parity fix.
- Added `#[tracing::instrument]` to all 8 public async `LiveHosts` methods and `pod0-tts-host`'s `TtsClient::generate`, recording `status`/`duration_ms`/`outcome` via a shared `tracing_support` helper that never touches request/response bodies or credentials; installed the sole process-wide `tracing_subscriber` in `pod0-cli::main`.

## Task Commits

1. **Task 1: Consolidate the two colliding tokio runtimes into one shared Handle (HOST-04, D-03)** - `413d584c` (feat)
2. **Task 2: Delete the duplicate blocking HTTP client and route every call through LiveHosts (HOST-03)** - `5fc0d500` (feat)
3. **Task 3: Add tracing instrumentation to pod0-live-hosts and pod0-tts-host, install the subscriber in pod0-cli (D-02)** - `cbd30320` (feat)

## Files Created/Modified
- `rust/crates/pod0-portable-media/src/source.rs` - `MediaLoader::new_with_handle` (new); `HttpTransport` gains an `owned_runtime: Option<Runtime>` + always-present `handle: Handle`; `load_http` drives `Runtime::block_on` directly when it owns the runtime, `Handle::block_on` only for the caller-owned path
- `rust/crates/pod0-cli/src/host.rs` - `HostExecutor.client` field removed; shared runtime switched to `Builder::new_multi_thread().worker_threads(1)`; `fetch_feed`/`fetch_library` rewritten onto `LiveHosts::http_get`; `map_adapter_error` (new)
- `rust/crates/pod0-cli/src/host/playback.rs` - `execute`/`HostPlayer::new` take a `&tokio::runtime::Handle`/`Handle` parameter
- `rust/crates/pod0-cli/src/host/agent_http.rs` - rewritten onto `LiveHosts::openai_chat`/`ollama_chat`; `contains_tool_call` removed; `completed()` populates `proposed_tool_call`
- `rust/crates/pod0-cli/src/host/agent_payload.rs` - `to_chat_messages` (replaces `messages()`), producing `pod0_live_hosts::ChatMessage`
- `rust/crates/pod0-cli/src/host/search.rs` - rewritten onto `LiveHosts::http_get`
- `rust/crates/pod0-cli/src/main.rs` - installs `tracing_subscriber::fmt().with_target(true).init()`
- `rust/crates/pod0-cli/Cargo.toml` - `tokio` gains `rt-multi-thread`; `tracing-subscriber.workspace = true`; unused direct `reqwest` dependency removed
- `rust/crates/pod0-cli/tests/live_agent.rs` - ollama fixture gains `"done":true`; tool-call test assertions updated to real observed post-migration behavior
- `rust/Cargo.toml` - `[workspace.dependencies]` gains `tracing = "=0.1.44"`, `tracing-subscriber = { version = "=0.3.23", features = ["fmt"] }`
- `rust/crates/pod0-live-hosts/Cargo.toml`, `src/{http,openai_chat,ollama_chat,embeddings,rerank,transcription,download,lib}.rs` - `tracing.workspace = true`; `#[tracing::instrument]` on all 8 public async methods
- `rust/crates/pod0-live-hosts/src/tracing_support.rs` - new shared `record_outcome`/`error_kind` helpers
- `rust/crates/pod0-tts-host/Cargo.toml`, `src/client.rs` - `tracing = "=0.1.44"` (concrete literal); `TtsClient::generate` instrumented, real work moved to a new private `generate_inner`

## Decisions Made
See `key-decisions` in frontmatter — the current-thread-to-multi-thread runtime-flavor fix (load-bearing, not optional), the dual `Runtime::block_on`/`Handle::block_on` dispatch in `pod0-portable-media`, the concrete-literal `tracing` pin in `pod0-tts-host`, and the minimal tracing-field scheme (no URL field, no separate `provider` field).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Current-thread runtime's `Handle::block_on` hangs forever when called from a thread other than the one that created it**
- **Found during:** Task 1 verification (`cargo test -p pod0-cli -p pod0-portable-media`)
- **Issue:** The plan's illustrative code and RESEARCH.md's Open Question 2 resolution both assumed a `tokio::runtime::Handle` "works correctly under either single- or multi-threaded dispatch." This is false for a `current_thread`-flavored `Runtime`: its I/O/timer driver only makes progress on the thread that is actually driving it via `block_on`. `Handle::block_on()` called from a *different* thread than the one that built the `Runtime` enters the runtime context but the driver never runs — futures like `tokio::time::sleep` or a pending HTTP response simply never complete. This is exactly the real `pod0-cli` shape: `HostExecutor::new` (which builds the one shared `Runtime`) runs on the `Shell`-constructing thread, while every `HostExecutor::execute` call — including the newly-threaded `playback::execute` path — runs on the separate `pod0-host-pump` worker thread (`app/host_loop.rs`). I reproduced this independently with a minimal standalone repro (a `current_thread` runtime's `Handle` cloned and `block_on`'d from a spawned thread hangs indefinitely; the identical repro with `new_multi_thread()` instead completes in ~200ms) before treating it as a confirmed bug rather than a flaky test.
- **Fix:** Switched `HostExecutor::new`'s shared runtime from `Builder::new_current_thread()` to `Builder::new_multi_thread().worker_threads(1)` (added `rt-multi-thread` to `pod0-cli`'s own `tokio` feature list for correctness independent of feature-unification luck from other workspace crates). `pod0-portable-media`'s `MediaLoader::new` (still `current_thread`, used standalone/cross-thread by its own pre-existing test) now drives its owned `Runtime` via `Runtime::block_on` directly rather than through the newly-added `Handle` field, so its existing cross-thread-safe behavior is unaffected; only the new caller-owned `new_with_handle` path uses `Handle::block_on`, which is now safe because `pod0-cli`'s runtime is multi-thread.
- **Files modified:** `rust/crates/pod0-cli/src/host.rs`, `rust/crates/pod0-cli/Cargo.toml`, `rust/crates/pod0-portable-media/src/source.rs`
- **Verification:** `cd rust && cargo test -p pod0-cli -p pod0-portable-media --all-targets --locked` exits 0, including the pre-existing `cancellation_does_not_wait_for_a_stalled_blocking_dns_task` test that this bug caused to hang/timeout before the fix.
- **Committed in:** `413d584c` (Task 1 commit)

**2. [Rule 1 - Bug] `live_agent.rs`'s Ollama test fixture was missing `"done":true`**
- **Found during:** Task 2 verification (`cargo test -p pod0-cli --test live_agent`)
- **Issue:** The old hand-rolled JSON parser in `agent_http.rs` never checked Ollama's `done` field. `LiveHosts::ollama_chat`'s parser correctly requires `done == true` for a non-streaming response (matching Ollama's real API contract) and rejects the response otherwise. The test's fake server fixture never included this field, so migrating onto the correct parser caused a previously-passing test to fail — the test fixture was already technically incomplete, just never exercised against a strict parser.
- **Fix:** Added `"done":true` to the fixture body in `live_agent.rs`.
- **Files modified:** `rust/crates/pod0-cli/tests/live_agent.rs`
- **Committed in:** `5fc0d500` (Task 2 commit)

**3. [Rule 3 - Blocking issue] Now-unused direct `reqwest` dependency in `pod0-cli`**
- **Found during:** Task 2, after confirming no `reqwest` symbol remained referenced in `pod0-cli`'s own source
- **Issue:** With the blocking client deleted, `pod0-cli`'s direct `reqwest.workspace = true` dependency had no remaining call sites — leaving it in place would contradict the plan's own "one pooled client" objective by implying a second HTTP client dependency still existed at this crate's boundary.
- **Fix:** Removed `reqwest.workspace = true` from `pod0-cli/Cargo.toml` (still available transitively via `pod0-live-hosts`/`pod0-portable-media`); regenerated the small resulting `Cargo.lock` diff.
- **Files modified:** `rust/crates/pod0-cli/Cargo.toml`, `rust/Cargo.lock`
- **Committed in:** `5fc0d500` (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (1 Rule 1 production bug fix load-bearing for correctness, 1 Rule 1 test-fixture bug fix, 1 Rule 3 dependency cleanup)
**Impact on plan:** The runtime-flavor fix was necessary — without it, Task 1's own change would have introduced a silent, un-testable production deadlock the moment `pod0-cli`'s real host pump dispatched a playback request, since none of the existing unit tests exercise `HostExecutor` across its real two-thread boundary. No scope creep into unrelated product behavior.

## Issues Encountered


## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

Plan 3 (HOST-05 approval/capability parity) can now build on: a single shared multi-thread runtime with no cross-thread deadlock risk, one pooled HTTP client per process, and `AgentModelCompleted.proposed_tool_call` actually reachable in headless tests — exactly the precondition Plan 3's approval-flow testing needs. No blockers identified for Plan 3.

---
*Phase: 01-headless-host-crates*
*Completed: 2026-08-22*

## Self-Check: PASSED

All claimed created/modified files and all three task commit hashes (`413d584c`, `5fc0d500`, `cbd30320`) verified present via `git log`/file existence checks below.
