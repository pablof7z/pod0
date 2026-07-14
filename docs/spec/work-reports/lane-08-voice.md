# Lane 8 — Voice Conversational Mode

## Scope delivered

Replaced the (intended) `Voice/AudioConversationManager.swift` stub with a complete conversational voice stack:

- **State machine** for the live conversation (`idle | listening | thinking | speaking | duckedWhileBriefing | error(VoiceError)`)
- **Streaming STT** (`SpeechRecognizerService`) wrapping `SFSpeechRecognizer` with on-device-preferred recognition
- **Streaming TTS** (`ElevenLabsTTSClient`) targeting Flash v2.5 over WebSocket, with REST fallback to Multilingual v2
- **Local fallback TTS** (`AVSpeechFallback`) for offline / BYOK-declined scenarios
- **Barge-in detector** (`BargeInDetector`) with energy-threshold VAD and TTS-bleed suppression
- **Caption log** (`VoiceCaption` + `VoiceCaptionLog`) — a11y requirement
- **Voice UI** (`VoiceView`, `VoiceOrbView`) — full-screen session per `ux-06-voice-mode.md` brief intent
- **Integration protocols** for Lanes 1 (`AudioSessionCoordinatorProtocol`) and 9 (`VoiceBriefingHandle`)
- **Glue protocol** for the agent (`VoiceTurnDelegate`) — `AgentChatSession` is NOT modified by this lane

## State machine

```
                     ┌──────── interrupt ─────────┐
                     ▼                            │
idle ── PTT/ambient ──▶ listening ──▶ thinking ──▶ speaking
  ▲           barge-in │              │            │
  │                    │              │            │ briefing handoff
  │                    │              ▼            ▼
  └──── exit ──────────┴──── error ◀────── duckedWhileBriefing
```

Transitions are driven by `AudioConversationManager`'s `@MainActor`-isolated methods and observed via `@Observable`. Each transition cancels any in-flight task from the previous stage so cancellation discipline (especially for barge-in) is consistent across all paths.

`VoiceError` is used in the `.error(_)` case rather than the bare `Error` protocol so the state stays `Equatable`. This unblocks SwiftUI `.onChange(of:)` and `.animation(value:)` use.

## Files

| Path | Lines | Role |
|---|---|---|
| `App/Sources/Voice/AudioConversationManager.swift` | 471 | State machine + orchestration |
| `App/Sources/Voice/AudioSessionCoordinator.swift` | 63 | Lane 1 bridge protocol + no-op default |
| `App/Sources/Voice/VoiceTurnDelegate.swift` | 121 | Agent integration contract + stub |
| `App/Sources/Voice/SpeechRecognizerService.swift` | 221 | SFSpeechRecognizer streaming wrapper |
| `App/Sources/Voice/ElevenLabsTTSClient.swift` | 255 | Flash v2.5 WebSocket + REST fallback |
| `App/Sources/Voice/AVSpeechFallback.swift` | 105 | `AVSpeechSynthesizer` fallback |
| `App/Sources/Voice/BargeInDetector.swift` | 193 | VAD with TTS-bleed suppression |
| `App/Sources/Voice/VoiceCaption.swift` | 118 | Caption struct + observable log |
| `App/Sources/Features/Voice/VoiceView.swift` | 313 | Full-screen voice UI |
| `App/Sources/Features/Voice/VoiceOrbView.swift` | 197 | Reusable agent orb |

All under the 500-line hard limit. `VoiceView.swift` is 13 lines over the 300 soft limit; further extraction (caption rail subview, action row subview into separate files) is a future cleanup but not required.

## Latency targets — best-effort estimates

| Stage | Target | Approach | Estimate |
|---|---|---|---|
| STT first-partial | <300 ms | `SFSpeechRecognizer` streaming + on-device when supported | 150–250 ms after first vocalisation |
| Agent first-partial | LLM-bound | `AsyncThrowingStream` from `VoiceTurnDelegate` | matches OpenRouter SSE; ~500 ms typical |
| TTS first-byte | <100 ms | ElevenLabs Flash v2.5 over WebSocket, `optimize_streaming_latency=2` on REST fallback | sub-100 ms WS / ~400 ms REST |
| Barge-in detection | <150 ms | 3 sustained energy frames @ ~50 ms each + bleed subtraction | ~150 ms onset |
| End-to-end (user end-of-speech → first agent audio) | <1.2 s | sum of stages | best case ~700 ms, typical 1.0–1.2 s |

These are paper estimates from the iOS API surface and ElevenLabs published numbers — real-device measurement is the responsibility of integration QA, not the lane.

## iOS-version gating

Project deployment target is **iOS 26.0** (per `Project.swift`). Notes:

- The lane brief mentions "iOS 25" — this is treated as legacy phrasing. There is no live deployment branch below iOS 26, so reachable fallback code paths are written for **same-version-different-locale** robustness rather than older-OS support.
- `SFSpeechRecognizer` is the primary recogniser today. iOS 26's `SpeechAnalyzer` / `SpeechTranscriber` are intentionally **not** wired in this lane — adding them inside `SpeechRecognizerService` is a swap-in upgrade gated on `if #available(iOS 26.1, *)` (or equivalent point release marker) once the new APIs prove stable in the field. The protocol-based design (`SpeechRecognizerServiceProtocol`) makes that swap a single-file change.
- `SpeechDetector`-based VAD is similarly stubbed out — `BargeInDetectorProtocol` allows substitution without disturbing the manager.
- All ElevenLabs and `AVSpeech` APIs used here are stable on iOS 26.0.

## Integration protocols defined

### For Lane 1 (Audio)

```swift
protocol AudioSessionCoordinatorProtocol: Sendable {
    func beginVoiceCapture() async throws
    func beginVoicePlayback() async throws
    func duckOthersForBriefing() async throws
    func unduckOthersAfterBriefing() async throws
    func endVoiceSession() async
}
```

Lane 1 supplies the concrete implementation backed by the singleton `AVAudioSession`. Lane 8 ships `NoopAudioSessionCoordinator` so the manager builds and previews work today. The Voice tab calls `manager.setAudioCoordinator(_:)` after Lane 1's coordinator is constructed at app launch.

The audio coordinator is also the future home for the **TTS-frame play API** (e.g. `play(_ data: Data)`) — `AudioConversationManager.beginSpeaking` currently captures frames and feeds them into the barge-in detector, but does NOT yet route them to a player node. That's a one-line addition once Lane 1 lands.

### For Lane 9 (Briefings)

```swift
@MainActor
protocol VoiceBriefingHandle: AnyObject {
    func waitUntilFinished() async
}
```

Lane 9 invokes `manager.attachToBriefing(handle)`. The manager:

1. Cancels in-flight TTS.
2. Asks the audio coordinator to duck others.
3. Transitions to `.duckedWhileBriefing`.
4. Awaits the handle's `waitUntilFinished()`.
5. Restores the audio mix.
6. Resumes ambient listening (or returns to `idle` for PTT mode).

This contract lets Lane 9 own the briefing player entirely — the Voice manager just acts as a controller for the conversation state around the briefing.

### For Lane 10 (Agent tools) / orchestrator

```swift
@MainActor
protocol VoiceTurnDelegate: AnyObject {
    var canSubmit: Bool { get }
    func submitUtterance(_ text: String) -> AsyncThrowingStream<VoiceTurnEvent, Error>
}
```

`AgentChatSession` is NOT modified by Lane 8 (per spec). At merge time the orchestrator (or Lane 10) supplies a small adapter:

```swift
@MainActor
final class ChatSessionVoiceAdapter: VoiceTurnDelegate {
    private let session: AgentChatSession
    var canSubmit: Bool { session.canSend }
    func submitUtterance(_ text: String) -> AsyncThrowingStream<VoiceTurnEvent, Error> {
        // observe session.streamingContent + session.phase via withObservationTracking
        // translate into VoiceTurnEvent .partialText / .finalText / .toolInvocation
        // call session.startSend(text) once at the start
    }
}
```

The streaming-events shape was chosen over direct `@Observable` observation in the manager so:
- Cancellation propagates cleanly via `Task.cancel()` (critical for barge-in).
- The manager has zero coupling to `AgentChatSession`'s internals.
- Test doubles only need a one-method conformance.

A `StubVoiceTurnDelegate` is included for previews and tests; it streams a synthetic word-by-word echo so the manager exercises every state transition without an LLM.

## Permissions — orchestrator action required

Lane 8 deliberately does **not** modify `App/Resources/Info.plist` (per spec). The orchestrator MUST add these keys at merge time, otherwise the very first `startPushToTalk()` call fails silently:

| Key | Suggested value |
|---|---|
| `NSMicrophoneUsageDescription` | "Podcastr uses your microphone for voice conversations with the agent and for hands-free briefings." |
| `NSSpeechRecognitionUsageDescription` | "Podcastr transcribes your voice on-device so the agent can respond." |

## ElevenLabs WebSocket protocol — flagged for review

The codebase had no prior usage of the ElevenLabs streaming WebSocket. The implementation in `ElevenLabsTTSClient` matches the public `wss://api.elevenlabs.io/v1/text-to-speech/{voice}/stream-input?model_id=eleven_flash_v2_5` shape with:

- Initial JSON message: `{ "text": " ", "voice_settings": {…}, "xi_api_key": <key>, "generation_config": {…} }`
- Text push: `{ "text": "...", "try_trigger_generation": true }`
- End sentinel: `{ "text": "" }`
- Inbound audio frames as base64 `audio` field on JSON messages, with `isFinal: true` closing the stream.

This matches ElevenLabs' published documentation but should be **verified against a live key** during integration. If the schema has shifted, the swap point is `streamViaWebSocket(...)` — protocol shape (`AsyncThrowingStream<Data, Error>`) does not change.

REST fallback uses the documented `/v1/text-to-speech/{voice}/stream` endpoint with `optimize_streaming_latency=2` and `eleven_multilingual_v2` model — well-established and unlikely to surprise.

## What's NOT in this commit

- **Audio playback wiring**: TTS frames are captured for the barge-in detector but not yet routed to an `AVAudioPlayerNode`. That's a Lane 1 collaboration — see "For Lane 1 (Audio)" above.
- **Real `SpeechAnalyzer`/`SpeechTranscriber` path**: stubbed via `SpeechRecognizerServiceProtocol`. Swap is a single-file change once the API stabilises.
- **iOS 26 `SpeechDetector` VAD**: stubbed via `BargeInDetectorProtocol`.
- **Voice tab registration in `RootView`**: the spec lists `App/Sources/App/RootView.swift` as adjacent territory and the orchestrator handles tab wiring at merge time. `VoiceView` is a self-contained `View` ready to drop in.
- **Tests under `AppTests/Sources`**: spec said "build green," not "tests green," and the budget went into the production state machine and the integration protocol surface.

## Build status

`xcodebuild` against the iPhone 17 simulator: **succeeded**. No warnings or errors introduced by Lane 8 files. Pre-existing warnings in unrelated files are unchanged.
