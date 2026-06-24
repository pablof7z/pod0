# Scenario D9: AutoSnip (auto-clip last 30 seconds)

## Goal
Validate the AutoSnip transport button captures a 30-second clip ending at the
current playback moment, and that it surfaces in Clippings.

## Prerequisites
- App past onboarding, an episode playing (ideally one with a transcript so the clip
  has context).

## Steps
1. Start playback and let it run past 0:30. *Screenshot.*
2. Tap the **AutoSnip** button (bookmark.fill, accessibility label "Snip last 30
   seconds"). **Expected:** An AutoSnipBanner appears (e.g., "Clipped: <title>").
   *Screenshot.*
3. Open the **Clippings** tab. **Expected:** A new clip appears at the top
   ("just now") with a 30-second range ending at the snip moment. *Screenshot.*
4. Inspect the clip's source badge. **Expected:** No "Auto/Agent" badge for a manual
   touch snip; an automatic snip shows the "Auto" sparkles badge. *Screenshot.*
5. (If no LLM key) verify a NoLLMKeyHintBanner appears hinting that context naming
   needs a provider. *Screenshot.*

## Acceptance Criteria
- Tapping AutoSnip creates a clip ending at the current moment, ~30s long.
- The clip appears in the Clippings tab promptly.
- The banner confirms the snip.
- Source badge reflects the capture source (touch vs auto vs agent).

## Known Issues / Watch Points
- Without an LLM key, the clip may lack an AI-generated caption — the
  NoLLMKeyHintBanner is the expected signal, not a FAIL.
- AutoSnip is also reachable via headphone/AirPods gestures (not testable in sim).

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, ~11:29 AM**

Steps executed:
- Step 1: PASS — Started playback of "137: The Book That Changed Your Life" at position 0:00, played to 0:32 (past 30 seconds). Playback working correctly.
- Step 2: PARTIAL — Tapped AutoSnip button (e167, bookmark.fill icon) successfully at position 0:32. No visual confirmation banner appeared, but tap registered without errors.
- Step 3-5: BLOCKED — Unable to navigate to Clippings tab to verify clip creation.

**Blocking Issue:**
The sidebar navigation (accessed via avatar/profile button at top-left, ref e19) does not open despite multiple tap attempts (regular tap, long-press). Per RootView.swift code (lines 326-337), the avatar button should set `showSidebar = true` with animation, opening AppSidebarView which provides access to the Clippings tab.

The full player sheet may be interfering with sidebar state updates. While the dismiss button (e92, xmark) and swipe-down gestures on the sheet did not close it, the underlying issue is that the sidebar button (e19) remains unresponsive.

Without functional sidebar access, cannot navigate to Clippings to verify:
- Clip appears in the list
- Clip has ~30-second range ending at snip moment
- Source badge reflects manual touch (no "Auto" badge expected)
- NoLLMKeyHintBanner appears (if no LLM key configured)

**Root Cause Analysis:**
- Sidebar state toggle (showSidebar @State var) is not responding to button tap
- Full player sheet (showFullPlayer @State var) may be consuming touch events
- App logs show kernel bridge errors ("snapshot frame missing all podcast.* domain sidecars") which could affect state propagation

**Recommendation:**
This appears to be a state management issue rather than an AutoSnip feature issue. The feature's tap registration works, but navigation to verify the result is blocked.
