# Scenario D10: Ad / pre-roll skip

## Goal
Validate the pre-roll skip button (manual) and the "Auto-skip ads" setting
(automatic seek past detected ad reads).

## Prerequisites
- App past onboarding with an episode that has a detected ad/pre-roll segment.
  (Ad detection is LLM-driven via `podcast.ads`; a seeded fixture or a real episode
  with sponsor reads is needed.)

## Steps
1. Start an episode that begins with a pre-roll ad. **Expected:** Near the ad, a
   "Skip Ns ad" button (forward.end.fill, label "Skip pre-roll ad") appears above
   the scrubber. *Screenshot.*
2. Tap **Skip Ns ad**. **Expected:** Playback seeks to the segment end; the button
   auto-hides once past the ad. *Screenshot.*
3. Settings → Playback → enable **Auto-skip ads**. Return to an episode with a
   mid-roll ad and play into it. **Expected:** The player automatically seeks past
   the detected ad read. *Screenshot.*
4. Check the chapters list. **Expected:** Detected ads are flagged (speaker.slash)
   regardless of the auto-skip setting. *Screenshot.*

## Acceptance Criteria
- The pre-roll skip button appears only while the playhead is inside the ad segment.
- Tapping it seeks to the ad's end.
- With "Auto-skip ads" on, playback seeks past detected ads automatically.
- Detected ads are flagged in the chapter list even with auto-skip off.

## Known Issues / Watch Points
- MEMORY (android_ad_skip_and_library): the ad-skip mechanism is complete; the LLM
  detector is `podcast.ads`. Detection quality varies — false negatives are expected
  on episodes the model didn't flag.
- Without a detected ad segment, the skip button never appears — mark BLOCKED if no
  fixture is available.

## Notes
**Result: BLOCKED**
**Tested: 2026-06-24, ~3:52 UTC**

**Prerequisite Failure: No episodes with detected ads found**

Testing Steps:
- Started episode: "R.F.K. Jr.'s Newest Mission: Getting Us Off Antidepressants" (The Daily)
- Seeked to beginning (0:00 of ~34 min episode)
- Played episode for ~2 minutes
- No "Skip ad" button appeared at any point
- Episode details showed "No chapters yet" — no detected ads flagged

Observations:
- The LLM detector (`podcast.ads`) appears not to have processed any episodes in the current library
- No ad segments were detected in playback
- No chapters/ad markers visible in episode details
- Without a fixture episode with detected ad segments, cannot proceed with Steps 1-4

**Acceptance Criteria Assessment:**
All criteria cannot be evaluated without detected ad segments:
- Pre-roll skip button: NOT FOUND (no detected ads)
- Manual skip: NOT TESTED
- Auto-skip setting: NOT TESTED (feature exists but can't verify behavior)
- Chapter flagging: NOT VISIBLE (no chapters detected)
