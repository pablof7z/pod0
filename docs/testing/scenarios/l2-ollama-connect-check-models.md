# Scenario L2: Ollama provider — connect, "Check Available Models", model count

## Goal
Complete the Ollama configuration that G1 left PARTIAL: beyond setting the endpoint,
exercise **Connect with BYOK** / **Save Manual Key** and **Check Available Models**
so a model count is retrieved, and confirm the provider status flips to connected.

## Prerequisites
- App past onboarding.
- A reachable Ollama: EITHER a local `ollama serve` on the host with
  `deepseek-v4-flash:cloud` pulled (endpoint `http://localhost:11434/api/chat`; the
  simulator reaches the host via localhost), OR Ollama Cloud with a valid key for
  BYOK/manual entry.

## Steps
1. Settings → Intelligence → Providers → **Ollama Cloud**. **Expected:** a
   **Connection** section (status label + "Connect with BYOK" / manual key field +
   "Save Manual Key") and an **Endpoint** section (URL field, default
   `http://localhost:11434/api/chat`). *Screenshot.*
2. (Local path) Confirm/Set the Endpoint to `http://localhost:11434/api/chat`, Save.
   **Expected:** "Reset to Default" appears. *Screenshot.*
3. (Cloud path) Tap **Connect with BYOK** (or paste a manual key + **Save Manual
   Key**). **Expected:** status flips toward connected (green checkmark / "saved").
   *Screenshot.*
4. Tap **Check Available Models**. **Expected:** a brief validating state
   (ProgressView; button disabled), then a model count appears on success (the
   credential/info area shows the count). G1 reported this button was not visible —
   it lives in the Connection section (OllamaSettingsView): if absent, confirm the
   build and that the connection method (BYOK vs manual) is set. *Screenshot.*
5. Back out to Providers. **Expected:** Ollama shows Connected/configured (not "Not
   set up"). *Screenshot.*

## Acceptance Criteria
- The Ollama endpoint sets and persists (default
  `http://localhost:11434/api/chat`).
- A connection (BYOK or manual key) can be established, OR a local endpoint reached.
- **Check Available Models** returns a model count when the provider is reachable.
- The Providers list reflects Ollama as connected/configured.

## Known Issues / Watch Points
- If "Check Available Models" errors, the local Ollama isn't up or the model isn't
  pulled, or the cloud key is invalid — confirm `ollama list` on the host shows the
  model. A simulator cannot reach a server that isn't running on the host.
- Empty/invalid endpoint falls back to the default — verify the fallback copy.
- This unblocks the live-LLM scenarios (L1 model selection, L3/L4 Q&A, L5 highlight);
  if Ollama is unreachable here, those will also fail at the network layer.

## Notes
