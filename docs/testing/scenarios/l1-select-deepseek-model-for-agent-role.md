# Scenario L1: Select `deepseek-v4-flash:cloud` for the Agent role in Models

## Goal
Drive the model-selection flow that G1 found BLOCKED ("Models section missing"):
navigate Settings → Intelligence → **Models**, open the **Agent (Initial)** role,
and select `deepseek-v4-flash:cloud` via the Ollama provider. Confirm the selection
persists. The Models row IS defined (SettingsView.swift intelligenceSection, the
"Models" row → `AIModelsSettingsView`); G1 likely hit a stale build or a popup-menu
picker that didn't open.

## Prerequisites
- App past onboarding.
- Ollama provider configured and reachable so its model list is populated (G1):
  endpoint `http://localhost:11434/api/chat`, or Ollama Cloud connected via BYOK.
  For the model to be selectable from the live list, `deepseek-v4-flash:cloud` must
  be retrievable; if the live list is empty you can still use the custom model ID
  field (see step 4).
- Post-#617 build (pickers use `.pickerStyle(.navigationLink)`, not popup menus).

## Steps
1. Settings → Intelligence. **Expected:** rows include **Agent**, **Providers**,
   **Models** (icon "slider.horizontal.3", purple, value = current model short
   name), **Transcripts**. If **Models** is absent, you are on a stale build —
   rebuild/reinstall (MEMORY: shared simulator build clobber) and retry. *Screenshot.*
2. Tap **Models**. **Expected:** `AIModelsSettingsView` with a **Language Roles**
   section listing **Agent (Initial)**, **Agent (Thinking)**, Memory Compilation,
   Categorization, Chapter Compilation, Embeddings; plus **Speech & Media** and
   **Retrieval** sections. *Screenshot.*
3. Tap the **Agent (Initial)** row (accessibilityLabel reads
   "Agent (Initial), <model>, <provider>"). **Expected:** a
   `ProviderModelSelectorView` pushes: capability filter chips, a sort picker, a
   provider filter menu (with counts), and a Models list. *Screenshot.*
4. Filter to the **Ollama** provider (provider filter menu) and find
   `deepseek-v4-flash:cloud` in the list; tap it → `ProviderModelDetailView` → tap
   **"Select for Agent (Initial)"**. If the live list lacks it, use the **Custom
   model ID** field: enter `deepseek-v4-flash:cloud` and tap **"Use custom ID"**.
   *Screenshot.*
5. Pop back to the Models list. **Expected:** the **Agent (Initial)** row now shows
   `deepseek-v4-flash:cloud` (Ollama). Pop to Settings → Intelligence; the Models
   row value reflects the new model. *Screenshot.*
6. Force-quit and relaunch (without a data wipe). Re-open Models. **Expected:** the
   Agent role still shows `deepseek-v4-flash:cloud` (persisted). *Screenshot.*

## Acceptance Criteria
- The **Models** row is present in Settings → Intelligence and opens
  `AIModelsSettingsView`.
- The **Agent (Initial)** role can be set to `deepseek-v4-flash:cloud` (via the
  live Ollama list or the custom model ID field).
- The selection is reflected in the role row and the Models settings value, and
  persists across a relaunch.

## Known Issues / Watch Points
- G1 root-caused the missing Models section to a stale/partial build — verify the
  running build is current (check version vs `git log`) BEFORE declaring the row
  missing.
- If the provider model list is empty (Ollama unreachable), the model can still be
  pinned via the Custom model ID field — exercise that fallback and note it.
- The Agent uses two roles (Initial + Thinking). This scenario sets **Initial**;
  if a Thinking model is required for tool-use, set it the same way and note it.
- Don't confuse the **Speech** role (Whisper, K5) with the **Agent** role (LLM).

## Notes
