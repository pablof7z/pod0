# Pitfalls Research

**Domain:** Voice UI cutover to a shared Rust-owned durable conversation authority, plus new async HTTP-provider Rust crates entering an existing UniFFI-exposed workspace
**Researched:** 2026-08-22
**Confidence:** MEDIUM (general patterns cross-checked across multiple independent sources: reqwest/tokio docs and discussions, AWS backoff research, voice-agent engineering writeups; project-specific severity assessments are HIGH — grounded directly in `.planning/PROJECT.md` and `.planning/codebase/CONCERNS.md`)

## Critical Pitfalls

### Pitfall 1: Second conversation authority emerges silently through the cancellation path, not the happy path

**What goes wrong:**
Teams cut voice over to the shared `SharedAgentConversationSession` for the *turn-taking* happy path (send audio transcript in, get response out) but leave a separate, voice-only code path for cancellation, barge-in, or error recovery — because the voice UI already had its own interruption handling (`StubVoiceTurnDelegate` or AVAudioSession-driven logic) and it's easier to keep using it for "just the interruption case." The result: two things can believe they own "what happens next" in the conversation — text/Rust believes a turn is in flight or cancelled, voice believes something else. This is the textbook way "one writer per domain" gets violated without anyone writing a second store.

**Why it happens:**
Interruption/cancellation is disproportionately more complex than the request/response path (it has to unwind an in-flight LLM call, a pending tool call, and audio playback simultaneously), so it's tempting to special-case it in the layer that already has the audio session state (Swift) rather than routing the interrupt itself through Rust and waiting for Rust's acknowledgment. Stale barge-in responses and lost context after interruption are state failures that happen specifically when the agent trusts a side channel instead of tracking task state through one authoritative store.

**How to avoid:**
Barge-in must be a *command sent to* `SharedAgentConversationSession`, not a local Swift-side abort. The client (Swift/voice) should optimistically mute/stop local audio playback immediately for responsiveness, but must not consider the turn cancelled, and must not start a new turn, until Rust's cancellation acknowledgment comes back — mirroring the "server is the ultimate arbiter of state" pattern used in production real-time voice systems: client holds local mute pending server ack; if Rust rejects the barge-in (e.g., a non-cancellable side effect already committed), the client unmutes and lets the original turn continue.

**Warning signs:**
- Any Swift-side voice code that transitions turn/session state without a corresponding Rust command+response.
- `StubVoiceTurnDelegate` (or any successor) still reachable in a release build, even behind a flag.
- Two different "is a turn in flight" booleans — one derived from Rust projection, one tracked locally in the voice UI layer.
- Barge-in tests only exercise the "clean cancel" case, never "barge-in arrives after Rust has already committed a side effect (tool call, paid request)."

**Phase to address:**
The phase that implements the barge-in/cancellation contract itself (not deferred to polish) — this is the highest-risk part of #142 per the project's own risk framing ("duplicate turns, uncancelled paid requests... or a second conversation authority").

---

### Pitfall 2: Cancellation latency budget is treated as a UX nicety instead of a correctness constraint

**What goes wrong:**
The cancel path is built to *eventually* stop everything, but not fast enough to prevent a race: user barges in, but the in-flight LLM call/tool call already committed a paid request or a durable side effect before the cancellation reaches it, producing an "uncancelled paid request" that shows up in the transcript or the bill after the user thought they interrupted it.

**Why it happens:**
Text-based cancellation has generous latency tolerance (nobody notices 300ms). Voice does not — barge-in has to detect interruption, stop audio, and cancel the in-flight call across process/thread boundaries within roughly a hundred milliseconds total for the interaction to feel responsive, and each hop (VAD → Swift → FFI → Rust → provider HTTP client) eats part of that budget. If the design only proves cancellation is *eventually consistent*, it will pass casual testing but fail under real barge-in timing, especially against a provider whose cancellation is itself unreliable (most LLM providers do not guarantee mid-stream compute actually stops server-side, even when the client drops the connection).

**How to avoid:**
Treat "committed vs. cancellable" as an explicit state machine inside the Rust session, not an implicit race. Any provider-facing call that becomes non-cancellable past a certain point (e.g., after the provider has accepted a tool-call commitment) must be marked so, and the UI must be told "this turn cannot be stopped, only its output can be discarded" rather than silently double-committing. Add cancellation-latency tests that inject barge-in at multiple points in the turn lifecycle (before dispatch, mid-stream, after tool-call commit) and assert observed behavior, not just eventual quiescence.

**Warning signs:**
- No explicit state distinguishing "cancellable" from "already committed" in the turn/effect model.
- Cancellation tests only cover the "barge-in before dispatch" case.
- Provider client cancellation relies solely on dropping the HTTP connection with no server-side cancel call and no idempotency key protecting against a resumed/duplicate charge.

**Phase to address:**
Same phase as Pitfall 1 (the barge-in/cancellation implementation); the latency-budget tests belong in that phase's verification, not deferred to hardware/UAT.

---

### Pitfall 3: Provider HTTP clients built without pooling, timeouts, or bounded retries turn a transient network blip into a stuck conversation

**What goes wrong:**
`pod0-live-hosts` (already flagged in `CONCERNS.md`) adds `reqwest`/`tokio` for OpenAI/Ollama chat, embeddings, and transcription with error types defined but no visible retry/backoff, connection-pool configuration, or timeout policy in the diff. Left as-is, a slow or failing provider doesn't just fail one request — because `reqwest::Client` instances each own an independent connection pool, creating a new client per request (a very easy mistake to make inside a per-call host-effect handler) silently defeats connection reuse and can exhaust local/remote connection limits under load; combined with no request timeout, a hung provider call can block a worker indefinitely rather than surfacing a bounded failure.

**Why it happens:**
`reqwest::Client` "just works" per call, so it's easy to construct one inline at the call site without noticing it should be a long-lived singleton (it already wraps an internal `Arc`, so sharing it costs nothing). Timeouts and retry policy are also easy to omit because the happy-path demo never exercises a slow or down provider.

**How to avoid:**
Construct exactly one `reqwest::Client` per host process (or per provider), configured with an explicit request timeout and `pool_idle_timeout`/`pool_max_idle_per_host`, and share it via the existing host-effect dispatch structure. Wrap provider calls in bounded retry with exponential backoff **and jitter** (jitter specifically prevents synchronized retry storms — cited AWS research shows 60-80% retry-storm reduction from adding jitter over backoff alone) and a hard retry ceiling, with `ProviderError`/`NetworkError` distinguishing retryable (timeout, 429, 5xx) from non-retryable failures.

**Warning signs:**
- `reqwest::Client::new()` (or `ClientBuilder::new().build()`) called inside a per-request function rather than constructed once and passed/stored.
- No `.timeout(...)` set anywhere on the client or per-request builder.
- Retry logic, if present, has no jitter and no attempt ceiling.
- `chat_tests.rs` / `embeddings_tests.rs` (currently 124 and 94 lines per `CONCERNS.md`) don't include a timeout-exceeded or connection-refused test case.

**Phase to address:**
The headless host-crate integration phase (`pod0-live-hosts` commit + CI), before it's exercised by the voice cutover — voice will be the first caller with a hard latency budget, so this needs to be solid before #142 depends on it.

---

### Pitfall 4: Embedding a tokio runtime inside a UniFFI-exposed, historically-synchronous Rust core creates blocking-runtime deadlocks that only appear under FFI concurrency

**What goes wrong:**
`pod0-facade`/`pod0-storage` predate `tokio`; this is the first time an async runtime enters a core that Swift calls into synchronously across an FFI boundary. Two known failure classes recur in this situation: (1) blocking work (sync SQLite calls, `unwrap`-heavy recovery code) executed directly inside async tasks starves the runtime's worker threads, so latency spikes under load instead of failing fast; (2) if more than one part of the dependency graph ends up initializing its own `tokio::runtime::Runtime` (easy to do across several new crates — `pod0-cli`, `pod0-live-hosts`, `pod0-nostr-host`, `pod0-tts-host` — each independently deciding to own a runtime), you get multiple runtime instances with separate globals that don't coordinate, and calling async code from within another runtime's blocking context panics ("Cannot drop a runtime in a context where blocking is not allowed" / "cannot start a runtime from within a runtime").

**Why it happens:**
Each new host crate was very plausibly built and tested in isolation (its own `#[tokio::main]` or its own `Runtime::new()`), which works fine standalone but breaks once they're all linked into one process behind the same `Pod0Facade`. The FFI boundary makes this worse: Swift calls are synchronous, so somewhere a `block_on` has to bridge sync-to-async, and if that bridging happens more than once at different layers, it's the classic "async/blocking/async sandwich" that produces intermittent panics only under concurrent FFI calls — not under single-threaded manual testing.

**How to avoid:**
Exactly one `tokio::runtime::Runtime` should exist for the whole linked process (constructed once, likely owned by `pod0-facade` or a shared host-runtime crate, not by each of the six new crates independently). All FFI-facing sync entry points bridge into it via a single, shared `block_on` (or a runtime handle passed down), never a fresh runtime per call. Any blocking work inside async tasks (sync SQLite via `pod0-storage`, filesystem I/O) goes through `tokio::task::spawn_blocking`, not inline.

**Warning signs:**
- `grep -rn "tokio::runtime::Runtime::new\|#\[tokio::main\]\|Runtime::new()"` across the six new crates returns more than one call site.
- Any new crate has its own `Cargo.toml` `tokio` feature set that differs from the workspace's (e.g., one crate pulls `rt-multi-thread`, another `rt`), which is a strong signal they're not sharing a runtime.
- Intermittent (not deterministic) panics or hangs that only reproduce under concurrent facade calls from Swift, never in single-threaded Rust unit tests.

**Phase to address:**
The headless host-crate commit/CI-integration phase — this needs a workspace-wide audit before merge, since it's invisible in any single crate's own test suite.

---

### Pitfall 5: New crates get committed and pass `cargo build`/`cargo test` per-crate but were never exercised together, or through CI, or through the facade boundary they're meant to serve

**What goes wrong:**
Six substantial (1,400+ line, per `CONCERNS.md`) crates are currently untracked and not in workspace CI. The natural failure mode when finally committing them is: each compiles and its own unit tests pass, so it looks done — but nobody has run `cargo build --workspace --all-targets`, `cargo test --workspace`, and `cargo clippy --workspace` together, and nobody has exercised the new `Pod0Facade` headless methods (`pending_host_effects`, `next_host_effect_at`, `library_page_with_totals`) concurrently with normal dispatch, which is exactly the concurrency scenario `CONCERNS.md` already flags as untested and potentially bypassing projection isolation.

**Why it happens:**
Standard cargo CI habits (`cargo test`) silently only run the default/library target per crate unless `--all-targets --workspace` is used explicitly; integration tests and cross-crate interaction are easy to miss because each crate "passes" independently. The new headless diagnostic methods were plausibly added and manually verified via one call at a time, never under the concurrent load a real headless host would produce.

**How to avoid:**
Before merging: run `cargo build --workspace --all-targets`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo audit` (new deps: `reqwest`, `tokio`, and transitive tree) as one CI job, not per-crate. Add an explicit test that calls `pending_host_effects`/`next_host_effect_at` concurrently with a normal `dispatch()` call and asserts no partial/torn state is observed. Gate the six new crates in CI from the same commit that adds them — don't commit first and wire CI later.

**Warning signs:**
- CI config that runs `cargo test` without `--workspace` or `--all-targets`.
- The new crates present in `Cargo.toml` workspace `members` but absent from any CI job matrix.
- No test exercises two facade calls (one normal dispatch, one diagnostic read) concurrently.

**Phase to address:**
The headless host-crate commit/CI-integration phase — explicitly called out as its own Active requirement in `.planning/PROJECT.md`, so it should have its own verification gate rather than being folded silently into #142.

---

### Pitfall 6: Bootstrap and effect-outbox signature changes ship as "just refactors" without a recovery story, then fail only in the field

**What goes wrong:**
`create_authoritative_store()` / `establish_empty_authority()` has no visible atomic-completion marker; if `establish_empty_authority()` fails partway (disk full, permissions) after migrations have started, the store is left partially initialized with no documented recovery path — this only bites on fresh installs, which is precisely when it's most damaging (first-run failure). Separately, `claim_next_with_identity()`'s new `defer_core_wakes: bool` parameter changes effect-outbox query semantics for all four public callers; if the boolean's meaning is inverted or defaulted wrong at even one call site, effects silently claim in the wrong mode rather than failing loudly.

**Why it happens:**
Both changes are internal refactors made in service of the headless/voice work, so they get reviewed as "plumbing" rather than as behavior changes with failure modes of their own. Partial-failure recovery is exactly the kind of code that's hard to write a test for without deliberately injecting a failure mid-sequence, so it's the first thing skipped under time pressure.

**How to avoid:**
Add a bootstrap-completion marker (or transactional wrapper) so a partially-initialized store is detectable and either resumable or safely deletable-and-retriable, and add a test that injects failure mid-`establish_empty_authority()`. For `defer_core_wakes`, add a test per caller that asserts the specific query behavior difference (which effects are/aren't claimed) rather than just "compiles and returns Ok."

**Warning signs:**
- No test that kills/fails the process mid-bootstrap and then retries `create_authoritative_store()` against the same path.
- `defer_core_wakes` call sites reviewed only for "does it compile," not "is the boolean correct for this caller's semantics."

**Phase to address:**
Headless host-crate integration phase (bootstrap is a direct dependency of standing up headless validation); effect-outbox semantics belong to whichever phase last touched `effect_outbox.rs` — verify before voice cutover depends on it for turn scheduling.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|-----------------|------------------|
| Keep a Swift-side fallback path for voice cancellation "just in case Rust is slow" | Avoids blocking on Rust cancellation latency work | Reintroduces the exact second-conversation-authority risk #142 exists to eliminate | Never in production; only as a dev-only, explicitly flagged debug path that never ships |
| Construct `reqwest::Client` per call for simplicity | Less state to thread through host-effect handlers | Defeats connection pooling, multiplies TLS handshake cost, risks connection exhaustion under voice's higher call frequency | Never — the fix (singleton client) is nearly free |
| Let each new host crate own its own tokio runtime during standalone development | Faster to get one crate compiling/testing in isolation | Multi-runtime FFI panics once crates are linked together in the real process | Acceptable only pre-integration, in each crate's own `#[cfg(test)]`; must be consolidated before workspace merge |
| Skip retry/backoff on provider calls for the first cut | Ships the happy path faster | Any transient provider hiccup becomes a stuck or duplicated voice turn — directly the "uncancelled paid requests" risk called out in scope | Never for the voice cutover; may be acceptable for an internal CLI-only debug tool with no user-facing latency budget |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|-----------------|-------------------|
| Voice UI → `SharedAgentConversationSession` | Routing the happy-path turn through Rust but keeping cancellation/interruption in Swift | Route barge-in as a command to Rust; client mutes optimistically but waits for Rust's ack before considering the turn cancelled |
| `pod0-live-hosts` → OpenAI/Ollama HTTP | New `reqwest::Client` per request; no timeout | One shared `Client` per process/provider with explicit `.timeout(...)` and pool settings |
| `pod0-live-hosts` retry logic → provider errors | Retrying without distinguishing retryable (429/5xx/timeout) from non-retryable (4xx auth/validation) errors, or retrying without jitter | Bound retries, classify via `ProviderError`, apply exponential backoff with jitter |
| New host crates → workspace CI | Committing crates before CI matrix includes them, or running `cargo test` without `--workspace --all-targets` | Add crates and CI job in the same change; use `--workspace --all-targets` |
| `Pod0Facade` headless diagnostics → normal dispatch | Calling `pending_host_effects`/`next_host_effect_at` with no concurrency test against live dispatch | Add a concurrent-call test before treating these methods as safe for headless consumers |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|-----------------|
| No connection pooling on provider HTTP client | Rising per-call latency under voice's higher request cadence vs. text | Singleton `reqwest::Client` with pool config | Noticeable once concurrent voice + text turns exceed a handful of simultaneous requests |
| No timeout on provider calls | A single slow/hung provider request blocks a worker/effect indefinitely | Explicit request timeout matched to the barge-in latency budget | First real provider outage or network degradation |
| Effect-outbox JSON extraction in SQL WHERE clauses (already flagged in `CONCERNS.md`) | Query planner can't use indexes; slows as effect volume grows | Denormalize wake-time into a queryable column | ~100k+ active effects, per existing scaling-limits note |
| Multiple tokio runtimes across linked crates | Intermittent hangs/panics only under concurrent FFI load, not in isolated crate tests | One shared runtime instance, audited via grep before merge | As soon as two of the six new crates are exercised concurrently in the same process |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Provider API keys read from env vars in `pod0-cli`/`pod0-live-hosts` (already flagged in `CONCERNS.md`) | Exposure via `ps aux`, log/error exfiltration, inheritance by child processes | Prefer keyring/secure storage; mask keys in any logged error path; scrub env before spawning children |
| Retry/error logging that includes raw request/response bodies from provider calls | API keys or user transcript content leaking into logs | Redact provider auth headers and truncate/redact free-text content in structured logs |
| No idempotency key on provider calls that get retried | A retried "cancelled" turn is billed twice, or produces a duplicate voice response after the user already got an answer | Attach a per-turn idempotency/request ID Rust-side so retries and cancellation races can't double-charge or double-emit |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-------------------|
| Barge-in muted locally but the original response keeps playing seconds later because Rust rejected/couldn't complete the cancel | User hears the "interrupted" answer resume mid-sentence, feels broken | Make the reject-cancel path an explicit, tested state (per Pitfall 2) with a clear resume behavior, not a leftover buffer flush |
| Voice turn silently fails after a provider timeout with no spoken feedback | User thinks the app is unresponsive, repeats the request, compounding duplicate-turn risk | Bound provider timeout to the barge-in latency budget and surface a spoken/haptic failure signal before the user re-tries |
| Siri/Shortcuts re-enabled before cold/warm invocation tests pass (explicitly gated in `.planning/PROJECT.md`) | First voice-agent interaction via Siri could hit an uninitialized or partially-warm session, producing a confusing dead turn | Keep the Siri/Shortcuts entry point behind its own gate until cold/warm tests are green, independent of in-app voice mode readiness |

## "Looks Done But Isn't" Checklist

- [ ] **Barge-in / cancellation:** Often missing the "Rust rejects the cancel" path — verify a test where a non-cancellable side effect is already committed when barge-in arrives.
- [ ] **Provider HTTP client:** Often missing pooling/timeout/retry config even when "it works in manual testing" — verify `Client` is constructed once and grep for `.timeout(`.
- [ ] **New host crates in CI:** Often "compiles for me" but never run as `cargo test --workspace --all-targets` — verify the CI job matrix explicitly includes each new crate.
- [ ] **Headless facade diagnostics:** Often manually spot-checked once, not under concurrency — verify a concurrent-call test exists against `pending_host_effects`/`next_host_effect_at`.
- [ ] **Bootstrap (`create_authoritative_store`):** Often only tested on the success path — verify a mid-failure/retry test exists for fresh-install scenarios.
- [ ] **Siri/Shortcuts voice routing:** Often re-enabled as soon as in-app voice mode works — verify cold/warm invocation tests are a separate, explicit gate per `.planning/PROJECT.md`.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|----------------|------------------|
| Second conversation authority discovered post-release (duplicate/lost turns in the field) | HIGH | Feature-flag voice mode off immediately, audit affected sessions via Rust's durable log to identify divergence, patch the specific side channel, re-enable behind a narrower rollout |
| Provider client exhausts connections / stuck calls under load | MEDIUM | Roll out singleton-client + timeout fix; in the interim, cap concurrent voice sessions or add a circuit breaker around the provider client |
| Multiple tokio runtimes causing intermittent FFI panics | MEDIUM | Audit and consolidate to one runtime instance (grep-driven, per Pitfall 4); requires a full workspace rebuild/retest, not a hotfix |
| Partial bootstrap failure on fresh install | LOW–MEDIUM | Detect via missing completion marker; instruct clean re-install (delete partial store, retry `create_authoritative_store`) since it only affects fresh installs |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|-------------------|----------------|
| Second conversation authority via cancellation side-channel | Voice barge-in/cancellation implementation phase (#142 core) | Test: barge-in only ever transitions turn state after a Rust ack; no Swift-local turn-state mutation exists |
| Cancellation latency budget / uncancelled paid requests | Voice barge-in/cancellation implementation phase (#142 core) | Test: barge-in injected at pre-dispatch, mid-stream, and post-commit points, each with asserted (not just eventual) outcome |
| Unpooled/untimed/unretried provider HTTP clients | Headless host-crate commit + CI phase (`pod0-live-hosts`) | Test: single shared `Client` instance asserted via construction-site grep or a runtime instance-count check; timeout-exceeded and 429 test cases exist |
| Multiple tokio runtimes across new crates | Headless host-crate commit + CI phase | Audit: grep for `Runtime::new`/`#[tokio::main]` across all six crates returns exactly one owner; concurrent-FFI-call test passes without panics |
| Untested new crates / untested headless facade methods | Headless host-crate commit + CI phase | CI: `cargo build/test/clippy --workspace --all-targets` green in one job; concurrency test for `pending_host_effects`/`next_host_effect_at` exists |
| Bootstrap partial-failure / effect-outbox signature semantics | Headless host-crate commit + CI phase (bootstrap); whichever phase owns `effect_outbox.rs` (signature) | Test: injected mid-bootstrap failure + retry; per-caller `defer_core_wakes` behavior test |
| Siri/Shortcuts routing re-enabled early | Voice cutover rollout phase (final gate before #142 closes) | Explicit cold/warm invocation test suite, gated separately from in-app voice mode readiness, per `.planning/PROJECT.md` Active requirements |

## Sources

- [Common Failure Modes in Voice Agents — Picovoice](https://picovoice.ai/guide/voice-agents/common-failure-modes/) (MEDIUM)
- [Voice AI Barge-In and Turn-Taking: A 2026 Implementation Guide — FutureAGI](https://futureagi.com/blog/voice-ai-barge-in-turn-taking-2026/) (MEDIUM)
- [Barge-in and turn-taking: how conversational voice systems handle interruption — The Voice Layer](https://thevoicelayer.com/posts/barge-in-and-turn-taking-how-conversational-voice-systems-handle-interruption) (MEDIUM)
- [Real-Time Voice Ordering Performance Constraints — Stable Kernel](https://stablekernel.com/blogs/blog-real-time-voice-ordering-performance-constraints) (MEDIUM)
- [Barge-in and interruption handling for on-device voice agents — EdgeAI](https://www.runedge.ai/blog/barge-in-interruption-handling-on-device-voice) (MEDIUM)
- [reqwest `Client` docs.rs](https://docs.rs/reqwest/latest/reqwest/struct.Client.html) (HIGH — official docs)
- [reqwest Tutorial: HTTP Client Best Practices in Rust](https://reintech.io/blog/reqwest-tutorial-http-client-best-practices-rust) (MEDIUM)
- [Does Reqwest support connection reuse across requests? — WebScraping.AI](https://webscraping.ai/faq/reqwest/does-reqwest-support-connection-reuse-across-requests) (MEDIUM)
- [Top 5 Tokio Runtime Mistakes That Quietly Kill Your Async Rust — Techbuddies Studio](https://www.techbuddies.io/2026/03/21/top-5-tokio-runtime-mistakes-that-quietly-kill-your-async-rust/) (MEDIUM)
- [Calling the Tokio runtime via CGO/FFI from multiple goroutines — tokio-rs/tokio Discussion #5840](https://github.com/tokio-rs/tokio/discussions/5840) (MEDIUM — maintainer discussion)
- [Help with async FFI library — tokio-rs/tokio Discussion #3534](https://github.com/tokio-rs/tokio/discussions/3534) (MEDIUM — maintainer discussion)
- [Async -> blocking -> async sandwich causes tokio panics? — users.rust-lang.org](https://users.rust-lang.org/t/async-blocking-async-sandwich-causes-tokio-panics/134538) (MEDIUM)
- [AI Agent Retry Patterns - Exponential Backoff Guide — Fast.io](https://fast.io/resources/ai-agent-retry-patterns/) (MEDIUM)
- [How to Implement Exponential Backoff with Jitter in Rust — OneUptime](https://oneuptime.com/blog/post/2026-01-25-exponential-backoff-jitter-rust/view) (MEDIUM)
- [GitHub - ihrwein/backoff: Exponential backoff and retry for Rust](https://github.com/ihrwein/backoff) (MEDIUM — widely used crate)
- [The Cancellation Tax: Your Inference Bill After the User Hits Stop — TianPan.co](https://tianpan.co/blog/2026-04-23-cancellation-tax-streaming-abort-billing) (MEDIUM)
- [feat: Add request cancellation API for in-flight LLM generation — lemonade-sdk/lemonade Issue #2590](https://github.com/lemonade-sdk/lemonade/issues/2590) (MEDIUM)
- [What is idempotency in Redis? Cost-saving patterns for LLM apps — Redis](https://redis.io/blog/what-is-idempotency-in-redis/) (MEDIUM)
- [Continuous Integration — The Cargo Book](https://doc.rust-lang.org/cargo/guide/continuous-integration.html) (HIGH — official docs)
- `.planning/PROJECT.md` and `.planning/codebase/CONCERNS.md` (HIGH — direct repo inspection, project-specific severity and file references)

---
*Pitfalls research for: Voice-to-Rust agent conversation cutover (#142) and headless Rust host crate integration*
*Researched: 2026-08-22*
