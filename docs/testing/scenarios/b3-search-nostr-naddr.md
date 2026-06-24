# Scenario B3: Discover/subscribe a Nostr-published podcast (NIP-F4)

## Goal
Validate the Nostr-native discovery path: finding and subscribing to a podcast
published on Nostr (NIP-F4 kind:10154), without RSS.

## Prerequisites
- App past onboarding. Network + relay (`relay.primal.net`) reachable.
- At least one NIP-F4 podcast exists on the relay (an owned podcast published via
  H4 can serve as the fixture, or a known naddr/npub publisher).

## Steps
1. From Home, open **Add Show** → switch to the **Nostr** segment. **Expected:**
   A "Filter shows" field; results stream from the relay in real time. *Screenshot.*
2. Wait for kind:10154 shows to appear. **Expected:** Rows show title, description,
   categories. *Screenshot.*
3. (If supported) paste a publisher `npub`/`naddr` into the filter to target a
   specific publisher. **Expected:** That publisher's show(s) appear. *Screenshot.*
4. Tap a row to subscribe. **Expected:** Subscribe succeeds (checkmark); inbound
   episodes upsert via the Nostr episodes observer. *Screenshot.*
5. Open the subscribed show. **Expected:** Episodes that arrived over Nostr render
   in the show detail. *Screenshot.*

## Acceptance Criteria
- The Nostr tab streams kind:10154 shows reactively (no manual refresh needed).
- Subscribing to a Nostr show works without an RSS feed URL.
- Episodes delivered over Nostr (kind:54 author-filtered) appear in the show.

## Known Issues / Watch Points
- This path depends on relay content; if no NIP-F4 shows are published, the list is
  empty — set up a fixture via H4 first or mark BLOCKED.
- Per MEMORY, must be reactive — results should arrive via subscription, not poll.
- Discovery uses `NostrDiscoveryObserver` + `EnsureInterest`; a feedless subscribe
  dispatches `SubscribeNostr`. Note if subscribe silently no-ops.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, 9:48-9:50 AM**

### Observations
- Step 1: Successfully opened Add Show dialog and switched to Nostr tab. Filter field present and reactive search working as expected.
- Step 2: App shows "Searching... Looking for NIP-F4 shows on this relay" message. After ~13 seconds, no kind:10154 shows appeared in the list.
- Step 2.5: Tested filter reactivity by typing "test" → UI changed to "No matches" (reactive behavior confirmed). Cleared filter → UI returned to "Searching..." (confirmation of reactive subscription, not polling).
- No NIP-F4 podcasts were found on relay.primal.net at test time.

### Acceptance Criteria Status
- Reactive streaming: ✅ PARTIAL (stream updates reactively, but no content on relay)
- Subscribe feedless: ⛔ CANNOT TEST (no shows to subscribe to)
- Episode delivery: ⛔ CANNOT TEST (no shows)

### Blockers
**Prerequisite not met**: "At least one NIP-F4 podcast exists on the relay" — No shows published on relay.primal.net at test time. Per scenario instructions, this requires a fixture via H4 first or the scenario is marked BLOCKED.
