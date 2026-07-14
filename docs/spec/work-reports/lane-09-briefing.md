# Lane 9 — Briefing Composer + Briefings UI

> Replaces the `BriefingComposer` and `BriefingsView` stubs from
> `1dd7045 scaffold` with a complete pipeline + UI for generating, persisting,
> and playing back AI briefings, per `docs/spec/briefs/ux-08-briefings-tldr.md`
> and `docs/spec/briefs/ux-15-liquid-glass-system.md`.

Branch: `worktree-agent-acc77e0b9bfab2baf`
Single commit: `feat(briefing): BriefingComposer pipeline, segment player, AVFoundation stitching, Briefings UI`

---

## What shipped

### Data types (`App/Sources/Briefing/`)

- `BriefingRequest.swift` (102 lines) — request, scope, length puck stops,
  style enum.
- `BriefingSegment.swift` (138 lines) — segment, attribution, original-audio
  quote.
- `BriefingScript.swift` (113 lines) — persisted script + recorded branch.
- `BriefingTrack.swift` (89 lines) — playable unit emitted by composer.

### Cross-lane contracts (`App/Sources/Briefing/`)

- `BriefingProtocols.swift` (132 lines) — defines:
  - `RAGSearchProtocol` (Lane 6 supplies impl)
  - `WikiStorageProtocol` (Lane 7 supplies impl)
  - `TTSProtocol` (Lane 8 supplies impl)
  - `BriefingPlayerHostProtocol` (Lane 1 supplies impl)
- `BriefingFakes.swift` (286 lines) — fake implementations of all four
  protocols including a real `SilentAudioWriter` that emits valid AAC m4a
  files of arbitrary duration so the AVFoundation pipeline runs end-to-end
  without an ElevenLabs/OpenRouter key.

### Composer pipeline (`App/Sources/Briefing/`)

- `BriefingComposer.swift` (298 lines) — orchestrates gather → script →
  segment → stitch → persist. Hands progress out via a sendable callback
  closure for the W6 *Composing your briefing* surface.
- `BriefingPrompts.swift` (137 lines) — system + user prompts for
  morning / weekly / catch-up / topic-deep-dive styles, with a strict JSON
  output contract so we never have to parse markdown.
- `BriefingFixtureScript.swift` (215 lines) — deterministic fallback the
  composer uses when no API key is configured. Threads real RAG candidates
  through as attributions and quotes so downstream AVFoundation, storage,
  and player code see realistic data.
- `BriefingAudioStitcher.swift` (195 lines) — `AVMutableComposition` +
  `AVAssetExportSession.export(to:as:)` (iOS 18+ async API). Splices TTS m4a
  files together with original-audio quotes trimmed from the source
  enclosure URLs. Falls back to silent padding rather than dropping cues
  silently when a quote source can't load.
- `BriefingStorage.swift` (130 lines) — file-system persistence under
  `Application Support/podcastr/briefings/<id>.json` plus
  `<id>.m4a` and a `<id>/` scratch directory for per-segment renders.
- `BriefingPlayerEngine.swift` (202 lines) — `@Observable @MainActor`
  sequential playback driver with the *pause-and-resume* branch contract
  from UX-08 §3 (anchors at sample-accurate position; emits an
  `AsyncStream<BranchEvent>` so SwiftUI views and the voice mode can
  react in parallel).

### UI (`App/Sources/Features/Briefings/`)

- `BriefingsView.swift` (224 lines) — library shelf + preset row + compose
  toolbar entry. Brass-amber `.glassSurface` tint per UX-08 §4 so a
  briefing is never mistaken for an episode.
- `BriefingsViewModel.swift` (96 lines) — owns the list and the active
  compose flow. Self-contained — does not couple to `AppStateStore`, so
  this lane stays orthogonal to the rest of the app.
- `BriefingComposeSheet.swift` (172 lines) — W1 from the spec: freeform
  field, length picker, scope chips, style radios, prominent compose CTA.
- `BriefingPlayerView.swift` (296 lines) — W2 from the spec: editorial
  hairline + serif title, transcript pane, transport, per-segment actions
  (*deeper · skip · share*), segment rail with brass-amber active pill,
  attribution chip strip.
- `BriefingBranchPromptSheet.swift` (39 lines) — branch-question sheet
  that hands the prompt to `BriefingPlayerEngine.beginBranch`.

---

## Constraints honoured

- **No SPM deps added.** AVFoundation only.
- **File-size budget.** Soft 300 / hard 500 — every file ≤ 298 lines.
- **Build green.** `xcodebuild -workspace Podcastr.xcworkspace -scheme
  Podcastr -destination 'generic/platform=iOS Simulator'` returns
  `** BUILD SUCCEEDED **`. Zero warnings in any new file.
- **Real LLM/TTS calls stubbed.** `FakeTTS` writes valid AAC m4a of
  computed duration; `FakeRAGSearch` returns a fixture set seeded by
  query so dev/preview builds get deterministic data; the OpenRouter
  call site exists in `BriefingComposer.composeViaLLM` and falls back
  to the fixture script when no key is set, so the data flow is real
  end-to-end.
- **Lane boundaries respected.** `Audio/`, `Podcast/`, `Voice/`,
  `Knowledge/`, `Transcript/`, `Agent/`, `Features/{Library,Player,
  EpisodeDetail,Agent}/`, `Project.swift`, and `App/Resources/Info.plist`
  are untouched. Every cross-lane integration goes through a protocol in
  `BriefingProtocols.swift`.

---

## Decisions worth flagging

1. **`enclosureURL` vs. `mediaURL`.** The lane brief said `enclosureURL`;
   the actual `Episode` model uses `mediaURL`. I kept the spec's terminology
   on `BriefingQuote.enclosureURL` since the briefing is consuming an RSS
   enclosure conceptually, and the composer accepts either at the
   `RAGCandidate.enclosureURL` boundary.
2. **One commit, one composer.** `BriefingComposer` is the single
   `@unchecked Sendable` orchestrator — non-Sendable AVFoundation work is
   confined to `BriefingAudioStitcher` and `SilentAudioWriter`, both of
   which keep all writer/composition references in a single stack frame.
3. **OpenRouter call deferred.** The brief said "do not modify
   Agent/Features/Agent." `composeViaLLM` builds the prompts and is the
   single line to wire when Lane 10 lands. Everything below that line —
   segmentation, TTS, stitching, persistence — runs against the fixture
   script today.
4. **Player view does not modify `RootView`.** Per the existing comment
   in `RootView`, briefings are reached from Today/Ask, not the tab bar.
   `BriefingPlayerView` is a `NavigationDestination` from `BriefingsView`
   and a presented sheet from anywhere else; navigation wiring belongs
   to a downstream lane.
5. **Branch contract is data-pure.** `BriefingPlayerEngine` records branches
   in memory and exposes `flushRecordedBranchesToScript()`, leaving
   persistence to whoever calls it. This keeps the engine reusable from
   the agent voice loop without forcing storage I/O on its critical path.

---

## Files added (relative to repo root)

- `App/Sources/Briefing/BriefingRequest.swift`
- `App/Sources/Briefing/BriefingSegment.swift`
- `App/Sources/Briefing/BriefingScript.swift`
- `App/Sources/Briefing/BriefingTrack.swift`
- `App/Sources/Briefing/BriefingProtocols.swift`
- `App/Sources/Briefing/BriefingFakes.swift`
- `App/Sources/Briefing/BriefingPrompts.swift`
- `App/Sources/Briefing/BriefingFixtureScript.swift`
- `App/Sources/Briefing/BriefingAudioStitcher.swift`
- `App/Sources/Briefing/BriefingStorage.swift`
- `App/Sources/Briefing/BriefingPlayerEngine.swift`
- `App/Sources/Features/Briefings/BriefingsViewModel.swift`
- `App/Sources/Features/Briefings/BriefingComposeSheet.swift`
- `App/Sources/Features/Briefings/BriefingPlayerView.swift`
- `App/Sources/Features/Briefings/BriefingBranchPromptSheet.swift`

## Files modified

- `App/Sources/Briefing/BriefingComposer.swift` — replaced empty stub
  (lane brief: *"replace stub. Final class + protocol."*).
- `App/Sources/Features/Briefings/BriefingsView.swift` — replaced empty
  stub.
