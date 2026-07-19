# iOS playback qualification

Issue: #70

Ownership: Native by design

Durable policy owner today: Pod0 Rust listening slice (#58, #81, #82)

Native executor: Swift `AudioEngine` / AVFoundation through `CorePlaybackHost`

## Loss budget

- Continuous playback may lose at most 30 seconds after an ungraceful process death.
- First position, pause, seek, interruption, route loss, segment end, and
  natural end persist the latest observed playhead. Continuous observations
  commit at most every 30 seconds.
- Relaunch restores the stable episode identity and its latest durable position.

## Automated qualification matrix

| Scenario | Evidence | Result |
| --- | --- | --- |
| Streamed episode starts and requests a background download | `PlaybackStateAutoDownloadTests` | Automated |
| Downloaded episode avoids a duplicate download request | `PlaybackStateAutoDownloadTests` | Automated |
| Background/termination flush preserves episode and playhead | `AppTests`, `PlaybackResumeDurabilityTests` | Automated |
| Pause and explicit seek flush immediately | `PlaybackStateAudioCallbackTests`, `AppTests` | Automated |
| Interruption pauses, records a typed observation, and flushes | `PlaybackLifecycleQualificationTests` | Automated |
| Interruption resumes only with OS permission and the same episode | `PlaybackLifecycleQualificationTests` | Automated |
| Headphone/Bluetooth route loss pauses and clears auto-resume intent | `PlaybackLifecycleQualificationTests` | Automated policy; device check pending |
| Stale item-end callback cannot finish a replacement queue item | `PlaybackLifecycleQualificationTests` | Automated |
| Queue ordering and auto-play-next remain deterministic | `PlaybackQueueTests`, `PlaybackAutoPlayNextTests` | Automated |
| Shared queue identity, same-episode selection, and adjacent segment transitions remain deterministic | Rust `runtime_playback_recovery_tests`, `SharedPlaybackMappingTests` | Automated |
| Completion and mark-played do not resurrect a stale position | `EpisodePlayedStateTests`, `AppTests` | Automated |
| Pre-seek end callbacks and replaced-episode observations cannot overwrite newer shared state | Rust `runtime_playback_race_tests` | Automated |
| Sleep-timer pause travels through the same flush boundary | `PlaybackStateAudioCallbackTests`, `PlaybackSleepTimerLabelTests` | Automated |
| Shared queue/resume/rate survive facade relaunch while the session timer clears | Rust `restart_restores_queue_resume_rate_and_clears_session_timer`, `SharedPlaybackVerticalSliceTests` | Automated |
| Now Playing seek/pause uses the same persistence side effects | `PlaybackStateAudioCallbackTests` | Automated |
| Underlying offline playback failures become safe retry guidance | `PlaybackLifecycleQualificationTests` | Automated |
| Audio route/interruption with real wired and Bluetooth hardware | Physical-device checklist below | Pending hardware |

## Latest automated evidence

Validated on 2026-07-19 against an iPhone 17 Pro simulator running iOS 26.5:

- The complete `Podcastr` scheme passed 664 tests with zero failures or skips.
- The app built, installed, launched, and rendered onboarding without a crash or
  shared-store bootstrap error.
- The locked Rust workspace passed formatting, Clippy with warnings denied,
  all 62 unit tests, dependency/facade/schema policies, license/source checks,
  and the configured security audit.
- Generated Swift and Kotlin bindings matched facade metadata; the Kotlin/JNA
  runtime smoke passed.
- The core built for Apple device and simulator, Android API 23 ARM64, and
  Android API 23 x86_64.

No physical device was attached for this run, so wired/Bluetooth route evidence
remains explicitly open rather than being inferred from simulator coverage.

## Typed native boundary

`PlaybackAudioSessionObserver` converts AVAudioSession notifications into
`PlaybackAudioSessionEvent`. `CorePlaybackHost` reports the raw route and
interruption facts; the Rust playback policy decides pause, resume, checkpoint,
reload, queue advance, and completion. The host emits this bounded observation:

```text
PlaybackObservation {
  episode_id,
  host_state,
  position_ms,
  duration_ms,
  route,
  interruption,
  ended,
  observed_at
}
```

AVFoundation port objects, localized errors, URLs, and notification payloads do
not leave the native adapter. The player animates its playhead from AVPlayer
locally; the host stream is coalesced to at most one position observation per
second, and Rust writes no more than the first sample, each 30-second cap, and
semantic boundaries.

## Sleep-timer lifecycle

`Off`, bounded duration, and end-of-episode are platform-neutral Rust modes.
Duration and end-of-episode timers are session-scoped: Rust stores the active
mode while the process lives, asks the native host to arm or cancel the OS
timer, and clears the mode to `Off` when the facade reopens. Relaunch never
re-arms an expired or ambiguously interrupted timer. Queue, resume position,
rate, and playback preferences remain durable across the same restart.

## Physical-device checklist

Run before the iOS validation gate closes:

1. Start a streamed episode on the built-in speaker, background and foreground
   the app, then force-quit and relaunch. Confirm episode and position restore
   inside the loss budget.
2. Repeat from a downloaded episode with airplane mode enabled.
3. While playing through wired headphones, disconnect them. Confirm playback
   pauses, does not jump to the speaker, and resumes only after an explicit tap.
4. While playing through Bluetooth, disconnect and reconnect the route. Confirm
   the same pause/no-auto-resume behavior.
5. Trigger a phone/Siri interruption. Confirm begin pauses and flushes; end
   resumes only when iOS supplies `shouldResume` and the episode did not change.
6. Exercise lock-screen pause, seek, rate, and next/previous controls and confirm
   the in-app player, Now Playing surface, and durable position stay aligned.

Record device model, iOS version, route model, observed position delta, and any
system log evidence in the tracking issue. This hardware-only evidence is a
gate requirement, not a reason to duplicate playback policy in Swift.
