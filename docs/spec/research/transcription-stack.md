# Transcription Stack — Research Notes

> Research note for the Podcast Player. Audience: engineers + product. Goal: timestamped, speaker-diarized transcripts for every episode the user subscribes to, ideally cheaply, at high accuracy, with iOS-friendly plumbing.

## TL;DR Recommendation

1. Always check the publisher's `<podcast:transcript>` first; parse VTT / SRT / Podcasting 2.0 JSON into our internal model. This is free, fast, and growing in coverage.
2. When a transcript is absent, send the audio to **ElevenLabs Scribe v1** (the batch model) via async webhook. It is the best-quality option that ships diarization + word timestamps in one call, at $0.22 / hr — competitive with Deepgram Nova-3 and ~3.3× cheaper than Whisper / GPT-4o-transcribe.
3. Run the upload from a **background `URLSession`** kicked off when an episode is downloaded; finalize ingestion under **`BGTaskScheduler`** when the webhook resolves.
4. Persist into a single Swift `Codable` `Transcript` model that is lossless across VTT / SRT / JSON / Scribe word-list / Deepgram word-list, then chunk for embeddings via OpenRouter.
5. Apple `SpeechAnalyzer` (iOS 26) is a credible on-device fallback for free-tier users, but it has no diarization — keep cloud as the default for the diarized RAG corpus and use on-device only as an opt-in privacy mode.

## 1. ElevenLabs Scribe — The Default Cloud Path

Scribe ships in two SKUs: **Scribe v1 / v2 (batch)** and **Scribe v2 Realtime** (~150 ms latency streaming). For batch podcast ingestion, v1 / v2 is the right call.

**Capabilities**
- 90+ languages; auto language detection.
- Word-level timestamps; optional `character` granularity.
- Speaker diarization up to **32 speakers**; auto-detect or pass `num_speakers`.
- Audio event tags (laughter, footsteps, music) on by default.
- Optional surcharges: keyterm prompting (+$0.05/hr), entity detection (+$0.07/hr), speaker role detection (~+10%).

**Limits**
- File size: **up to 3 GB** (~10 hours of 192 kbps MP3).
- Async + webhook path is the documented way to handle long files; it returns immediately and POSTs the result when ready.
- Concurrency depends on plan tier; queueing adds ~50 ms above the limit.
- Pricing: **$0.22/hr Scribe v1/v2**, $0.39/hr Realtime. Creator plan ($22/mo) bundles 27 hrs; Pro ($99) bundles 100 hrs; Scale ($299) bundles 450 hrs.

**REST shape** (`POST https://api.elevenlabs.io/v1/speech-to-text`, `multipart/form-data`):

```
model_id: scribe_v2
file: <bytes>            # OR cloud_storage_url
diarize: true
num_speakers: null       # auto
timestamps_granularity: word
tag_audio_events: true
webhook: true
webhook_metadata: { "episode_id": "..." }
```

Response includes `words[]` with `text`, `start`, `end`, `type` (`word|spacing|audio_event`), `speaker_id`, `logprob`, plus optional `characters[]` and `audio_duration_secs`. (See [Create transcript reference](https://elevenlabs.io/docs/api-reference/speech-to-text/convert), [Async webhooks cookbook](https://elevenlabs.io/docs/cookbooks/speech-to-text/webhooks).)

**Latency for a 1-hour podcast**: typically a few minutes wall-clock for batch; with `cloud_storage_url` (R2 / S3) the upload step is offloaded entirely. Plan for **~3-6 min** end-to-end for one hour.

## 2. Publisher Transcript Discovery (Podcasting 2.0)

Every feed parser must look for `<podcast:transcript>` inside each `<item>` ([spec](https://podcasting2.org/docs/podcast-namespace/tags/transcript)). Required attributes: `url` and `type`. Accepted MIME types:

- `text/vtt` — preferred; `<v Speaker>` tags carry diarization.
- `application/json` — Podcasting 2.0 JSON: `{ "version": "1.0.0", "segments": [{ "speaker", "startTime", "endTime", "body" }] }`.
- `application/x-subrip` (SRT) — speaker often inlined as `Sarah: ...`.
- `text/html` / `text/plain` — last resort, no timestamps.

Parsing strategy:
- VTT → split on cue boundaries; speaker = `<v>` content; words have cue-level granularity.
- JSON → 1:1 map to our `Segment` model; word-level lossless.
- SRT → cue-level; regex `^([A-Z][\w .'-]{1,30}):\s` to extract speaker prefix.
- HTML / plain → fall through to Scribe (no usable timestamps).

Multiple `<podcast:transcript>` tags can coexist; pick by `language`, then by quality (JSON > VTT > SRT > HTML).

## 3. Alternatives Considered

| Provider | Model | $/hr (batch) | Diarization | Word TS | Lang | Notes |
|---|---|---|---|---|---|---|
| **ElevenLabs** | Scribe v1/v2 | $0.22 | up to 32 spk | yes | 90+ | Cleanest API; great audio-event tagging |
| Deepgram | Nova-3 | $0.26 ($0.0043/min + $0.12/hr diarize) | yes | yes | 30+ | Best for streaming; precise timestamps |
| AssemblyAI | Universal | $0.17 ($0.15 + $0.02 diarize) | yes | yes | 90+ | Cheapest at base; rich audio intelligence add-ons |
| OpenAI | gpt-4o-transcribe | $0.36 | only `-diarize` SKU (~2.5×) | partial | many | Best WER (~4.1%) but pricey for long-form |
| OpenAI / OpenRouter | whisper-large-v3 | ~$0.36 (or self-host) | none built-in | yes | many | Diarization needs WhisperX or pyannote sidecar |
| Apple | SpeechAnalyzer (iOS 26) | $0 | none | yes | 50+ | On-device only; no length cap; great fallback |
| Apple | SFSpeechRecognizer | $0 | none | partial | many | Legacy; ~1 min reliable cap; do not use |
| ggml | whisper.cpp on-device | $0 (battery) | none | yes | many | Q6_K + CoreML viable; 1 hr ≈ heavy thermal |

**Why Scribe over Deepgram/AssemblyAI for *this* product**: AssemblyAI is technically cheaper, but Scribe's diarization quality on conversational long-form podcasts plus integrated audio-event tagging map best to the wiki + RAG product surface. Deepgram is the natural switch if we ever need streaming. Keeping a `TranscriptionProvider` protocol in Swift is mandatory.

Sources: [Deepgram pricing 2026](https://brasstranscripts.com/blog/deepgram-pricing-per-minute-2025-real-time-vs-batch), [AssemblyAI pricing 2026](https://costbench.com/software/ai-transcription-apis/assemblyai/), [OpenAI transcription pricing](https://costgoat.com/pricing/openai-transcription), [iOS 26 SpeechAnalyzer guide](https://antongubarenko.substack.com/p/ios-26-speechanalyzer-guide), [whisper.cpp local transcription](https://jimmysong.io/ai/whisper-cpp/).

## 4. Hybrid Strategy for 10-50 hrs/user/week

Power user = 50 hrs/wk = **~217 hrs/month**. We optimize for *unit economics*, *user wait time*, and *battery*.

- **On-device for everything** is a non-starter: 217 hrs/mo of whisper.cpp (even Q6_K + CoreML) means hours of sustained ANE / GPU load — thermal throttle, battery drain, no diarization, slow time-to-first-query.
- **Cloud for everything** is fine financially (see §8) but requires a billing model. Three viable models, in order of operational sanity:
  1. **Us-pays + tier cap** (default tier covers ~20 hrs/wk; power tier covers 100 hrs/wk). Predictable margin, simple UX.
  2. **User-BYOK** (user pastes their own ElevenLabs / OpenRouter / Deepgram key). Zero infra cost, power-user friendly, awful onboarding.
  3. **Creator-pays** is a fantasy for an indie podcast app; ignore.
- **Recommended**: us-pays with a default tier + a hard monthly cap, plus an opt-in BYOK escape hatch in advanced settings, plus an opt-in on-device mode (SpeechAnalyzer) for privacy-conscious users that disables diarization.

Trigger policy:
- Episode auto-downloads (Wi-Fi) → if `<podcast:transcript>` exists → parse + done.
- Else → enqueue Scribe job using `cloud_storage_url` against our R2 bucket, `webhook=true`.
- On webhook → store transcript, chunk + embed, mark episode "ready for RAG".
- User taps an unsubscribed episode they want now → priority queue (low concurrency reserved).

## 5. End-to-End Pipeline

```
RSS poll
  └─ new episode item
       ├─ has <podcast:transcript>? ── yes ──► fetch + parse (VTT/JSON/SRT) ──┐
       │                                                                      │
       └─ no ──► download audio (Wi-Fi, BG URLSession)                         │
                  └─ upload to R2 (BG URLSession)                              │
                       └─ POST /v1/speech-to-text (model=scribe_v2,            │
                          diarize=true, webhook=true, cloud_storage_url=…)     │
                              └─ webhook → server-side normalize ──────────────┤
                                                                               ▼
                                                                Internal Transcript
                                                                       │
                                              ┌────────────────────────┼────────────────────────┐
                                              ▼                        ▼                        ▼
                                     Chunk (≈500 tokens,        Generate per-episode      Index for full-text
                                      30s overlap, speaker      summary + wiki update     search (FTS5)
                                      boundaries respected)
                                              │
                                              ▼
                                  Embed via OpenRouter (e.g.
                                   text-embedding-3-large)
                                              │
                                              ▼
                                  Local sqlite-vss / Core ML
                                       vector index
                                              │
                                              ▼
                                       Agent RAG ready
```

## 6. iOS Implementation Notes

- **Audio prep**: podcasts are 64-128 kbps MP3. Scribe takes MP3 directly — do not transcode. For SpeechAnalyzer fallback, downsample via `AVAudioConverter` to 16 kHz mono PCM.
- **Background uploads**: shared `URLSession` with `.background(withIdentifier:)`, `isDiscretionary = true`, `sessionSendsLaunchEvents = true`. Body must be a file URL. System relaunches the app to deliver completion. ([Background tasks guide 2026](https://medium.com/@chandra.welim/background-tasks-in-ios-the-complete-guide-2a46b793084b)).
- **`BGTaskScheduler`**: `BGProcessingTaskRequest` (`requiresNetworkConnectivity = true`) for embed/index work after the webhook returns. `BGAppRefreshTaskRequest` as a poll-based reconciliation fallback for missed webhooks.
- **Push**: Scribe's webhook hits *our* server, which sends silent APNs (`content-available: 1`) to wake the device and pull the transcript JSON.
- **Clip extraction**: `AVAssetExportSession` with `AVAssetExportPresetAppleM4A`, `timeRange` from word timestamps ± 1 s padding.
- **Storage**: gzipped JSON per episode in the App Group container (~50-200 KB / hour). Vector store as sqlite-vss next to it.

## 7. Internal Transcript Data Model

Goal: lossless across all source formats; cheap to render; cheap to chunk.

```swift
struct Transcript: Codable, Identifiable, Hashable {
    let id: String                   // episode_id
    let language: String             // BCP-47, e.g. "en-US"
    let source: Source               // publisher | scribe | speechAnalyzer | …
    let model: String?               // e.g. "scribe_v2"
    let createdAt: Date
    let duration: TimeInterval
    let speakers: [Speaker]
    let segments: [Segment]          // sorted by startTime

    enum Source: String, Codable { case publisherJSON, publisherVTT, publisherSRT, publisherHTML, scribe, deepgram, assemblyAI, openAI, speechAnalyzer, whisperLocal }
}

struct Speaker: Codable, Hashable {
    let id: String                   // "spk_0" or imported tag like "Tim Ferriss"
    var displayName: String?         // resolved later via host/guest detection
    var role: Role?                  // host | guest | caller | unknown
    enum Role: String, Codable { case host, guest, caller, unknown }
}

struct Segment: Codable, Hashable, Identifiable {
    let id: UUID
    let speakerId: String?
    let start: TimeInterval
    let end: TimeInterval
    let text: String
    let words: [Word]?               // optional; populated when source has word TS
    let confidence: Float?           // 0…1
    let isAudioEvent: Bool           // true for [laughter], [music] tags
}

struct Word: Codable, Hashable {
    let text: String
    let start: TimeInterval
    let end: TimeInterval
    let speakerId: String?
    let confidence: Float?
}
```

Adapters: `Transcript.fromScribe(_:)`, `Transcript.fromPodcastingJSON(_:)`, `Transcript.fromVTT(_:)`, `Transcript.fromSRT(_:)`, `Transcript.fromSpeechAnalyzer(_:)`. Each adapter MUST set `source` and `model`, MUST sort segments, and MUST stable-id speakers across calls.

## 8. Cost Model — Power User (50 hrs/wk)

- **Audio per month**: 50 × 4.33 ≈ **217 hrs**.
- **Publisher transcript hit rate (estimate)**: ~25% of mainstream English podcasts publish `<podcast:transcript>` today and growing. Treat as 25% free.
- **Cloud-transcribed**: 217 × 0.75 ≈ **163 hrs/mo**.
- **Scribe v2 batch**: 163 × $0.22 = **$35.86 / power user / month**.
  - With diarization included; +$0.07/hr if we enable entity detection → +$11.41.
- **Embeddings (OpenRouter `text-embedding-3-large`)**: 217 hrs × ~150 wpm × ~1.3 tok/word = ~2.5 M tokens at $0.13/M = **~$0.33/mo**. Negligible.
- **Storage / R2**: 217 hrs × ~50 MB MP3 = ~10 GB/mo at $0.015/GB = **~$0.15/mo**. Negligible.
- **Total ceiling per power user**: **~$36-50/mo**.

For a typical user (10 hrs/wk, 25% publisher hit): 32.5 hrs cloud × $0.22 ≈ **$7.15/mo**. A $14.99/mo Pro tier covers it with margin; a $4.99/mo basic tier needs a 5-hr/wk cap or BYOK.

Switching to AssemblyAI base + diarize ($0.17/hr) cuts the power-user ceiling to ~$28/mo; switching to Whisper / GPT-4o-transcribe pushes it to ~$59/mo before diarization sidecars.

## 9. Risks and Open Questions

- Scribe diarization on noisy podcasts (call-ins, overlapping speech) — pilot ~20 episodes before locking it in as default.
- Webhook reliability over months — always have a poll-based reconciliation path; never trust a single webhook.
- Speaker name resolution — Scribe returns `spk_0..N`. Use show notes + chapter markers + LLM pass to map IDs to real names.
- Privacy — expose an on-device-only toggle using iOS 26 SpeechAnalyzer (no diarization) with a clear UX explanation.
- iOS 26 SpeechAnalyzer maturity — benchmark before promoting it from fallback to default.

## Sources

- [ElevenLabs Speech-to-Text capabilities](https://elevenlabs.io/docs/capabilities/speech-to-text)
- [ElevenLabs Create Transcript API](https://elevenlabs.io/docs/api-reference/speech-to-text/convert)
- [ElevenLabs Async Webhooks cookbook](https://elevenlabs.io/docs/cookbooks/speech-to-text/webhooks)
- [ElevenLabs API pricing](https://elevenlabs.io/pricing/api)
- [ElevenLabs Scribe v2 Realtime](https://elevenlabs.io/realtime-speech-to-text)
- [Podcasting 2.0 transcript tag spec](https://podcasting2.org/docs/podcast-namespace/tags/transcript)
- [Podcastindex transcripts examples](https://github.com/Podcastindex-org/podcast-namespace/blob/main/docs/examples/transcripts/transcripts.md)
- [Deepgram Nova-3 introduction](https://deepgram.com/learn/introducing-nova-3-speech-to-text-api)
- [Deepgram diarization docs](https://developers.deepgram.com/docs/diarization)
- [Deepgram pricing 2026](https://brasstranscripts.com/blog/deepgram-pricing-per-minute-2025-real-time-vs-batch)
- [AssemblyAI pricing 2026](https://costbench.com/software/ai-transcription-apis/assemblyai/)
- [OpenAI transcription pricing 2026](https://costgoat.com/pricing/openai-transcription)
- [GPT-4o-Transcribe vs Whisper 2026](https://tokenmix.ai/blog/gpt-4o-transcribe-vs-whisper-review-2026)
- [iOS 26 SpeechAnalyzer guide](https://antongubarenko.substack.com/p/ios-26-speechanalyzer-guide)
- [iOS speech recognition 2026 playbook](https://www.forasoft.com/blog/article/speech-recognition-with-neural-networks-on-ios-1621)
- [whisper.cpp on iOS / edge](https://jimmysong.io/ai/whisper-cpp/)
- [BGTaskScheduler reference](https://developer.apple.com/documentation/backgroundtasks/bgtaskscheduler)
- [iOS 18+ background URLSession guide](https://medium.com/@melissazm/ios-18-background-survival-guide-part-3-unstoppable-networking-with-background-urlsession-f9c8f01f665b)

---

File: `/Users/pablofernandez/Work/podcast-player/.claude/research/transcription-stack.md`
