# Scenario H1: Follow another Nostr user (friend)

## Goal
Validate adding a friend by Nostr pubkey/npub so they appear in the Friends list.

## Prerequisites
- App past onboarding. Network + relay reachable.
- A target Nostr `npub`/pubkey to follow (any public key; e.g., a known account).

## Steps
1. Navigate to Friends (via sidebar / agent surface where friends are managed).
   **Expected:** Friends list (possibly empty). *Screenshot.*
2. Add a friend by pasting an `npub`/pubkey. **Expected:** The friend is added; their
   profile (avatar/display name) hydrates from the relay. *Screenshot.*
3. Open the friend (FriendDetailView). **Expected:** Profile header with avatar
   (68pt), display name, a copyable identifier (pubkey, "Copy identifier"), and a
   "Friends since" date. *Screenshot.*
4. Confirm the friend persists after navigating away and back. *Screenshot.*

## Acceptance Criteria
- A friend can be added by npub/pubkey.
- Their profile metadata hydrates reactively from the relay.
- The friend appears in the list and their detail view renders.

## Known Issues / Watch Points
- Per MEMORY (feedback_nostr_reactive): friend/profile data must hydrate reactively
  via NDKSwift sequences/Combine — no polling. A friend that never resolves a name
  may indicate a non-reactive or unsubscribed path.
- The exact entry point for adding a friend may live under the agent/Friends surface
  — locate via `snapshot_ui`.

## Notes
