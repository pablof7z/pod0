# Pod0 BDD Scenario Catalog

This catalog is a planning-level inventory for future automated, manual, and
agent-driven validation. It complements the executable scenario files one level
up; those files remain the canonical runbooks for currently runnable flows.

Each row is one concrete BDD scenario. Evidence is encoded per scenario:

- `SS`: screenshots or UI-tree snapshots to capture.
- `Perf`: metric or budget evidence to capture; `none` means no extra metric.
- `Deps`: seed, mock, live service, fixture relay, or cassette/replay need.
- `Boundary`: NMP/RMP doctrine exercised, or `none`.

LLM, STT, TTS, online search, and relay-network flows must be runnable in
deterministic replay. Cassettes should store request intent, provider response,
normalized tool output, token/cost metadata when available, and a replay clock.
Do not store provider API keys, private keys, or raw user secrets.

## Cluster Files

| Cluster | Scenarios | Scope |
|---|---:|---|
| [01 - Foundation, Identity, Discovery, Library](01-foundation-identity-discovery-library.md) | 64 | First run, accounts, discovery, search, subscriptions, library management. |
| [02 - Playback, Downloads, Transcripts, Clips](02-playback-downloads-transcripts-clips.md) | 64 | Audio, queue, downloads, transcript ingest, transcript UI, highlights, sharing. |
| [03 - Agent, LLM, Knowledge, Voice](03-agent-llm-knowledge-voice.md) | 72 | Agent chat, provider transport, cassettes, wiki/RAG, voice, generated media. |
| [04 - Nostr, Settings, Platform, Regression](04-nostr-settings-platform-regression.md) | 64 | NIP-F4/NIP-84/social, settings, Android/TUI parity, performance and doctrine. |
| [05 - Chirp And NMP Regression Parity](05-chirp-nmp-regression-parity.md) | 56 | Sister-repo fix parity, NMP master drift, projection stale-state, offline honesty, visual parity, and D8 gates. |
| [06 - Expanded Product Validation](06-expanded-product-validation.md) | 102 | First-run edge cases, discovery recovery, library persistence, playback routes, downloads, transcripts, clips, voice, offline replay honesty, settings, and social trust expansion. |
| **Total** | **422** | Comprehensive scenario-only catalog for Pod0. |

## Source Notes

- Existing runbooks: `docs/testing/scenarios/*.md`.
- Product and UX sources: `docs/spec/PRODUCT_SPEC.md`, split product-spec files,
  and UX briefs under `docs/spec/briefs/`.
- Current plan sources: `docs/plan.md`, `docs/BACKLOG.md`,
  `docs/plan/shared-llm-task-architecture.md`,
  `docs/plan/nmp-feature-parity.md`, and `docs/plan/pod0-nostr-publishing.md`.
- Code/test sources: `App/Sources/Features`, `AppUITests`, `AppTests`,
  `apps/nmp-app-podcast`, `apps/podcast-*`, `android/Podcast`, and
  `apps/podcast-tui`.
- Sister repo patterns from `../chirp`: fixture relay, signed-event validation,
  projection cache replay, typed projection parity patterns, dead-button
  prevention, stale async state fixes, dark/light and Dynamic Type validation,
  offline/reconnect honesty, and NIP-05 lookup-state regression evidence.
