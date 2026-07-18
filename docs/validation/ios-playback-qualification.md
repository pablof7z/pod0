# iOS playback qualification

Issue: #70

Ownership: Native by design

Durable policy owner today: temporary Swift `PlaybackState` / `AppStateStore`

Target durable policy owner: Pod0 Rust listening slice (#58, #81)

## Loss budget

- Continuous playback may lose at most 30 seconds after an ungraceful process death.
- Pause, seek, interruption, route loss, background, and termination boundaries
  persist the latest observed playhead and request an ordered storage flush.
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
| Completion and mark-played do not resurrect a stale position | `EpisodePlayedStateTests`, `AppTests` | Automated |
| Sleep-timer pause travels through the same flush boundary | `PlaybackStateAudioCallbackTests`, `PlaybackSleepTimerLabelTests` | Automated |
| Now Playing seek/pause uses the same persistence side effects | `PlaybackStateAudioCallbackTests` | Automated |
| Underlying offline playback failures become safe retry guidance | `PlaybackLifecycleQualificationTests` | Automated |
| Audio route/interruption with real wired and Bluetooth hardware | Physical-device checklist below | Pending hardware |

## Typed native boundary

`PlaybackAudioSessionObserver` converts AVAudioSession notifications into
`PlaybackAudioSessionEvent`. `PlaybackSessionPolicy` deterministically reduces
those inputs to `pauseAndPersist`, `resume`, or `none`. After each boundary,
the host emits this bounded projection:

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
not leave the native adapter. A future Rust playback policy can consume the
same stable observation without importing Apple concepts.

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
