# Scenario H3: Share an episode

## Goal
Validate sharing an episode: copy deeplink, copy link at current time, system share,
and quote share.

## Prerequisites
- App past onboarding, an episode open/playing (>5s for the timestamped link;
  transcript ready for quote share).

## Steps
1. In the player top bar, tap **Share** ("Share episode"). **Expected:** PlayerShare
   Sheet with: "Copy episode link", "Copy link at current time" (when >5s played),
   "System share", and "Share quote" (when transcript ready). *Screenshot.*
2. Tap **Copy episode link**. **Expected:** Copies `podcastr://e/<guid>`. *Screenshot.*
3. Tap **Copy link at current time**. **Expected:** Copies the link with
   `?t=<seconds>`. *Screenshot.*
4. Tap **System share**. **Expected:** The iOS share sheet opens with an app-context
   preview. *Screenshot.*
5. Tap **Share quote**. **Expected:** Spinner while the kernel resolves transcript
   boundaries, then a QuoteShareView with a transcript-aligned segment. *Screenshot.*

## Acceptance Criteria
- Copy episode link copies `podcastr://e/<guid>`.
- Copy link at current time appends the timestamp (`?t=`), only available after >5s.
- System share opens the iOS share sheet.
- Share quote resolves a transcript-aligned quote (requires transcript).

## Known Issues / Watch Points
- "Share quote" needs a ready transcript; otherwise the option is hidden.
- Episode rows also offer "Share with timestamp" when currently playing — a related
  but separate entry point.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, ~12:37 PM**

Unable to access the player view to test share functionality. Navigation attempts:
- Started on Friends view, navigated back through various settings screens
- Mini-player was visible throughout (This American Life, 137: The Book That Changed Your Life)
- Attempted to open player by: tapping mini-player bar, tapping episode title, tapping Inbox button
- Each navigation attempt kept returning to or staying in Settings view
- Mini-player remained visible but was not actionable (swipe/drag gestures not supported on e67 element ref)
- Tap on mini-player-bar (e67) did not expand the player view

**Blocking Issue:** Cannot access the expanded player view where the Share button (top bar) is located. The mini-player exists and shows the episode, but the full player view needed to test all 5 steps is not reachable from the current app state.

**Next Steps:** Requires investigation of player navigation flow - may need to restart the app, check if there's a specific navigation path to the full player, or verify if the Share button exists in a different UI location (e.g., episode row context menu).
