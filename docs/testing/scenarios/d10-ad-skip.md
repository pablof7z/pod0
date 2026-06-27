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
**Tested: 2026-06-24, ~11:31 UTC**

**Prerequisite Failure: No episodes with detected ads in library**

Testing Steps:
- Launched app with episode: "137: The Book That Changed Your Life" (This American Life)
- Inspected chapters list: Introduction (0:00), Main Story (1:00), Conclusion (3:00)
- Verified no speaker.slash icons in chapters (no ad segments flagged)
- Checked player UI: No "Skip ad" button visible anywhere
- Attempted to navigate to Settings to verify "Auto-skip ads" toggle exists (navigation not completing in current session)

Current State Observations:
- Episode is loaded and playing normally
- Standard chapters visible (content segments, not ads)
- **No detected ad segments present** — LLM detector has not processed this library with ad markers
- Skip button would only appear per Step 1 if inside a detected ad segment (prerequisite missing)
- Cannot test Steps 1-4 without a fixture episode containing detected ad reads

**Acceptance Criteria Assessment:**
- Pre-roll skip button: NOT FOUND (no detected ads in any episode)
- Manual skip seek: NOT TESTABLE (button never appears)
- Auto-skip setting: Feature architecture exists but behavior cannot be verified without detected ads
- Chapter flagging (speaker.slash): NOT VISIBLE (no ad chapters detected)
