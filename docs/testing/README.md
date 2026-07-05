# Podcastr iOS Simulator Test Scenario Suite

This directory contains an exhaustive set of manual / agent-driven test scenarios
for the **Podcastr** iOS app (a Nostr-native podcast client). Scenarios are written
to be executed by Haiku agents driving a **real iOS simulator** through the
`xcodebuildmcp` (XcodeBuildMCP) UI-automation tooling — taps, swipes, text entry,
screenshots, and UI-tree snapshots.

## Architecture recap (so you know what you are testing)

- **iOS Swift frontend** (`App/Sources/`) is a *thin renderer*. It owns almost no state.
- **Rust kernel via FFI** (`apps/nmp-app-podcast/`) owns ALL state. The Swift shell
  dispatches typed actions into the kernel and renders projections it pushes back.
- **Nostr-native**: identity is a Nostr keypair; NIP-84 for highlights/clippings,
  NIP-F4 (kind `10154`/`54`/`10064`) for podcast publishing & discovery, NIP-46 for
  remote signing (bunker / Amber / nsec.app).
- **Transcripts**: either supplied by the podcast RSS feed (Podcasting 2.0
  `<podcast:transcript>`), or auto-generated via a configurable STT provider
  (OpenRouter Whisper, ElevenLabs Scribe, AssemblyAI, or on-device Apple STT).
- **AI Agent**: chats about episode content using a configured LLM provider.
  For local testing: **Ollama** at `http://localhost:11434/api/chat`, model
  `deepseek-v4-flash:cloud`. **OpenRouter** is used for Whisper transcription.

Because state is kernel-owned, most reactive UI updates ride a "push frame" from
Rust. Watch for cases where the UI does not update because the kernel did not bump
the relevant projection revision (a recurring class of bug — see Watch Points in
individual scenarios).

## Simulator configuration

Use the dedicated test simulator:

| Field        | Value                                  |
|--------------|----------------------------------------|
| Name         | `podcast-iter`                         |
| iOS version  | 26.5                                   |
| UDID         | `9956D3C2-466B-4005-A5FF-1B018B8DE734` |

Boot it before running any scenario:

```
xcode: boot_sim  { simulatorUuid: "9956D3C2-466B-4005-A5FF-1B018B8DE734" }
xcode: open_sim
```

> NOTE: Multiple agents may share one simulator. If another agent installed a
> different build, **reinstall your build** before UI-verifying (see MEMORY:
> "Shared simulator build clobber").

### Bundle / scheme

- Bundle ID: `io.f7z.podcast` (NOT `com.podcastr.app` — that is the App Group).
- Build the app for the simulator with the `xcode` MCP tools
  (`session_show_defaults` first, then `build_run_sim`). The Rust core must be
  cross-compiled for `aarch64-apple-ios-sim` as part of the build.

### Useful launch arguments (UI-test seams)

The app reads these from `CommandLine.arguments` (see `App/Sources/UITestSeeder.swift`):

| Argument                  | Effect                                                                  |
|---------------------------|-------------------------------------------------------------------------|
| `--UITestSeed`            | Overwrites `podcasts.json` with a deterministic seeded library.         |
| `--UITestSeedRelaunch`    | Preserves the kernel's existing `podcasts.json` (for relaunch tests).   |
| `--UITestSeedOrphanClip`  | Writes a `clips.json` directly (orphan-clip edge case).                 |
| `--UITestAgentStub`       | Agent returns a canned stub reply (no live LLM needed).                 |

Most discovery / Nostr / live-LLM scenarios should run **without** seeding so you
exercise real network + kernel paths. Playback / library / clippings scenarios can
use `--UITestSeed` to get to a known state quickly. Each scenario's Prerequisites
section says which mode it expects.

## Provider configuration notes

Several scenarios require AI providers to be configured. Configure them once
(Scenario I1/I2) and they persist in the iOS Keychain across launches (unless the
app data is wiped).

- **Ollama (agent LLM)**: Settings → Intelligence → Providers → Ollama Cloud, OR
  configure the endpoint. For a *local* Ollama, set the endpoint to
  `http://localhost:11434/api/chat`. Select model `deepseek-v4-flash:cloud` under
  Settings → Intelligence → Models. The local Ollama server must be running on the
  host machine for live agent scenarios.
- **OpenRouter (Whisper transcription)**: Settings → Intelligence → Providers →
  OpenRouter → "Enter OpenRouter key manually". Enter your OpenRouter API key.
  Then Settings → Intelligence → Transcripts → enable "AI transcription fallback"
  and select Whisper as the speech provider under Models → Speech.

> A simulator cannot reach a `localhost` Ollama unless the server is on the same
> host the simulator runs on (it is — the simulator shares the Mac's network).
> If agent calls fail with connection errors, confirm `ollama serve` is up and
> the model is pulled.

### Provider cassette replay

Provider-backed scenarios must not be blocked on live credentials. Redacted
provider cassettes live under `tests/fixtures/provider_cassettes/` and are
verified by:

```
cargo run -p nmp-app-podcast --bin provider-cassettes -- verify tests/fixtures/provider_cassettes
```

Set `POD0_PROVIDER_CASSETTE_DIR=tests/fixtures/provider_cassettes` to run the
Rust provider transports in replay mode. See
[`provider-cassettes.md`](provider-cassettes.md) for the cassette contract,
current coverage, and redacted audio URL convention.

## How to run a scenario

1. Boot the simulator and install/launch the build (with any launch args the
   scenario's Prerequisites call for).
2. Follow the numbered **Steps**. Each step has an action, an expected result,
   and a screenshot checkpoint. Take a screenshot (`xcode: screenshot`) and/or a
   UI-tree snapshot (`xcode: snapshot_ui`) at each checkpoint.
3. Prefer `snapshot_ui` to locate elements by `accessibilityIdentifier` /
   `accessibilityLabel` before tapping. Scenarios list the known identifiers.
4. Compare observed behavior against **Acceptance Criteria**.
5. Record results in the scenario file's **Notes** section (see below).

## How to read / write testing Notes

Each scenario file ends with a `## Notes` section that starts **BLANK**. When you
run a scenario, append a dated entry there:

```
## Notes

### 2026-06-24 — run by <agent-id> — RESULT: PASS / FAIL / BLOCKED
- Step 4: scrubber did not respond to drag; seek only worked via skip buttons.
- Screenshot: <path or attachment ref>
- Build: <git sha / build number>
- Follow-up: filed as ...
```

When reading Notes to interpret prior results:
- **PASS** = all acceptance criteria met on that build.
- **FAIL** = a criterion was not met; read the step notes for the specific failure.
- **BLOCKED** = could not complete (missing provider, build clobbered, crash on
  launch, etc.). A BLOCKED result is NOT a feature failure — re-establish the
  prerequisite and rerun.
- Always check the **Build** sha — a stale build is a common false-negative
  (see MEMORY: "perf_emit_pipeline_churn" — a 'pegged CPU' was just a stale build).

## Scenario index

See [`scenarios/INDEX.md`](scenarios/INDEX.md) for the full list with one-line
descriptions. Scenarios are grouped by letter:

- **A** — Onboarding & Identity
- **B** — Podcast Discovery & Search
- **C** — Library & Episode Management
- **D** — Playback
- **E** — Transcripts
- **F** — NIP-84 Highlights (Clippings)
- **G** — AI Agent Interaction
- **H** — Social / Nostr Features
- **I** — Settings
- **J** — Edge Cases & Regression

## General watch points (apply to every scenario)

- **Reactive push gaps**: if the UI doesn't update after an action, the kernel may
  not have bumped the relevant per-domain projection revision. Note it.
- **Scaffold vs. real**: per `docs/plan.md`, feature parity is *not* fully achieved;
  some surfaces are heuristics/scaffolds, not full behavior. If a feature looks
  stubbed, capture it in Notes rather than asserting a hard FAIL — cross-reference
  `docs/BACKLOG.md`.
- **Flaky UI gate**: the iOS UI test lane is known to be intermittently flaky on a
  clean tree (see MEMORY). If a scenario fails once, rerun before recording FAIL.
- **Off-main work**: snapshot decode and Now Playing artwork have a history of
  main-thread jank; watch for UI freezes during large library loads or playback start.
