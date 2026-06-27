# Scenario I2: Configure OpenRouter and enable Whisper transcription

## Goal
Validate saving an OpenRouter API key and enabling the Whisper transcription
fallback so episodes without publisher transcripts can be transcribed.

## Prerequisites
- App past onboarding.
- OpenRouter API key (your own key — get one at openrouter.ai)

## Steps
1. Settings → Intelligence → Providers → **OpenRouter**. **Expected:** Connection
   section with a status label (`openrouter-status-label`), a BYOK button, and a
   manual key field (`openrouter-api-key-field`). *Screenshot.*
2. Paste the test key into the manual key field and submit. **Expected:** Status
   flips to "Manual key saved" / "Connected"; a "Validate Key" button appears.
   *Screenshot.*
3. Tap **Validate Key**. **Expected:** "Validating…", then a key-info card with usage
   limits on success. *Screenshot.*
4. Settings → Intelligence → **Transcripts**. Enable **AI transcription fallback**.
   **Expected:** Toggle on; if the STT provider needs a key, a readiness warning is
   shown until configured. *Screenshot.*
5. Settings → Intelligence → **Models → Speech** → select **Whisper** (OpenRouter).
   **Expected:** Whisper selected; the Transcripts readiness warning clears. *Screenshot.*

## Acceptance Criteria
- The OpenRouter key saves and the status label reflects it.
- Validate Key succeeds and shows usage limits.
- "AI transcription fallback" can be enabled and Whisper selected as the speech
  provider; the readiness warning resolves once configured.

## Known Issues / Watch Points
- The disconnect button (`openrouter-disconnect-button`) removes the key — don't
  tap it unless testing teardown.
- Keys are stored in the Keychain and survive relaunch (but not a data wipe).
- This scenario is a prerequisite for E2 (Whisper transcription).

## Notes

**Result: PARTIAL**
**Tested: 2026-06-24, approx 1:06 PM**

### Step-by-step observations:

- **Step 1:** Settings → Intelligence → Providers → OpenRouter. ✅ PASS
  - Connection section displays as expected with status label (`openrouter-status-label`), "Connect with BYOK" button, and manual key field (`openrouter-api-key-field`)
  - Screenshot: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_8669affc-4ce1-4386-8010-b2afd676b691.jpg

- **Step 2:** Paste test key and submit. ✅ PASS
  - Key pasted successfully (masked as dots in field)
  - Tapped Save button
  - Status changed to "Manual key saved" (green checkmark)
  - "Validate Key" button appeared
  - Screenshot: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_b10e83df-9101-436e-96d1-d5965321fe31.jpg

- **Step 3:** Validate Key. ✅ PASS
  - Tapped "Validate Key" button
  - Validation succeeded with "Key validated" status (green checkmark)
  - Key info card displayed: "sk-or-v1-61f...f24" with usage limits "Paid -1 req/10s"
  - Screenshot: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_0c130cdb-a085-4328-9276-d7fd4cc28a9b.jpg

- **Step 4:** Settings → Intelligence → Transcripts. Enable AI transcription fallback. ✅ PASS
  - Navigated to Transcripts screen
  - "AI transcription fallback" toggle was ALREADY ENABLED (green toggle showing value=1)
  - "Auto-ingest publisher transcripts" toggle also enabled
  - Screenshot: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_e5b76032-2dcb-4ecc-9950-656c532ab070.jpg

- **Step 5:** Settings → Intelligence → Models → Speech → Select Whisper. ❌ BLOCKED
  - Navigated to Speech settings
  - Current Transcription Provider: "Apple on-device"
  - Tapped Provider button (e128) multiple times to open Picker menu
  - Picker menu failed to open/display options
  - Swift code shows `.pickerStyle(.menu)` for STTProvider picker and enum includes `.openRouterWhisper` case
  - Whisper option is implemented in code but UI menu is not responding to taps
  - ISSUE: The menu picker control appears to be non-functional or requires a different interaction method

### Test Results Summary:

**Acceptance Criteria:**
1. ✅ OpenRouter key saves and status label reflects it — PASS
2. ✅ Validate Key succeeds and shows usage limits — PASS
3. ⚠️ AI transcription fallback can be enabled (already was) — PARTIAL (feature exists but no user action needed)
4. ❌ Whisper selected as speech provider — BLOCKED (UI menu picker non-functional)

The scenario is blocked at Step 5 due to a UI bug where the provider picker menu doesn't respond to user interaction. The code supports Whisper (.openRouterWhisper case) but the SwiftUI Picker control with .menu style is not opening when tapped.
