# Scenario H5: Episode comments and feedback

## Goal
Validate the episode comments section (Nostr-anchored) and the in-app Feedback
compose/thread flow (with optional screenshot annotation).

## Prerequisites
- App past onboarding. Network + relay reachable. An episode with a Podcasting 2.0
  GUID (comments only render for those).

## Steps
1. Open an episode detail with a P2.0 GUID and scroll to **Comments**
   (EpisodeCommentsSection). **Expected:** Comment thread (may be empty) anchored to
   the episode. *Screenshot.*
2. Open the **Feedback** surface (toolbar/sidebar). **Expected:** "Feedback" list
   with a "Mine"/"Everyone" segmented control and a search bar. *Screenshot.*
3. Tap the compose (pencil, "New feedback"). **Expected:** FeedbackComposeView with
   an identity row, a text editor (placeholder "What's on your mind?"), and a 280-char
   counter. *Screenshot.*
4. Type feedback. Tap the camera icon ("Attach screenshot") → annotate (undo/clear
   tools) → done. **Expected:** A screenshot preview with "Re-annotate"/"Remove".
   *Screenshot.*
5. Tap **Send**. **Expected:** Publishes the thread to Nostr; it appears under "Mine".
   *Screenshot.*

## Acceptance Criteria
- The comments section renders for P2.0-GUID episodes and is Nostr-anchored.
- Feedback compose enforces the 280-char limit and requires non-empty text to Send.
- Screenshot attach + annotate (undo/clear) works; the image attaches.
- Sending publishes to Nostr and the thread shows under "Mine".

## Known Issues / Watch Points
- Comments are conditional on a Podcasting 2.0 GUID — absence is expected for many
  feeds (not a FAIL).
- Feedback can be anonymous ("Anonymous — tap to set identity") or tied to the Nostr
  identity — verify the identity row reflects the current state.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, 8:54 AM**

### Observations

**Step 1 - Comments Section: PASS**
- Successfully navigated to episode detail for "As Trump Purges Immigration Judges, One Speaks Out" (The Daily)
- Scrolled to Comments section and verified it renders
- Comments section shows:
  - "Comments" heading with icon
  - Text input field "Add a comment..."
  - User identity (npub1jsps1...a53mwc)
  - "Post" button
  - Message: "Be the first to comment. Posts publish to your Nostr relay and stay readable from any NIP-22 client."
- Episode has Podcasting 2.0 GUID (comments rendered successfully, not a feed without GUID)
- Screenshot: `/var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_2fafa2b8-d47d-4384-92a1-02ee6413b877.jpg`

**Step 2 - Feedback Surface: BLOCKED (NOT FOUND)**
- Opened sidebar navigation: Home, Library, Podcasts, Bookmarks, Clippings
- No Feedback option visible in sidebar
- No Feedback tab/view found in main navigation
- No Feedback option in episode options menu (3-dot menu not accessed due to navigation issues)
- Attempted to locate Feedback surface via:
  - Sidebar navigation scroll (no additional options)
  - Home dropdown menu (not tested due to UI timeout issues)
  - Episode detail view

**Blocker**: Feedback surface is not accessible via normal navigation paths. Feature may be:
- Not yet implemented
- Gated behind a different UI pattern not discoverable through standard navigation
- Requires specific app state or user identity setup

### Acceptance Criteria Status
- ✓ Comments section renders for P2.0-GUID episodes and is Nostr-anchored
- ✗ Feedback compose and thread flow: Cannot test (feature not found)
- ✗ Screenshot attach + annotate: Cannot test (feature not found)
- ✗ Publish to Nostr: Cannot test (feature not found)
