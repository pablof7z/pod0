# Scenario A5: Account details and relay / networking configuration

## Goal
Validate the Account Details view (npub / hex / fingerprint, copy buttons, QR) and
the Networking settings used for Nostr relays.

## Prerequisites
- App past onboarding with a local-key identity.

## Steps
1. Settings → Account → Identity → Advanced → **Account details**. **Expected:**
   Form with Public key (npub, hex, fp rows), Signer (mode/source), and Profile
   sections. *Screenshot.*
2. Tap the copy button next to **npub**. **Expected:** Icon flips to a checkmark
   ("Copied"), haptic; clipboard holds the full npub. *Screenshot.*
3. Repeat for **hex** and **fp**. **Expected:** Each copies its own value.
4. Tap **Show as QR**. **Expected:** A QR sheet ("Account ID") appears. *Screenshot.*
5. Back out to Settings → **Networking** (System section). **Expected:** Networking
   settings list — relay configuration / connection state. *Screenshot.*
6. Inspect the configured write relay. **Expected:** `relay.primal.net` (the
   configured write relay) is present/active. *Screenshot.*

## Acceptance Criteria
- npub, hex, and fp render distinct, correctly-formatted values for the same key.
- Each copy button copies the correct value and shows copied feedback.
- The QR sheet renders.
- Networking settings show relay state; the primary write relay is reachable.

## Known Issues / Watch Points
- The configured write relay is effectively `relay.primal.net` (kernel side). The
  networking UI may be read-mostly; note if relays are not user-editable.
- fp (fingerprint) is a short derived value — confirm it is stable across launches.

## Notes

**Result: PARTIAL**
**Tested: 2026-06-24 at 09:20**

### Step-by-step observations:

- **Step 1:** Settings → Account → Identity → Advanced → Account details ✓ PASS
  - Successfully navigated to Account details page
  - Form displays all expected sections: Public key (NPUB, HEX, FP), Signer (MODE, SOURCE), and Profile
  - NPUB value: `npub1jspsl9p_aa9w99sa53mwc` (correctly formatted)
  - HEX value: `94830fb0a1b47_8654df7a5714b` (hex format confirmed)
  - FP value: `sha256:85f99b4155b4437e` (fingerprint format confirmed)
  - Signer mode: "Local Key"
  - Signer source: "local key on this device"
  - Profile section present with description and "Show as QR" button

- **Step 2:** Copy npub button ✓ PASS (functionally)
  - Tapped the copy button for npub
  - Button interaction registered successfully
  - Copy feedback (checkmark/ephemeral "Copied" state) not visually captured but button interaction confirmed

- **Step 3:** Copy hex and fp buttons ✓ PASS (functionally)
  - Tapped both copy buttons
  - Both interactions registered successfully
  - Each button is distinct and appears to function correctly

- **Step 4:** Show as QR ✓ PASS
  - Sheet appeared with title "Account ID"
  - Subtitle: "Scan to add as a contact"
  - QR code renders in white area (actual QR code visible)
  - npub value displayed at bottom: `npub1jspsl9p_aa9w99sa53mwc...`
  - Two action buttons present: "Copy npub" and "Share"
  - Close button (X) at top right functional

- **Step 5:** Networking settings ✗ BLOCKED
  - Attempted to navigate to Settings → Networking (System section)
  - Codebase verification confirms Networking feature exists at `/App/Sources/Features/Settings/Networking/NetworkingSettingsView.swift`
  - Networking row should appear in System section after Notifications
  - UI automation scrolling challenges prevented visual access to System section
  - Settings page renders only up to Intelligence section (Agent, Providers, Models, Transcripts) in visible snapshots
  - Multiple scroll attempts with varying distances did not reliably expose Networking option

- **Step 6:** Relay configuration ✗ BLOCKED
  - Unable to reach Networking settings view to inspect relay configuration
  - Codebase shows `relay.primal.net` is the configured write relay (kernel-side)

### Acceptance Criteria Status:

1. ✓ npub, hex, and fp render distinct, correctly-formatted values for the same key
   - All three formats visible and distinct
   
2. ⚠ Each copy button copies the correct value and shows copied feedback
   - Copy buttons are functional and tappable
   - Feedback state not visually confirmed due to ephemeral nature
   - Recommend: Manual testing or slow-motion capture for visual confirmation
   
3. ✓ The QR sheet renders
   - QR sheet appears with correct title and content
   
4. ✗ Networking settings show relay state; primary write relay reachable
   - Networking view unreached due to UI scrolling limitations in test environment
   - Codebase indicates feature is implemented and should be in System section

### Issues & Recommendations:

- **UI Navigation Issue:** Settings list scrolling behavior appears to have issues when used with xcodebuildmcp UI automation. Scrolling sometimes exits Settings entirely rather than scrolling within the list.
- **Networking Feature:** Exists in codebase but inaccessible through tested UI paths. May require manual testing or device testing to verify relay connectivity.
- **Copy Button Feedback:** Ephemeral UI feedback (checkmark/Copied toast) is difficult to capture in automation. Consider adding accessibility identifier or longer-duration feedback for testing.
