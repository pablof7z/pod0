# Scenario G1: Configure the Ollama provider and select the model

## Goal
Validate configuring Ollama as the agent LLM provider (endpoint +/- key) and
selecting `deepseek-v4-flash:cloud` as the model.

## Prerequisites
- App past onboarding.
- For a LOCAL Ollama: `ollama serve` running on the host with the model pulled.
  Endpoint `http://localhost:11434/api/chat`. (The simulator shares the Mac's
  network, so localhost reaches the host.)

## Steps
1. Settings → Intelligence → **Providers** → **Ollama Cloud**. **Expected:**
   Connection section (status, BYOK/manual key) and an **Endpoint** section.
   *Screenshot.*
2. In the **Endpoint** field, set `http://localhost:11434/api/chat` (for local).
   Tap Save. **Expected:** Endpoint saved; "Reset to Default" appears if changed.
   *Screenshot.*
3. (Cloud path) Tap **Connect with BYOK** or paste a manual Ollama key, then
   **Check Available Models**. **Expected:** A model count appears on success.
   *Screenshot.*
4. Settings → Intelligence → **Models**. Select the agent role's provider = Ollama
   and model = `deepseek-v4-flash:cloud`. **Expected:** Selection persists. *Screenshot.*
5. Back out. **Expected:** Providers shows Ollama as Connected / configured. *Screenshot.*

## Acceptance Criteria
- The Ollama endpoint can be set and persists (default is
  `http://localhost:11434/api/chat`).
- A model count is retrievable when the provider is reachable.
- `deepseek-v4-flash:cloud` can be selected as the agent model under Models.
- The status reflects the configured state.

## Known Issues / Watch Points
- If "Check Available Models" fails, confirm the local Ollama is up and the model is
  pulled; a simulator can't reach a server that isn't running on the host.
- Empty/invalid endpoint falls back to the default — verify the fallback copy.

## Notes

**Result: PARTIAL**
**Tested: 2026-06-24, 11:59 UTC**

### Observations

**Step 1: Providers → Ollama Cloud — PASS**
- Successfully navigated to Settings → Intelligence → Providers → Ollama Cloud
- Verified: Connection section visible with "Not connected" status and "Connect with BYOK" button
- Verified: Endpoint section visible with default URL placeholder
- Screenshot: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_8da09a47-c9ae-4049-b85c-db835a76c82a.jpg

**Step 2: Set Endpoint and Save — PASS**
- Tapped on endpoint field and replaced default URL with `http://localhost:11434/api/chat`
- Tapped Save button
- Verified: Endpoint persisted and "Reset to Default" button appeared
- Verified: Endpoint field shows `http://localhost:11434/api/chat`
- Screenshot: /var/folders/bl/w2vvyf7n0sq2vrh10pg8bd4h0000gn/T/screenshot_optimized_f780ed76-27ef-4a76-affd-129bfccaa400.jpg

**Step 3: Check Available Models — BLOCKED**
- No "Check Available Models" button found on the Ollama configuration screen
- Connection section only shows "Not connected" and "Connect with BYOK" / "Save Manual Key" options
- Skipped due to lack of button and time constraints

**Step 4: Select Model in Models Settings — BLOCKED**
- Settings → Intelligence shows only "Agent" and "Providers" options
- "Models" option expected per Step 4 and code (AIModelsSettingsView.swift exists in codebase)
- Attempted multiple scroll/navigation paths: no Models section visible in UI
- Code review confirms AIModelsSettingsView should be present (SettingsView.swift lines 113-122)
- Possible issues: stale build, conditional rendering, or incomplete implementation

**Step 5: Back Out — NOT REACHED**
- Could not complete due to blocker on Step 4

### Acceptance Criteria Status
- ✓ The Ollama endpoint can be set and persists
- ✗ A model count is retrievable (no method visible in UI)
- ✗ deepseek-v4-flash:cloud can be selected as agent model (Models section not found)
- ✗ The status reflects configured state (Ollama shows "Not set up" in Providers, not "Connected")

### Root Cause Analysis
The "Models" settings section referenced in Step 4 and implemented in code (AIModelsSettingsView) is not appearing in the Settings UI. Despite confirming the file exists in the iOS codebase at `App/Sources/Features/Settings/AI/AIModelsSettingsView.swift`, the SettingsView hierarchy doesn't expose it in the current running build. This could indicate:
1. A build/compilation issue with the settings hierarchy
2. Conditional rendering that filters out the Models section
3. Incomplete feature toggle or build configuration
