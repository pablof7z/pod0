# Chirp / NMP Validation Pack - 2026-07-06

This pack turns recent shipped Chirp fixes and NMP doctrine drift into Pod0
validation requirements. It does not assert that Pod0 passes; it gives the
scenario report concrete pages, evidence gates, and issue links that agents can
work through until the product is actually proven.

## Current Inputs

| Source | Ref | Why it matters for Pod0 |
|---|---:|---|
| Pod0 source branch | `origin/main` at `2b295607` | Includes the merged scenario report generator and prior NMP/Chirp audit. |
| Pod0 NMP pin | `Cargo.toml` pin `1fc3e6bea390224cef30e37d2ccaa90615197521` | Still behind Chirp's shipped NMP master pin. |
| Chirp shipped master | `origin/master` at `b8474e74a4fa4a448d7d9e4c960afc5b314b2329` | Pins all NMP crates to `bc6b42592d7fd61bc6767cac246a24a6b23bf8e3`. |
| Chirp local validation master | `master` at `ef6138e69ec4ac9fd0d1658ff223c02626a3d1ae` | Adds validation evidence for offline/reconnect, dark/light, Dynamic Type, and performance. |
| NMP architecture scanner | `nmp_architecture_scan.py` | Flags concrete D8 sleep/polling paths now tracked as #734. |

## Added Scenario Coverage

New catalog file:
`docs/testing/scenarios/catalog/05-chirp-nmp-regression-parity.md`

The BDD catalog now has 320 scenarios total. The new 56-scenario Chirp/NMP
batch covers:

- `CHIRP-001` through `CHIRP-008`: dead-button prevention, swallowed dispatch
  rejection, stale async terminal states, action lifecycle retention, and
  late-result cancellation guards from Chirp #139/#140 patterns.
- `NMPM-001` through `NMPM-008`: coherent NMP re-pin, relay config runtime
  startup, relay/action-stage rate limiting, publish terminal states, relay URL
  ownership, and generated-code drift gates.
- `NIP05-001` through `NIP05-008`: scoped NIP-05 lookup state for Add Show/Add
  Friend, concurrent lookup isolation, offline failure honesty, retry, and
  redacted telemetry.
- `PROJ-001` through `PROJ-008`: stale revision rejection, projection reset,
  malformed sidecar behavior, account-scoped projection cleanup, dynamic
  projection unregister, and golden-frame decoder parity.
- `OFFLINE-001` through `OFFLINE-008`: offline publish pending states,
  reconnect flush, backoff diagnostics, uncached profile/show honesty, offline
  playback, cassette miss fail-closed behavior, and no-live-credential provider
  exploration.
- `VIS-001` through `VIS-008`: light/dark parity, Dynamic Type, Reduce Motion,
  Reduce Transparency, long metadata, ghosting through bars, and screenshot-level
  gh-pages critique requirements.
- `D8-001` through `D8-008`: haptics, press feedback, CarPlay startup, large
  snapshot decode, playback update cadence, provider replay rendering,
  generated report layout stability, and scanner-to-issue ownership.

## Issue Links

| Issue | Status | Validation requirement |
|---|---|---|
| #707 | Open | Re-pin NMP past publish/action lifecycle and persisted relay config fixes; validate `NMPM-*` plus publish/social flows. |
| #708 | Open | Adopt pollable NIP-05 lookup state; validate `NIP05-*`. |
| #709 | Open | Make action/projection registry drift checks runnable in CI; validate `NMPM-008` and `PROJ-008`. |
| #730 | Open | Add accessibility audit evidence for every scenario cluster; validate `VIS-*` and screenshot-level critiques. |
| #731 | Open | Normalize cassette IDs and fill cassette evidence gaps; validate `OFFLINE-007`, `OFFLINE-008`, and provider-backed LLM/STT/TTS/search pages. |
| #732 | Open | Expand OS integration coverage; validate CarPlay and capability scenarios. |
| #733 | Open | Automate xcresult evidence ingestion into scenario overlays. |
| #734 | Open | Remove Task.sleep polling paths from haptics, press feedback, and CarPlay startup; validate `D8-001`, `D8-002`, `D8-003`, and `D8-008`. |

## Chirp Evidence Mapped To Pod0

| Chirp source | Pod0 validation translation |
|---|---|
| `4a13013` / Chirp #139 fixed dead buttons using real interactive taps. | `CHIRP-001` requires every visible primary Pod0 control to dispatch one observable action/result. |
| `482c367` / Chirp #140 fixed swallowed dispatch rejections and stale/frozen async UI. | `CHIRP-002` through `CHIRP-008` force rejected, running, failed, canceled, and terminal states to be visible and fresh. |
| `3a800aa`, `b8474e7`, and Chirp's NMP pin to `bc6b42592`. | `NMPM-*` keeps Pod0's single-rev NMP discipline and validates relay config, publish lifecycle, and D8 rate limits. |
| `ef6138e` validation round 2 for S67/S68/S73/S76. | `OFFLINE-*` turns offline compose, reconnect flush, honest backoff, NIP-05 spinner, and relay add evidence into Pod0 scenarios. |
| `0402706` dark/light and Dynamic Type evidence. | `VIS-001` through `VIS-003` require Pod0 light/dark/Dynamic Type screenshot sets. |
| `acda9ac` clean-sim reverification plus performance pass evidence. | `D8-004`, `D8-005`, and `D8-007` require simulator performance traces and generated-page viewport checks. |

## Cassette And Replay Requirements

Existing cassette replay already verifies seven fixtures:

- OpenRouter chat completion.
- Ollama chat completion.
- OpenRouter embeddings.
- OpenRouter Whisper transcription.
- ElevenLabs Scribe transcription.
- AssemblyAI transcription.
- Perplexity search.

The new Chirp/NMP catalog adds replay pressure for gaps that still need fixtures
or overlays:

- NIP-05 success, failure, offline, retry, and concurrent unrelated resolution.
- Relay publish lifecycle: queued, ACK, reject, permanent failure, reconnect
  flush, and backoff.
- Projection golden frames for stale revision, reset, malformed sidecar, account
  switch, and dynamic unregister.
- Provider cassette miss fail-closed UI, not just Rust unit tests.
- CarPlay and haptics harness logs for D8 no-polling replacement.

Every cassette must remain redacted, deterministic, fail closed on a miss, and
carry scenario refs plus NMP doctrine tags. No provider key, nsec, bearer token,
raw private audio, or private relay material may appear in cassette bodies,
logs, screenshots, or generated pages.

## Report Publishing Expectations

After this catalog lands, the gh-pages generator should publish 320 scenario
pages. The new pages are expected to remain `incomplete` until real validation
attaches:

- step-by-step screenshots or video,
- UI-tree/accessibility snapshots,
- performance metrics with budgets,
- cassette IDs and replay logs,
- screenshot-level UI and UX critique,
- Liquid Glass and iOS primitive review,
- individual and grouped product-coherence judgments,
- linked issue and revalidation status.

This is intentional. The point is to make missing evidence visible rather than
allowing agents to claim blocked provider, NMP, or sister-repo parity work cannot
be explored.
