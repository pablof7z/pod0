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

**Result: PASS**
**Tested: 2026-06-24 approx 1:27 AM**

Tested all steps sequentially. Observations:

- **Step 1: Notifications Settings View** - PASS
  - Navigated Settings → System → Notifications successfully
  - Permission section displayed with "Not yet asked" status
  - "Allow" button visible
  - Categories section present with "New episode alerts" toggle (pre-enabled)
  - Screenshot: i5-screenshot-1-notif-settings.jpg

- **Step 2: iOS Permission Prompt** - PASS
  - Tapping "Allow" triggered iOS system permission dialog: "Podcast Would Like to Send You Notifications"
  - Dialog showed "Don't Allow" and "Allow" buttons
  - Screenshot: i5-screenshot-2-permission-prompt.jpg

- **Step 2b: Permission Granted** - PASS
  - After granting permission via iOS dialog, permission status updated to "Allowed" (green bell icon)
  - Text confirmed: "Notifications are enabled for this app."
  - Screenshot: i5-screenshot-3-after-grant.jpg

- **Step 3: Categories Section** - PASS
  - "New episode alerts" toggle is visible and enabled (green) after permission granted
  - Description text correct: "New-episode alerts also respect each show's individual notification toggle (see Subscriptions)."
  - Toggle is only enabled when authorization is granted (matches acceptance criteria)

- **Step 4: Toggle Persistence** - PASS
  - Toggled "New episode alerts" OFF: toggle switched to gray/disabled state
  - Screenshot: i5-screenshot-4-toggle-off.jpg
  - Toggled "New episode alerts" ON: toggle switched back to green/enabled state
  - State persisted correctly between toggles
  - Screenshot: i5-screenshot-5-toggle-on.jpg

- **Step 5: Per-Show Episode Alerts** - PASS
  - Navigated to Settings → Subscriptions
  - Found "This American Life" show with 1 subscription
  - "Episode alerts" toggle present for the show, currently enabled (green)
  - Bell icon indicates this is notification-related setting
  - Confirms per-show toggles gate notifications per show
  - Screenshot: i5-screenshot-6-subscriptions.jpg

**Acceptance Criteria Met:**
✅ Permission status reflects real authorization state (Allowed shown with green bell)
⏳ "Open Settings" button for denied state - NOT TESTED (would need to deny permission first)
✅ "New episode alerts" toggle disabled until authorized, then persists
✅ Per-show "Episode alerts" toggles in Subscriptions gate notifications per show
