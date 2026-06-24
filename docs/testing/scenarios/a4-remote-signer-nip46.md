# Scenario A4: Connect a NIP-46 remote signer

## Goal
Validate the NIP-46 remote-signer flow (bunker URI paste and QR pairing) so the
private key can stay in a separate signing app (Amber / nsec.app).

## Prerequisites
- App past onboarding.
- A NIP-46 bunker connection string OR a signer app/emulator able to scan the QR.
  (For a smoke test without a real signer, the UI/state transitions can still be
  validated through the connecting state.)

## Steps
1. Settings → Account → Identity → Advanced → **Sign in with a remote signer**.
   **Expected:** "Remote signer" screen with intro prose, a "Scan to connect" row,
   and a bunker URI input card. *Screenshot.*
2. (Path A — bunker URI) Paste a `bunker://…` URI into the connect card and tap
   **Connect**. **Expected:** Status shows connecting, then connected (if a real
   signer approves) — a Disconnect button appears. *Screenshot.*
3. (Path B — QR) Tap **Scan to connect**. **Expected:** "Scan to connect" screen
   shows a QR code (nostrconnect URI), optional "Open in Amber"/"Open in Primal"
   buttons, and a footnote about a 5-minute expiry. *Screenshot.*
4. With a signer, scan the QR. **Expected:** UI moves to "Waiting for signer to
   connect…", then "Connected" with a "Done" button on success. *Screenshot.*
5. Tap **Done** / back. Open Account Details. **Expected:** mode shows "Bunker via
   <app>" / "remote signer" as the source. *Screenshot.*

## Acceptance Criteria
- The QR screen renders a scannable code and a Cancel affordance while unpaired.
- The connecting state shows a "Waiting for signer to connect…" UI with a Cancel.
- On success the mode badge flips to a remote-signer/bunker label.
- On failure, an error + "Try again" is shown (not a silent hang).
- The footnote states the QR expires after 5 minutes.

## Known Issues / Watch Points
- Without a live signer you cannot reach the "Connected" state — record how far the
  flow progressed (QR shown, connecting state) and mark BLOCKED, not FAIL.
- Per MEMORY, all Nostr surfaces must be reactive (no polling) — the connected
  transition should arrive via subscription, promptly.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, 11:00 UTC**

Unable to complete the scenario due to navigation difficulties reaching the Identity > Advanced > Remote Signer settings screen.

**Observations:**
- Step 1: Successfully navigated to Settings screen (gear icon opens modal with Account, Library, Listening, Intelligence sections)
- Step 1: Can see "Quiet Thread" account showing "Local Key" mode in Account section
- Step 1: However, tapping on the account row to access Identity/Advanced settings was blocked — the element ref was not actionable (TARGET_NOT_ACTIONABLE error)
- The UI snapshot references were shifting between different screens (Home, Playback settings, Settings modal) making stable navigation difficult

**Blockers:**
- Account identity row not responding to tap action despite being visible in Settings modal
- Navigation stack issues causing screen state to change unexpectedly between snapshots
- Unable to reach the "Advanced" sub-screen where "Sign in with a remote signer" option should be located

**Next Steps for Manual Testing:**
- Try navigating through a direct link/deeplink to the remote signer screen if available
- Verify that the Identity row in Settings → Account is fully interactive and wired correctly
- Check if the Remote Signer feature has been fully implemented in the current build

**Acceptance Criteria Status:**
- QR screen not reached: UNKNOWN
- Connecting state not tested: UNKNOWN
- Mode badge flip not verified: UNKNOWN
- Error handling not verified: UNKNOWN
- 5-minute expiry footnote not verified: UNKNOWN
