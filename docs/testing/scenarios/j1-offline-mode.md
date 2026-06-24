# Scenario J1: Offline mode behavior

## Goal
Validate the app degrades gracefully offline: downloaded episodes still play, and
network-dependent surfaces show errors/empty states rather than crashing.

## Prerequisites
- App past onboarding with ≥1 DOWNLOADED episode (C3) and a transcribed episode.
- Ability to toggle the simulator/host network off (disable host Wi-Fi or use a
  network condition profile).

## Steps
1. While online, download an episode and confirm it's "Downloaded". *Screenshot.*
2. Turn the network OFF. *Screenshot.*
3. Play the downloaded episode. **Expected:** Playback works from local file. *Screenshot.*
4. Open the Search tab and search. **Expected:** A graceful error/empty state, not a
   crash or infinite spinner. *Screenshot.*
5. Try to subscribe to a new RSS URL. **Expected:** An error banner ("couldn't fetch
   feed" or similar). *Screenshot.*
6. Open the agent chat and send a message. **Expected:** A clear error bubble
   (`agent.error`) — the LLM is unreachable offline. *Screenshot.*
7. Turn the network back ON. **Expected:** Previously failed surfaces recover on retry.

## Acceptance Criteria
- Downloaded episodes play fully offline.
- Network-dependent actions (search, subscribe, agent) fail with clear errors, no
  crash, no permanent spinner.
- Reconnecting restores functionality on retry.

## Known Issues / Watch Points
- MEMORY (android_reactive_path_and_datadir) shows headless offline scenarios are a
  CI gate; iOS offline should be similarly robust.
- Streaming (non-downloaded) playback should fail gracefully offline, not hang.

## Notes
