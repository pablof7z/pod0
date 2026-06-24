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
