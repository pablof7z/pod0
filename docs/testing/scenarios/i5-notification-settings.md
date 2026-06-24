# Scenario I5: Notification settings

## Goal
Validate the Notifications settings: permission request/status and the new-episode
alert category toggle (respecting per-show toggles).

## Prerequisites
- App past onboarding. Start with notification permission not-yet-determined for the
  full flow (erase the sim or reset notification permissions).

## Steps
1. Settings → System → **Notifications** (NotificationSettingsView). **Expected:**
   A Permission section with a status row and, when not-determined, an **Allow**
   button. *Screenshot.*
2. Tap **Allow**. **Expected:** The iOS permission prompt; granting flips status to
   "Allowed" (green bell). *Screenshot.*
3. Inspect the **Categories** section. **Expected:** A "New episode alerts" toggle,
   enabled only when authorized. *Screenshot.*
4. Toggle "New episode alerts" off/on. **Expected:** Persists. *Screenshot.*
5. Cross-check Settings → Subscriptions: each show has its own "Episode alerts"
   toggle that also gates per-show notifications. *Screenshot.*

## Acceptance Criteria
- The permission status row reflects the real authorization state (Allowed / Denied
  / Not yet asked, etc.).
- When denied, an "Open Settings" button appears (deep-links to iOS Settings).
- The "New episode alerts" toggle is disabled until authorized, then persists.
- Per-show alert toggles in Subscriptions gate notifications per show.

## Known Issues / Watch Points
- iOS controls whether notifications are deliverable at all; the in-app toggles only
  filter once permission is granted.
- New-episode alerts respect BOTH the global category toggle and each show's toggle.

## Notes
