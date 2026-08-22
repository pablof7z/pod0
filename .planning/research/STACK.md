# Stack Research

**Domain:** Rust workspace with a UniFFI facade to Swift/iOS, plus headless host crates doing async HTTP calls to AI providers (chat, TTS, embeddings) via reqwest+tokio, callable both from the FFI boundary and a standalone CLI.
**Researched:** 2026-08-22
**Confidence:** HIGH (versions/patterns verified against crates.io and official UniFFI docs; recommendations grounded in the actual code already in this workspace, not generic advice)

## Existing Pattern Is Already Correct — Extend It, Don't Replace It

Before recommending anything new: the workspace already implements the standard 2025/2026 shape for this problem, and it should not be re-architected.

- **UniFFI facade stays fully synchronous.** `Pod0Facade` (`rust/crates/pod0-facade/src/runtime.rs`) exports no `async fn` at all. Durable work crosses the FFI boundary as typed, leased, pollable state (`pending_host_effects`, `next_leased_headless_host_requests`, `record_leased_host_observation`) backed by the SQLite effect outbox, not as `uniffi` async futures. **Keep this.** UniFFI's own docs confirm async trait/callback support exists but async Rust futures crossing into Swift do not yet conform to `Sendable` under Swift 6 — adopting `uniffi`'s async FFI now would import an unresolved concurrency-safety gap into a Swift 6 codebase. The lease/poll/outbox pattern pod0 already has sidesteps that gap entirely and is the right choice for a durable, cancellable, process-death-safe conversation session. Confidence: HIGH.
- **One `reqwest::Client` built once per host adapter, reused for all requests.** `pod0-live-hosts::LiveHosts::new`, `pod0-tts-host::client.rs`, and `pod0-portable-media::source.rs` each build a single `Client` via `Client::builder()` and store it on a long-lived struct — this is exactly the client-reuse pattern reqwest's own docs require (a `Client` holds a connection pool; constructing one per request defeats pooling and exhausts sockets). **CONCERNS.md's claim that "no visible connection pooling or client reuse for HTTP" exists is only partially right**: reuse exists in `pod0-live-hosts`, `pod0-tts-host`, and `pod0-portable-media`. The gap is real only in `pod0-cli`'s own `HostExecutor` (`crates/pod0-cli/src/host.rs`), which builds a *second*, separate `reqwest::blocking::Client` for agent chat completions instead of routing that traffic through the already-injected `pod0_live_hosts::LiveHosts` instance it also holds. That is duplication, not a missing-pool bug — fix it by deleting the blocking client and moving `agent_http.rs`'s OpenAI/Ollama calls onto `LiveHosts` (already `.await`-based and driven through the CLI's existing `current_thread` Tokio runtime in `HostExecutor::new`). Confidence: HIGH.
- **Retry timing belongs to the durable core, not the HTTP client.** `pod0-application::Retryability` (`Never` / `Automatic` / `AfterUserAction`) is already a first-class, UniFFI-exported classification, and the effect outbox (`pod0-storage/src/effect_outbox.rs`) already computes lease expiry and next-claim-at timing in SQLite. **Do not add `reqwest-middleware`/`reqwest-retry`-style client-side exponential backoff to the host adapters.** A middleware retry loop racing against the outbox's own lease/backoff schedule would double-retry, mask which layer is retrying, and violate "Rust owns durable product decisions" (`AGENTS.md`) by moving a durability decision into a stateless HTTP client. The correct fix for CONCERNS.md's "no retry logic" finding is narrower than it sounds: host adapters need to *classify* failures accurately (timeout vs. connect vs. 429/5xx vs. malformed body) and return them as `Retryability::Automatic` with `retry_after` when present — which `pod0-live-hosts::ProviderError` already models — and let the outbox schedule the retry. Confidence: HIGH.

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `reqwest` | `=0.12.28` (current pin) | Async HTTP client for provider adapters | Already the workspace standard; `0.13.x` is now current upstream but is a breaking major bump (MSRV 1.85+, TLS backend changes) — treat as a deliberate, separately-reviewed upgrade, not something to pull in during this milestone. Stay pinned at `0.12.28` for #142. |
| `tokio` | `=1.53.1` (current pin) | Async runtime for host adapters and the CLI's host pump | Already current upstream (1.53.x is the latest 1.x line as of this research). No action needed. |
| `uniffi` | `=0.32.0` (current pin) | Rust↔Swift/Kotlin FFI, synchronous facade only | Correct as pinned; do not adopt `uniffi`'s async future/stream support for this milestone (see rationale above). |
| `serde` / `serde_json` | `=1.0.228` / `=1.0.150` (current pins) | Provider JSON request/response modeling | Already standard and current; no change needed. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tracing` + `tracing-subscriber` | `tracing =0.1.41`, `tracing-subscriber =0.3.19` | Structured, leveled logging with request-scoped spans for provider HTTP calls | Add to `pod0-live-hosts` and `pod0-tts-host` to close CONCERNS.md's "no visible observability for external API calls" gap. Instrument each adapter call (`http_get`, `chat`, TTS synthesis) with a `tracing::instrument` span carrying a request/turn correlation ID (pod0 already generates `request_id`/`cancellation_id` — thread those into the span, never the API key or full body). `pod0-cli` installs a `tracing_subscriber::fmt` subscriber writing to stderr (keeps stdout clean for the newline-delimited JSON protocol). iOS does **not** need a Rust-side subscriber: the facade stays synchronous and provider HTTP happens only inside `pod0-cli`/headless hosts per the existing "native executes platform primitives" split, so tracing output only needs to exist on the CLI/test path where it's actually exercised. |
| `wiremock` | `=0.6.x` | HTTP mocking for provider-failure and retry-classification tests | Use in `pod0-live-hosts`/`pod0-tts-host` test suites to exercise timeout, connect-refused, 429-with-`Retry-After`, 500, and malformed-JSON responses against the real `reqwest::Client` over a loopback TCP listener — matching the pattern already used in `pod0-live-hosts/tests/http_tcp.rs` and `provider_tcp.rs` (hand-rolled TCP fixtures). `wiremock` reduces the boilerplate of those hand-rolled fixtures for new provider-failure-mode tests without changing the existing ones. Optional, not required — the existing TCP-fixture pattern already works and needs no forced migration. |
| `rustyline` | `=18.0.1` (current pin, `pod0-cli`) | Interactive REPL line editing | Already correct for the CLI's `--repl` mode; no change. |
| `zeroize` | `=1.9.0` (current pin) | Zero credential memory on drop | Already applied to provider secrets in `pod0-live-hosts`/`pod0-tts-host`/`pod0-nostr-host`; keep this discipline for any new credential-holding struct in these crates. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `cargo-deny` | License/advisory/source policy gate | Already wired via `rust/deny.toml` and `scripts/check_rust.sh`; any new dependency (e.g. `tracing`) must clear this before merge — `tracing`/`tracing-subscriber` are MIT, so they pass the existing `[licenses] allow` list unmodified. |
| `cargo-audit` | Vulnerability scanning | Already part of `scripts/check_rust.sh`; run after any `Cargo.lock` change from adding `tracing`. |

## Installation

```bash
# From rust/ — add tracing to the crates that make provider HTTP calls
cd rust
cargo add tracing --exact --package pod0-live-hosts
cargo add tracing --exact --package pod0-tts-host
cargo add tracing-subscriber --exact --package pod0-cli --features fmt

# Optional: HTTP mocking for new provider-failure tests
cargo add wiremock --exact --package pod0-live-hosts --dev
```

Pin exact versions in each crate's `[dependencies]` table to match this workspace's existing discipline (every dependency in `rust/Cargo.toml` and every crate `Cargo.toml` uses `=x.y.z` exact pins, not caret ranges) — do not let `cargo add` leave a caret range in place.

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|--------------------------|
| Keep facade synchronous + lease/poll host effects | `uniffi` async exported functions (`0.28+`) returning Rust futures directly to Swift `async`/`await` | Only once UniFFI's Swift async output is confirmed `Sendable`-clean under Swift 6 in this project's own CI — not now. Revisit if a future UniFFI release explicitly documents Swift 6 Sendable conformance for async exports. |
| Failure classification + `Retryability` feeds the durable effect outbox | `reqwest-middleware` (astral-sh, formerly TrueLayer) + `reqwest-retry` client-side exponential backoff | Appropriate for a stateless CLI tool with no durable retry ledger of its own. Pod0 already has one (the effect outbox); adding client-side retry here creates two independent retry authorities racing on the same request. |
| Hand-rolled loopback-TCP test fixtures (existing) or `wiremock` for new tests | `mockito` | `mockito` is a smaller, simpler mock server but has less flexible matcher/response-sequencing support than `wiremock` for the redirect-chain and conditional-GET (ETag/If-Modified-Since) scenarios `pod0-live-hosts::http.rs` already exercises. Only reach for `mockito` if `wiremock`'s tokio-runtime requirement conflicts with a specific test's threading model. |
| `tracing` for observability | `log` crate (already a dependency of `pod0-nostr-host` only) | `log` is fine for pod0-nostr-host's existing narrow use (it's not on the HTTP path); do not extend plain `log` to the HTTP-call sites — `tracing`'s spans are what let a single agent turn's provider call be correlated end-to-end, which `log`'s flat records cannot do without manual ID-threading in every message. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|--------------|
| A second `reqwest::blocking::Client` per host module (current state in `pod0-cli/src/host.rs`) | Duplicates connection pools and TLS setup that `pod0_live_hosts::LiveHosts` already provides on the same struct; doubles the surface that needs timeout/retry/observability work applied to it | Route all agent-chat HTTP (`host/agent_http.rs`) through the existing `LiveHosts` instance already held by `HostExecutor`, using the `current_thread` Tokio runtime already constructed in `HostExecutor::new` to drive the `.await` calls synchronously from the blocking host-pump thread |
| `uniffi` async exported functions on `Pod0Facade` for this milestone | Swift 6 `Sendable` conformance for UniFFI-generated async Swift bindings is explicitly unresolved per UniFFI's own docs; adopting it now imports an upstream concurrency-safety gap into voice-turn cancellation, which is exactly the property #142 needs to get right | The existing synchronous lease/poll/outbox pattern already in `Pod0Facade` |
| Client-side exponential-backoff retry middleware (`reqwest-middleware`/`reqwest-retry`) wrapping the host-adapter `Client` | Would retry independently of, and race against, the effect outbox's own lease-expiry/backoff scheduling in SQLite — two retry authorities for one durable decision | Return accurate `Retryability`/`retry_after` classification from the adapter (`ProviderError` already has the fields) and let the outbox own retry timing |
| `reqwest` `0.13.x` for this milestone | Major version bump (MSRV 1.85+, default TLS backend changes) unrelated to #142's scope; every dependency in this workspace is exact-pinned and upgraded deliberately via `cargo-deny`/`cargo-audit` gates, not opportunistically | Stay on the current `=0.12.28` pin; track the `0.13` upgrade as separate, explicitly-scoped work |

## Stack Patterns by Variant

**If instrumenting provider HTTP calls for observability:**
- Add `tracing::instrument(skip(self, request))` (never `skip`-omit — the request/body must never land in a span field) on `LiveHosts::http_get`, `chat`, `transcription`, `embeddings`, and `pod0-tts-host`'s synthesis call.
- Record `request_id`, `cancellation_id`, provider kind, and status/duration as span fields; never record the URL unredacted (the crate already has `RedactedUrl` in `url_debug.rs` — reuse it in span fields) or the API key/body.
- Because the facade stays synchronous and only `pod0-cli`/headless hosts make the actual HTTP calls, only `pod0-cli`'s `main.rs` needs to install a `tracing_subscriber` — no Swift-side log bridging is required for this milestone.

**If adding a new headless host crate that calls an AI provider (the CLI/testing path):**
- Follow the existing `pod0-live-hosts` shape exactly: one `Client` built once in a `new(config)` constructor, `ClientConfig` struct with `connect_timeout` (not just request timeout) and `user_agent`, a `CancellationToken` checked before and raced against every request via `tokio::select!`, and a bounded-size read (`bounded_body`) on every response — this is already a coherent, repeatable pattern across `pod0-live-hosts`, `pod0-tts-host`, and `pod0-portable-media`. Do not invent a new HTTP-client-lifecycle shape for a sixth crate.

**If a host crate needs both a durable/leased path (iOS-parity) and a direct/blocking path (CLI convenience):**
- Don't build two separate HTTP clients (see "What NOT to Use" above). Build one async client and drive it from a `current_thread` Tokio runtime held by the synchronous caller, exactly as `HostExecutor` already does for `pod0-live-hosts`-routed calls — extend that same runtime handle to cover agent-chat calls too instead of adding a parallel blocking client.

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|------------------|-------|
| `reqwest =0.12.28` | `tokio =1.53.1` | Confirmed compatible — already building and passing in this workspace today. |
| `tracing =0.1.41` | `tracing-subscriber =0.3.19` | Standard pairing; `tracing-subscriber` 0.3.x is the current stable major and tracks `tracing` 0.1.x. |
| `reqwest =0.12.28` | `reqwest-middleware` (if ever reconsidered) | `reqwest-middleware` `0.4.x` depends on `reqwest ^0.12.0` — compatible if this guidance is ever revisited, but not recommended now (see "What NOT to Use"). |
| `uniffi =0.32.0` | Swift 6 async/`Sendable` | Unresolved per UniFFI's own docs as of this research — do not depend on async FFI conformance in Swift 6 for #142. |

## Sources

- `rust/crates/pod0-live-hosts/src/{client.rs,http.rs,error.rs,cancellation.rs}` — verified existing client-reuse, cancellation, and error-classification pattern directly from source
- `rust/crates/pod0-cli/src/{host.rs,host/agent_http.rs,app/host_loop.rs}` — verified the duplicate blocking-client gap and the existing sync-drives-async host-pump pattern directly from source
- `rust/crates/pod0-facade/src/runtime.rs` — verified the facade exports zero `async fn`; confirmed lease/poll pattern (`pending_host_effects`, `next_leased_headless_host_requests`, `record_leased_host_observation`)
- `rust/crates/pod0-application/src/contract_failure.rs` — verified `Retryability` is already a durable, UniFFI-exported core concept
- `rust/Cargo.toml`, `rust/crates/*/Cargo.toml` — verified current exact-pinned versions for `reqwest`, `tokio`, `uniffi`, and confirmed the workspace's exact-pin discipline
- [crates.io: reqwest versions](https://crates.io/crates/reqwest/versions) — confirmed `0.12.28` is one major behind current upstream (`0.13.4`); MEDIUM confidence on exact 0.13 release cadence details, HIGH confidence on "don't upgrade mid-milestone"
- [crates.io: tokio](https://crates.io/crates/tokio) — confirmed `1.53.1` pin is current
- [Mozilla UniFFI: Async Overview](https://mozilla.github.io/uniffi-rs/latest/internals/async-overview.html) and [UniFFI: Async/Future support](https://mozilla.github.io/uniffi-rs/0.28/futures.html) — confirmed async trait/callback FFI mechanism and the documented Swift 6 `Sendable` gap
- [TrueLayer/astral-sh reqwest-middleware](https://github.com/TrueLayer/reqwest-middleware/) and [reqwest-retry docs](https://docs.rs/reqwest-retry) — confirmed this is the standard retry-middleware approach for reqwest generally, used here only to explain why it is *not* the right fit for pod0's durable-outbox architecture
- [backon crate](https://github.com/Xuanwo/backon) — confirmed as a lighter-weight alternative retry primitive; not recommended for the same durable-authority reason as reqwest-middleware

---
*Stack research for: Rust workspace UniFFI facade + headless async HTTP host crates (Pod0 voice-agent cutover, #142)*
*Researched: 2026-08-22*
