# Scenario H2: View a friend's activity and add a note

## Goal
Validate the friend detail view: listening/activity transcript, notes (add/edit/
delete), rename, and remove-friend.

## Prerequisites
- App past onboarding with ≥1 friend (H1).

## Steps
1. Open a friend (FriendDetailView). **Expected:** Profile header + Notes section +
   Actions section. *Screenshot.*
2. Inspect the activity (FriendConversationTranscriptView) if shown. **Expected:**
   The friend's recent activity / shared listening surfaces. *Screenshot.*
3. Tap **Add note** → enter text → save. **Expected:** A note row appears under
   Notes with a count badge. *Screenshot.*
4. Tap the identifier ("Copy identifier"). **Expected:** Copies the pubkey; label
   flips to "Identifier copied". *Screenshot.*
5. Tap **Rename** → set a new display name → confirm. **Expected:** Header updates.
   *Screenshot.*
6. Tap **Remove Friend** (destructive) → confirm the two-step alert. **Expected:**
   Friend removed from the list. *Screenshot.*

## Acceptance Criteria
- Notes can be added to a friend and persist with a count badge.
- The identifier copies and shows copied feedback.
- Rename updates the display name; Remove deletes the friend (with confirmation).
- Friend activity, when present, renders.

## Known Issues / Watch Points
- "Friend's listening activity" may be sparse/empty depending on what the friend
  has published — empty is not a FAIL.
- Activity must be reactive (no polling).

## Notes
