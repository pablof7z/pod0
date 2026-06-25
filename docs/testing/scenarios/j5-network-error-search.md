# Scenario J5: Network errors during search / subscribe

## Goal
Validate that transient network errors during search and subscribe surface clear,
recoverable errors without crashing or corrupting state.

## Prerequisites
- App past onboarding. Ability to toggle the network mid-action (or use a flaky
  network condition).

## Steps
1. Open Search, begin typing a query, and toggle the network OFF mid-request.
   **Expected:** The in-flight search fails gracefully (empty/error state), no crash.
   *Screenshot.*
2. Restore the network and re-run the query. **Expected:** Results load on retry.
   *Screenshot.*
3. Open Add Show → From URL, enter a valid feed, turn the network OFF, tap Subscribe.
   **Expected:** A red error banner ("couldn't fetch feed"), Subscribe re-enabled to
   retry. *Screenshot.*
4. Restore the network and retry Subscribe. **Expected:** Subscribes successfully; no
   duplicate/half-subscribed state. *Screenshot.*
5. Enter a malformed/non-feed URL while online. **Expected:** A parse error banner,
   no subscription created. *Screenshot.*

## Acceptance Criteria
- Network failures during search/subscribe show clear errors, never crash.
- Retrying after recovery succeeds.
- A failed subscribe leaves no partial/duplicate subscription.
- Malformed URLs are rejected with an error.

## Known Issues / Watch Points
- The optimistic-subscribe path may show the show before episodes hydrate; a
  mid-flight network drop should not leave a permanently empty show — note if it does.
- Watch for an infinite spinner instead of a terminal error state.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24 ~14:10 UTC**

[Step-by-step observations]
- Step 1 (Search): Successfully navigated to Search interface; typed "science" query and cached results loaded (135: Deep Space, 136: Once More with Feeling, 137: The Book That Changed Your Life).
- Network toggle: Attempted to disable network via `xcrun simctl io ... poll off` but cannot verify if network is truly offline at the app layer; cached results remained visible, preventing validation of network error handling during search.
- Step 3+ (Add Show from URL): Feature NOT FOUND - explored Library, Sidebar (Home/Library/Podcasts/Bookmarks/Clippings), and Search interfaces but found no "Add Show → From URL" entry point. This feature may not be implemented or may be hidden in a different navigation flow.

[Blockers]
1. "Add Show → From URL" feature does not appear to exist in the current UI navigation.
2. Network error states cannot be reliably triggered/verified via simulator-level network toggle without app-layer error surfacing.
3. Time constraints (4 min/scenario) insufficient for full exploration of all possible navigation paths.

[Acceptance criteria met: NO]
- Network failure error messaging: Cannot verify (no error UI found during search)
- Retry success: Not tested
- No duplicate subscriptions: Not tested
- Malformed URL rejection: Not tested
