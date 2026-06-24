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
