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
