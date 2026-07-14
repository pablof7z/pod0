# Lane 05 — Transcripts + Episode Detail/Reader

> Engineer agent — worktree `worktree-agent-a5766641d8c9c8e8c`,
> branch identical, single commit.

## Summary

Two halves of Lane 5 landed together:

**A) Transcription stack.** Full Codable/Sendable `Transcript` data model;
Foundation-only parsers for WebVTT, SRT, and the Podcasting 2.0 JSON
transcript spec; a `PublisherTranscriptIngestor` that fetches a
`<podcast:transcript>` URL, sniffs format from MIME + extension + body, and
dispatches to the right parser; an `ElevenLabsScribeClient` actor that hits
`POST /v1/speech-to-text` (multipart) and exposes a stable submit/poll surface
(see "Polling note" below); and a `TranscriptionQueue` actor that
prioritises (now-playing > recently-subscribed > bulk), de-duplicates by
episode ID, and dispatches to either the publisher path or Scribe.

**B) Episode detail + transcript reader.** Three-mode `EpisodeDetailView`
(detail / reading / follow-along) with mode picker; editorial-typed
`TranscriptReaderView` with paragraph grouping, tap-to-jump, long-press to
open the share sheet, follow-along auto-scroll, and a VoiceOver paragraph
rotor; `ChapterRailView` built on `GlassEffectContainer` + `glassEffectID`
for morph-on-active per UX-15 §5; `QuoteShareView` for the export card;
`TranscribingInProgressView` for the Scribe-in-flight skeleton state; a
`MockEpisodeFixture` so the views render against realistic data while
Lane 2's `Episode` model is in flight; and a `DockedPlayerPlaceholder`
holding the dock geometry until Lane 4 ships the real player.

## Files added (17)

```
App/Sources/Transcript/Transcript.swift                                 143
App/Sources/Transcript/ElevenLabsScribeClient.swift                     296
App/Sources/Transcript/TranscriptionQueue.swift                         154
App/Sources/Transcript/Parsing/VTTParser.swift                          172
App/Sources/Transcript/Parsing/SRTParser.swift                          166
App/Sources/Transcript/Parsing/PodcastingTranscriptJSONParser.swift     110
App/Sources/Transcript/Parsing/PublisherTranscriptIngestor.swift        138

App/Sources/Features/EpisodeDetail/EpisodeDetailView.swift              167
App/Sources/Features/EpisodeDetail/EpisodeDetailHeroView.swift          185
App/Sources/Features/EpisodeDetail/TranscriptReaderView.swift           179
App/Sources/Features/EpisodeDetail/ChapterRailView.swift                 79
App/Sources/Features/EpisodeDetail/DockedPlayerPlaceholder.swift         49
App/Sources/Features/EpisodeDetail/QuoteShareView.swift                 146
App/Sources/Features/EpisodeDetail/TranscribingInProgressView.swift     160
App/Sources/Features/EpisodeDetail/MockEpisodeFixture.swift             125

AppTests/Sources/VTTParserTests.swift                                   143
AppTests/Sources/SRTParserTests.swift                                    99
```

Every file is under the AGENTS.md soft 300-line limit. None approach the hard
500-line limit.

## Data model

The brief overrides the broader research enumeration. Final shape:

```swift
struct Transcript: Codable, Sendable, Identifiable, Hashable {
    let id: UUID
    let episodeID: UUID
    let language: String                 // BCP-47
    let source: TranscriptSource         // .publisher | .scribeV1 | .onDevice
    let segments: [Segment]
    let speakers: [Speaker]
    let generatedAt: Date
}

struct Segment: Codable, Sendable, Hashable, Identifiable {
    let id: UUID
    let start: TimeInterval
    let end: TimeInterval
    let speakerID: UUID?
    let text: String
    let words: [Word]?
}

struct Word: Codable, Sendable, Hashable {
    let start: TimeInterval; let end: TimeInterval; let text: String
}

struct Speaker: Codable, Sendable, Hashable, Identifiable {
    let id: UUID
    let label: String
    let displayName: String?
}

enum TranscriptSource: String, Codable, Sendable, Hashable, CaseIterable {
    case publisher; case scribeV1; case onDevice
}
```

`Transcript` carries an O(log n) `segment(at: TimeInterval)` lookup used by
the follow-along reader.

## Parsers

All Foundation-only — no SPM deps were added.

- **VTTParser** — recognises the standard header, `NOTE`/`STYLE`/`REGION`
  blocks (skipped), `HH:MM:SS.mmm` and the abbreviated `MM:SS.mmm` timing
  forms, `<v Speaker>...</v>` diarization tags, and inline formatting tags
  (which it strips). Speakers are stable-IDed across cues.
- **SRTParser** — accepts both comma and dot decimal separators (wild SRT
  files use both), strips `>>`/`>` chevrons, recognises three speaker
  conventions (`Name: …`, `[Name]: …`, `>> Name: …`), and rejects
  spurious colons inside body text via a plausibility heuristic
  (`isPlausibleSpeakerLabel`).
- **PodcastingTranscriptJSONParser** — Foundation `JSONSerialization`,
  accepts both `body`/`text` field names, `startTime`/`start` keys (some
  publishers ship Podcasting 2.0 with subtle key drift), optional
  per-segment `words` arrays, and stringified numbers.

## Tests

15 new tests, all passing on iPhone 17 Pro (iOS 26). Run via the regular
`AppTemplateTests` target.

```
VTTParserTests
  testParsesSimpleCues
  testExtractsSpeakerFromVTag
  testReusesSpeakerIDAcrossCues
  testHandlesShortMMSSTimestamps
  testIgnoresNoteAndStyleBlocks
  testStripsInlineFormattingTags
  testSegmentsAreSortedByStart
  testThrowsWhenHeaderMissing

SRTParserTests
  testParsesNumberedCues
  testAcceptsDotDecimalSeparator
  testExtractsTitleCaseSpeakerPrefix
  testExtractsBracketedSpeakerLabel
  testIgnoresSpuriousColon
  testSortsSegmentsByStart
  testEmptyInputThrows
```

Combined with the pre-existing 17 tests, the full suite passes (32 tests, 0
failures).

## ElevenLabs Scribe client

`actor ElevenLabsScribeClient` reads its API key via the existing
`ElevenLabsCredentialStore` (Keychain). Submission is documented multipart:

```
POST https://api.elevenlabs.io/v1/speech-to-text
xi-api-key: <key>
multipart fields: model_id=scribe_v2, file=<bytes>, diarize=true,
                  timestamps_granularity=word, tag_audio_events=true,
                  language_code=<bcp47?>
```

The response decoder accepts both shapes seen in the wild:
1. **Sync inline** — `{ language_code, words[] }`. Returned for short clips.
2. **Async job** — `{ request_id, status }`. Returned for long jobs.

`Transcript.fromScribeRaw(_:)` converts Scribe word lists into our segment
shape using two heuristics: speaker-ID switch and >1.2s pause boundary.

### Polling note

Per `transcription-stack.md`, ElevenLabs Scribe's production async flow is
**webhook-only** for jobs longer than a few minutes — there is no
documented `GET /v1/speech-to-text/{job_id}` endpoint. Per the brief, I
implemented the API surface (`submit` → `pollResult`) but `pollResult`'s
loop probes that URL and gracefully falls back to `.webhookOnlyMode` if it
404s. Backoff is 2s → 4s → 8s → 16s → 30s capped, total deadline 10
minutes. The webhook path lives in a future lane (likely Lane 9 + a server
component).

## Episode detail + reader views

### Three modes

`EpisodeDetailView.Mode` — `.detail | .reading | .followAlong`. Toggled via
a segmented `Picker` in the navigation toolbar; gestural transitions
described in the brief (pull-up / tap-to-play / scrub-rail) are deferred
to a polish pass since the gesture library lives across Lanes 4+15.

### Reader

`TranscriptReaderView` typesets paragraphs with `Font.system(.serif)` (New
York), SF Rounded for speaker labels, SF Mono for timestamps. Paragraphs
are grouped by speaker switch in a private `paragraphGroups(_:)` step.
Tap-to-jump and long-press-to-share are wired through callbacks; the
`@State activeSegmentID` highlights the active paragraph and auto-scrolls
to upper-third when `followAlong` is on.

VoiceOver: a custom `accessibilityRotor("Paragraphs")` exposes per-segment
entries like `"Peter Attia, 14:31, paragraph"` per UX-03 §8.

### Chapter rail

`ChapterRailView` uses `GlassEffectContainer(spacing: 12)` with
`.glassEffect(.regular.interactive(), in: .capsule)` per child and a shared
`@Namespace` for `glassEffectID`. The active chapter expands its capsule
with the chapter title; inactive chapters render as small dots. The
container is what makes the morph read as one liquid bead per UX-15 §5.

### Player chrome

`DockedPlayerPlaceholder` is intentionally minimal — Lane 4 owns the player
internals. We own dock geometry: it appears in `.detail` and
`.followAlong`, vanishes in `.reading`. Tinted with `Color.orange.opacity(0.10)`
as a stand-in for `accentPlayer`.

### In-progress state

`TranscribingInProgressView` shows the partial transcript with a blinking
cursor, three animated skeleton lines below it, and the ETA + "ElevenLabs
Scribe" attribution per UX-03 §6.4. CTA "Notify me when ready" is a no-op
button (Lane 9 wires push).

## Mocks

`MockEpisodeFixture.timFerrissKeto()` returns a believable two-speaker
sample with chapter markers; `MockEpisodeFixture.inProgress()` returns a
near-empty Scribe-style transcript with one segment so the in-progress view
renders. `MockEpisode` is the Episode-shaped struct kept inside this lane;
when Lane 2's `Episode` lands, replace the type alias and delete this file.

## Build + test status

- `tuist generate` clean.
- `xcodebuild build` on iPhone 17 Pro (iOS 26): **BUILD SUCCEEDED**.
- `xcodebuild test`: **TEST SUCCEEDED — 32/32 passing**.
- Swift 6 strict concurrency (`SWIFT_STRICT_CONCURRENCY: complete`):
  no warnings on Lane-5 files.

## Constraints respected

- **No SPM deps added** — all parsing is `String`/`JSONSerialization`/Foundation.
- **No Audio/Podcast/Library/Player/Knowledge/Voice/Briefing edits** —
  Lane 5 only writes inside `App/Sources/Transcript/`,
  `App/Sources/Features/EpisodeDetail/`, and `AppTests/Sources/`.
- **No edits** to `Project.swift`, `App/Resources/Info.plist`, or any
  service file outside the lane (the existing
  `ElevenLabsCredentialStore` is consumed read-only).
- **File-size limits**: every file ≤ 296 lines, soft 300 limit honoured.

## Hand-off notes

1. Lane 2 should replace `MockEpisode` in `MockEpisodeFixture.swift` with
   their `Episode` type — either by deleting `MockEpisode` and re-pointing
   the views, or by making `MockEpisode` a `typealias`.
2. Lane 4 should replace `DockedPlayerPlaceholder` with their actual mini
   player. The placeholder already accepts `currentTime` + `duration`, the
   surface area Lane 4's player will need.
3. Lane 9 (Briefing) or a dedicated server lane should wire the Scribe
   webhook path — `ElevenLabsScribeClient.pollResult` currently throws
   `.webhookOnlyMode` on 404, which is the signal to switch.
4. UX-04 (wiki) and UX-05 (agent) hand-offs in the reader are stubs in the
   brief — the long-press action bar in `TranscriptRow` only fires
   `onShare` for now; Ask-Agent / Wiki-Link should land in a polish lane.

## Branch + commit + report

- **Branch**: `worktree-agent-a5766641d8c9c8e8c`
- **Commit**: see git log of this branch (single commit per the brief).
- **Report**: this file —
  `docs/spec/work-reports/lane-05-transcripts.md`
