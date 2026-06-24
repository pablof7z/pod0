# Scenario D6: Now Playing / lock screen / Control Center

## Goal
Validate that the system Now Playing info and remote transport controls
(Control Center / lock screen) reflect and control the current episode.

## Prerequisites
- App past onboarding, an episode playing.

## Steps
1. Start playback in-app. *Screenshot.*
2. Open Control Center on the simulator (swipe from the top-right). **Expected:**
   Now Playing tile shows the episode title, show, and artwork; transport controls
   present. *Screenshot.*
3. Tap pause in Control Center. **Expected:** In-app playback pauses; the in-app
   play/pause reflects paused. *Screenshot.*
4. Tap play in Control Center. **Expected:** Resumes; in-app reflects playing.
5. Use the skip controls in Control Center. **Expected:** Position jumps by the
   configured interval (same as in-app skip). *Screenshot.*
6. Lock the simulator (if supported) and verify lock-screen Now Playing artwork +
   controls. *Screenshot.*

## Acceptance Criteria
- Now Playing shows correct title, show, and artwork.
- Remote play/pause/skip control in-app playback bidirectionally.
- Lock-screen skip interval matches the in-app configured interval.

## Known Issues / Watch Points
- MEMORY/BACKLOG: Now Playing artwork must be built off-main (MediaPlayer renders
  off-main). A crash/hang when the artwork loads is a known historical issue.
- Simulator Control Center Now Playing can be finicky — if it doesn't populate,
  retry after a few seconds of playback; mark BLOCKED if the sim never shows it.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24 17:25 UTC**

Prerequisites not met and system-level UI inaccessible:
- App does not have any episodes available (subscriptions tab empty, no test data loaded)
- Cannot test Control Center integration: xcodebuildmcp ui-automation tools only support app-internal UI interactions; Control Center is a system-level UI element accessible via system gestures outside the app sandbox
- Cannot test lock screen: similarly requires access to system UI, not app UI
- The scenario requires testing remote transport controls (play/pause/skip) via Control Center, which is architectural testing that requires either:
  1. Manual testing on a real device or simulator with manual gesture input, OR
  2. App-level integration tests for the AVAudioSession/MediaPlayer APIs that surface Now Playing info to the system
  3. Setup with pre-populated episode data in the app's library

Recommendation: This scenario requires either manual simulator testing or architectural tests of the MediaPlayer integration layer, not app UI automation testing.
