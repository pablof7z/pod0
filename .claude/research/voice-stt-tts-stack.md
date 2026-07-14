# Voice Stack: STT, TTS, Barge-in, and Triggers for the Conversational Podcast Agent

> Research note for the iOS podcast player. Marquee feature: a TLDR briefing is playing in the user's ears, the user starts talking, the briefing ducks/pauses, the agent answers, the briefing resumes — all under ~1 s perceived latency.

The project already ships an `SFSpeechRecognizer`-based `VoiceItemService` and an `ElevenLabsTTSPreviewService`. Both are usable starting points, but neither is configured for full‑duplex, barge‑in conversational use. This document recommends the target architecture.

---

## 1. STT — Speech-to-Text

| Engine | Mode | Latency | Privacy | When to use |
|---|---|---|---|---|
| `SpeechAnalyzer` + `SpeechTranscriber` (iOS 26) | On‑device streaming | sub‑second partials, ~2× faster than Whisper Large V3 Turbo | 100% local | **Default for iOS 26+** |
| `SFSpeechRecognizer` (on‑device flag) | On‑device streaming | ~300–600 ms partials | 100% local | iOS 17–25 fallback |
| WhisperKit (Argmax) | On‑device streaming | <200 ms first‑word, <100 ms attainable on A17/A18 | Local | iOS 17–25 fallback when accuracy/multilingual matters more than disk size (~600 MB models) |
| ElevenLabs Scribe v2 Realtime | Cloud WebSocket | ~150 ms partials, committed segments slower | Cloud | Long‑form podcast transcript ingest (already in spec for transcripts) |
| Deepgram Nova‑3 | Cloud WebSocket | ~300 ms streaming WER ~6.8% | Cloud | Cheap fallback for live mode if local fails |
| OpenAI `gpt-4o-transcribe` | Cloud | ~320 ms, 2.46% WER | Cloud | Highest accuracy fallback, slowest of the cloud trio |

**Recommendation — live conversational STT:**

- **iOS 26+ → `SpeechAnalyzer`** with `SpeechTranscriber` and `SpeechDetector` modules, `volatileResults: true` for streaming partials. Apple's new model is ~2× Whisper Turbo, fully on‑device, engineered for long‑form streaming.
- **iOS 17–25 fallback → `SFSpeechRecognizer`** with `requiresOnDeviceRecognition = true` and `addsPunctuation = true`. The existing `VoiceItemService` already uses this path; harden for full‑duplex.
- **Skip WhisperKit by default** — adding 500–700 MB of weights for what Apple now ships natively isn't worth it. Keep behind a "Pro transcription" toggle if Apple's accuracy is inadequate for a target language.
- **Cloud STT (ElevenLabs Scribe) is for offline transcript ingest** of publisher episodes — not the live loop. Cloud STT in conversation breaks airplane mode and adds round‑trip latency.

## 2. TTS — Text-to-Speech

Two distinct workloads, two different models.

### 2a. Briefings (long-form, scripted)

5–15 minute narrations generated ahead of time. Quality > latency.

- **ElevenLabs Multilingual v2** (or `eleven_v3` GA) via the streaming endpoint, persisted as MP3/Opus, played through `AVPlayer` — Now Playing / lock screen / CarPlay come for free. Cache per `briefing_id` alongside downloaded episodes; the briefing is a first‑class episode.

### 2b. Conversational replies (short, fast)

Latency dominates — every 100 ms of silence after a barge‑in feels broken.

- **ElevenLabs Flash v2.5** via WebSocket TTS streaming. ~75 ms time‑to‑first‑byte, `flush:true` end‑of‑turn semantics, pipe PCM/Opus straight into `AVAudioPlayerNode`.
- **`AVSpeechSynthesizer` fallback** with a Premium / Personal Voice — free, on‑device, VoiceOver‑integrated. Becomes the privacy/offline mode.
- **OpenAI `gpt-4o-mini-tts`** is the safety‑net backup if we want a single vendor relationship via OpenRouter.

Skip Cartesia and PlayHT — no meaningful win over Flash v2.5 here.

## 3. AVAudioSession — the briefing-ducks-then-resumes flow

The "magical case" is:

```
briefing playing  →  user starts speaking  →  briefing ducks  →  STT captures  →
agent thinks/replies (TTS)  →  briefing resumes
```

This requires a single `AVAudioSession` reconfigured across two states. Do **not** spawn a second `AVAudioSession` — there is only one per process.

### State A — Briefing playback only

```swift
let session = AVAudioSession.sharedInstance()
try session.setCategory(.playback, mode: .spokenAudio, options: [])
try session.setActive(true)
```

`.spokenAudio` ducks when system spoken content (Siri, navigation) plays and resumes — exactly what a podcast app wants.

### State B — Conversation active (briefing audible underneath, mic open)

```swift
try session.setCategory(
    .playAndRecord,
    mode: .voiceChat,
    options: [.duckOthers, .defaultToSpeaker, .allowBluetoothHFP, .allowBluetoothA2DP]
)
try session.setPrefersEchoCancelledInput(true)   // iOS 18+
try session.setActive(true, options: .notifyOthersOnDeactivation)
```

Key choices:

- **`.playAndRecord` + `.voiceChat`** enables Apple's built‑in AEC, AGC, and noise suppression. AirPods get clean full‑duplex; iPhone speaker is good but not perfect, so we duck the briefing aggressively.
- **`setPrefersEchoCancelledInput(true)`** (iOS 18+) is the explicit knob; gate on `isEchoCancelledInputAvailable`.
- **`.duckOthers`** drops the briefing `AVPlayer` to ~‑20 dB during the conversational turn — the briefing keeps playing, so resume is seamless. For full pause, call `pause()` on the briefing player and omit `.duckOthers`.
- **`.voicePrompt`** is for one‑shot prompts ("Turn left in 200 m"), not full‑duplex — use `.voiceChat`.
- **Never `.mixWithOthers`** — it disables AEC.

Switching A → B mid‑playback re‑negotiates the route (50–150 ms glitch). Pre‑warm by reactivating with the new category on the wake gesture, before VAD confirms speech. After the turn, return to State A and ramp the briefing volume back up over ~250 ms via `AVAudioPlayerNode.volume`.

## 4. Barge-in / VAD

The dangerous edge case: the agent's *own* TTS leaking through the speaker triggers a barge‑in loop.

1. **AEC always on** (the session config above) removes ~95% of speaker leak.
2. **Silero VAD** via ONNX (`RealTimeCutVADLibrary`) at threshold ~0.5 with a 200–300 ms minimum voiced duration. Silero hits 87% TPR vs WebRTC's 50% at 5% FPR — the right choice against speaker bleed.
3. **Cross-correlate against a ~500 ms ring buffer of TTS output** before feeding VAD. Combined with AEC this gets near‑zero false positives.
4. **iOS 26 → use Apple's `SpeechDetector` module** instead — it ships alongside `SpeechTranscriber` and is co‑optimized for the same pipeline.
5. `voiceIsolation` is a system FaceTime/calls feature, not an API hook — ignore it.

Require ~250 ms of voiced audio to fire `userBargedIn`. Slower than Alexa (~100 ms) but eliminates misfires on the iPhone speaker; AirPods users won't notice.

## 5. AirPods + Action Button + Lock-screen triggers

| Trigger | Mechanism | Native? |
|---|---|---|
| AirPods Pro 2 stem squeeze | Cannot be hooked directly. **Workaround:** an `AppShortcut` that the user assigns via Settings → AirPods → "Press and Hold AirPods" → Shortcut | Requires user setup in Shortcuts |
| iPhone 15 Pro+ Action Button | `AppShortcut` exposed via `AppIntent`, user picks it in Settings → Action Button → Shortcut | User assigns once |
| Lock Screen control | iOS 18 Control Center / Lock Screen control via `ControlWidget` calling an `AppIntent` | Native |
| Siri "Hey Siri, ask Podcasts to…" | `AppShortcut` with phrases | Native |
| In‑app push‑to‑talk button | Just a SwiftUI button | Native |

Implementation:

```swift
struct StartVoiceModeIntent: AppIntent {
    static var title: LocalizedStringResource = "Talk to my podcasts"
    static var openAppWhenRun: Bool = true
    func perform() async throws -> some IntentResult {
        await VoiceMode.shared.start(trigger: .appIntent)
        return .result()
    }
}

struct PodcastShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: StartVoiceModeIntent(),
            phrases: ["Talk to my podcasts in \(.applicationName)",
                      "Ask \(.applicationName) about my podcasts"],
            shortTitle: "Talk to podcasts",
            systemImageName: "waveform.circle.fill"
        )
    }
}
```

A single `AppShortcut` covers all five triggers — Action Button, AirPods squeeze, Lock Screen control, Siri, and Spotlight all dispatch to the same intent. Document the AirPods setup once in onboarding.

## 6. CarPlay

The app uses the **Audio** CarPlay entitlement (depth‑5 templates: NowPlaying, List, TabBar). The right voice hook is *not* a custom CarPlay UI:

- The same `StartVoiceModeIntent` works while CarPlay is connected — our audio session config already handles the Bluetooth/HFP route.
- Add a `CPListItem` accessory on Now Playing that fires the intent. Tap → voice mode.
- App Shortcuts make "Hey Siri, ask Podcasts what was that thing about keto" hands‑free; Siri intercepts before our app sees the mic.

The richer **Voice‑based conversational apps** CarPlay category (depth‑3, voice‑primary modality) is available if we want a fully voice‑first CarPlay UI — defer to v2.

## 7. Privacy + permissions

- Mic prompt fires on **first voice‑mode invocation**, never at launch, with a one‑screen explainer ("We listen only while the wave is glowing. Audio stays on your iPhone.").
- `SFSpeechRecognizer.requestAuthorization` is required for iOS ≤25; iOS 26 `SpeechAnalyzer` on‑device path needs no separate prompt.
- Default to on‑device STT/TTS. One Settings toggle — "Use cloud voices for higher quality" — opts into ElevenLabs.
- Data retention copy: voice queries processed on‑device; only final text persists to chat history; audio buffers are never written to disk.
- Settings → Privacy ships a "Delete voice history" action that wipes chat history and cached briefing audio.

## 8. Latency budget

Target: **user finishes speaking → first audio frame from agent < 800 ms**. Acceptable: < 2 s. Hard ceiling: 3 s before the user assumes the app crashed.

| Stage | Budget (ms) | Notes |
|---|---|---|
| End‑of‑speech detection (VAD) | 250 | minimum voiced + silence hold |
| Final STT result commit | 50–150 | partials already streaming during speech |
| LLM first token (tools, RAG) | 300–700 | OpenRouter via existing `AgentOpenRouterClient`, prompt‑cached system+tools |
| LLM → TTS sentence boundary | 0 (overlapped) | stream first sentence as soon as it lands |
| TTS time‑to‑first‑byte | 75–150 | ElevenLabs Flash v2.5 streaming WS |
| Local network jitter | 50–150 | LTE/5G real‑world |
| **Total to first audio frame** | **~700–1,300** | feasible inside the 800 ms aspiration on Wi‑Fi |

To earn this budget: stream STT partials into the LLM speculatively (commit on final), pipe the LLM response to TTS per sentence, pre‑warm the TTS WebSocket on VAD start, and cache the OpenRouter system prompt + tool schema (~200–400 ms saved per repeat turn).

## 9. Implementation sketch

A new `AudioConversationManager` orchestrates state. Glue it to the existing `AgentChatSession` so the same agent loop powers both text and voice.

```swift
@MainActor
@Observable
final class AudioConversationManager {

    enum State: Equatable {
        case idle
        case listening
        case thinking
        case speaking
        case duckedWhileBriefing       // briefing playing, mic open, AEC engaged
    }

    private(set) var state: State = .idle

    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()                  // agent TTS playback
    private let session = AVAudioSession.sharedInstance()
    private let agent: AgentChatSession
    private let briefing: BriefingPlayer                      // wraps AVPlayer
    private var stt: any StreamingTranscriber                 // Apple SpeechAnalyzer or SFSpeechRecognizer
    private var tts: ElevenLabsStreamingTTS
    private var vad: SileroVAD?

    init(agent: AgentChatSession, briefing: BriefingPlayer) {
        self.agent = agent
        self.briefing = briefing
        self.tts = ElevenLabsStreamingTTS(model: .flashV25)
        self.stt = SpeechStack.makeTranscriber()              // picks Apple vs SFSpeech
        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: nil)
    }

    func start(trigger: VoiceMode.Trigger) async throws {
        try configureForConversation()
        if briefing.isPlaying {
            state = .duckedWhileBriefing
            briefing.duck(by: 18)                              // ~‑18 dB
        } else {
            state = .listening
        }
        try await stt.start { [weak self] partial, isFinal in
            await self?.handle(partial: partial, final: isFinal)
        }
        startVADGate()
    }

    private func configureForConversation() throws {
        try session.setCategory(
            .playAndRecord, mode: .voiceChat,
            options: [.duckOthers, .defaultToSpeaker,
                      .allowBluetoothHFP, .allowBluetoothA2DP]
        )
        if session.isEchoCancelledInputAvailable {
            try session.setPrefersEchoCancelledInput(true)
        }
        try session.setActive(true, options: .notifyOthersOnDeactivation)
        if !engine.isRunning { try engine.start() }
    }

    private func handle(partial: String, final: Bool) async {
        guard final else { return }
        state = .thinking
        // Reuse the existing agent loop; it already streams tokens + tools.
        await agent.send(partial, source: .voice) { [weak self] sentence in
            await self?.speak(sentence)
        }
        await finishAgentTurn()
    }

    private func speak(_ sentence: String) async {
        state = .speaking
        for await pcm in tts.stream(sentence) {
            player.scheduleBuffer(pcm, completionHandler: nil)
            if !player.isPlaying { player.play() }
        }
    }

    private func finishAgentTurn() async {
        state = briefing.isPlaying ? .duckedWhileBriefing : .idle
        try? configureForBriefingOnly()
        briefing.unduck(over: 0.25)
    }

    private func configureForBriefingOnly() throws {
        try session.setCategory(.playback, mode: .spokenAudio, options: [])
        try session.setActive(true)
    }
}
```

Hooks to `AgentChatSession`: add a `source: .text | .voice` parameter (voice turns get a "reply in 1–2 sentences" prompt addendum); add a per‑sentence callback so TTS can stream while the LLM is still generating; voice tool calls dispatch through the existing `AgentTools` unchanged, but player‑mutating tools (`play_episode_at`) pause rather than duck the briefing.

`BriefingPlayer.duck/unduck` are thin `AVPlayer.volume` ramps driven by `CADisplayLink`.

## Recommendation summary

- **Live STT:** `SpeechAnalyzer` (iOS 26+) → `SFSpeechRecognizer` on‑device fallback. WhisperKit only behind a Pro toggle.
- **Briefing TTS:** ElevenLabs Multilingual v2 (or v3), pre‑rendered, cached, played via `AVPlayer`.
- **Conversational TTS:** ElevenLabs Flash v2.5 WebSocket streaming, with `AVSpeechSynthesizer` as the offline fallback.
- **Audio session:** `.playAndRecord` + `.voiceChat` + `setPrefersEchoCancelledInput(true)` + `.duckOthers`.
- **Barge‑in:** Apple `SpeechDetector` (iOS 26) or Silero VAD via ONNX. Cross‑correlate against TTS output ring buffer.
- **Triggers:** one `AppIntent` + `AppShortcut` covers Action Button, AirPods squeeze, Lock Screen, Siri, Spotlight, and CarPlay.
- **Latency target:** 800 ms aspiration, 2 s acceptable. Achievable with on‑device STT, per‑sentence TTS streaming, and cached prompts.

---

Sources:
- [WWDC25 — Bring advanced speech-to-text to your app with SpeechAnalyzer](https://developer.apple.com/videos/play/wwdc2025/277/?time=26)
- [Apple Developer — Bringing advanced speech-to-text capabilities to your app](https://developer.apple.com/documentation/Speech/bringing-advanced-speech-to-text-capabilities-to-your-app)
- [Apple's New Transcription APIs Blow Past Whisper in Speed Tests — MacRumors](https://www.macrumors.com/2025/06/18/apple-transcription-api-faster-than-whisper/)
- [WhisperKit: On-device Real-time ASR with Billion-Scale Transformers (arXiv 2507.10860)](https://arxiv.org/html/2507.10860v1)
- [argmaxinc/WhisperKit GitHub](https://github.com/argmaxinc/WhisperKit)
- [ElevenLabs Realtime Speech to Text (Scribe v2)](https://elevenlabs.io/realtime-speech-to-text)
- [ElevenLabs Latency Optimization](https://elevenlabs.io/docs/eleven-api/guides/how-to/best-practices/latency-optimization)
- [ElevenLabs Models (Flash v2.5, Multilingual v2)](https://elevenlabs.io/docs/overview/models)
- [Deepgram vs OpenAI vs Google STT comparison](https://deepgram.com/learn/deepgram-vs-openai-vs-google-stt-accuracy-latency-price-compared)
- [Apple Developer — setPrefersEchoCancelledInput(_:)](https://developer.apple.com/documentation/avfaudio/avaudiosession/setprefersechocancelledinput(_:))
- [Apple Developer — AVAudioSession.CategoryOptions.duckOthers](https://developer.apple.com/documentation/avfaudio/avaudiosession/categoryoptions/1616618-duckothers)
- [WWDC25 — Get to know App Intents](https://developer.apple.com/videos/play/wwdc2025/244/)
- [Apple Developer — App Shortcuts](https://developer.apple.com/documentation/appintents/app-shortcuts)
- [CarPlay Developer Guide (February 2026)](https://developer.apple.com/download/files/CarPlay-Developer-Guide.pdf)
- [Picovoice — Best Voice Activity Detection 2026 (Cobra vs Silero vs WebRTC)](https://picovoice.ai/blog/best-voice-activity-detection-vad/)
- [RealTimeCutVADLibrary (Silero + WebRTC APM for iOS)](https://github.com/helloooideeeeea/RealTimeCutVADLibrary)
- [iOS Speech Recognition in 2026 — forasoft](https://www.forasoft.com/blog/article/speech-recognition-with-neural-networks-on-ios-1621)

File path: `/Users/pablofernandez/Work/podcast-player/.claude/research/voice-stt-tts-stack.md`
