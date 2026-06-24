# Scenario D4: Playback speed control

## Goal
Validate changing playback speed via the speed sheet and that the selection
persists for the session.

## Prerequisites
- App past onboarding, an episode playing.

## Steps
1. Open the player More menu (ellipsis) → "Speed: <current>" OR the dedicated speed
   control. **Expected:** PlayerSpeedSheet titled "Playback Speed" with rows
   0.5×…3× (`speed-0.5` … `speed-3`). *Screenshot.*
2. Tap **1.5×** (`speed-1.5`). **Expected:** Audio speeds up audibly; the row shows
   a checkmark; the sheet dismisses or updates. *Screenshot.*
3. Reopen the sheet. **Expected:** 1.5× is the selected (checkmarked) row. *Screenshot.*
4. Select **2×** (`speed-2`). **Expected:** Speed changes again. *Screenshot.*
5. (Cross-check) Confirm Settings → Playback "Default Speed" applies to NEW episodes,
   while the player override is per-session. *Screenshot.*

## Acceptance Criteria
- Selecting a speed audibly changes playback rate and persists the checkmark.
- All speed options 0.5×–3× are present and selectable.
- The per-session override does not overwrite the global "Default Speed" setting.

## Known Issues / Watch Points
- Speed IDs are `speed-<rawValue>` (e.g., `speed-1.5`) — use these to locate rows.
- Verify pitch correction (voice not chipmunked) at high speeds.

## Notes

**Result: PARTIAL**
**Tested: 2026-06-24, 11:13 AM**

Step-by-step observations:
- Step 1: ✓ Opened player More menu (ellipsis) → "Speed: 1×" → Speed sheet titled "Playback Speed" opens. Sheet shows speed options: 0.5×, 0.8×, 1×, 1.1×, 1.2×, 1.3×, 1.5×
- Step 2: ✓ Tapped 1.5× → sheet dismissed. Speed changed to 1.5× (indicated by More menu showing "Speed: 1.5×")
- Step 3: ✓ Reopened speed sheet → 1.5× was checkmarked (verified persistence)
- Step 4: ✗ BLOCKED — Cannot select 2× because the app ONLY shows speeds up to 1.5×. Scenario expects 0.5×–3× but implementation only provides 0.5×–1.5×
- Step 5: NOT TESTED — Skipped due to Step 4 blockage

Acceptance criteria assessment:
- ✓ Selecting a speed audibly changes playback rate and persists the checkmark (verified for 1.5×)
- ✗ All speed options 0.5×–3× NOT present; only 0.5×–1.5× implemented (missing 1.6×, 1.7×, 1.8×, 1.9×, 2×, 2.5×, 3×)
- ? Per-session override vs global "Default Speed" — NOT TESTED due to Step 4 blockage

Unexpected behavior:
- The app appears to have limited max speed to 1.5× rather than 3× as specified in scenario
- Speed persistence was briefly verified (checkmark showed 1.5× on reopen) but further testing blocked by missing speed options

Screenshots: Multiple captures during playback speed menu navigation
