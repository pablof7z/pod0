# Scenario I1: Configure AI providers and model roles

## Goal
Validate the Providers and Models settings: connecting providers and assigning
per-role provider/model selections (agent = Ollama + `deepseek-v4-flash:cloud`).

## Prerequisites
- App past onboarding. For live validation, a reachable Ollama (G1).

## Steps
1. Settings → Intelligence → **Providers** (`ai-provider-openrouter` is one row).
   **Expected:** A BYOK Vault hero, a Connections list (OpenRouter, ElevenLabs,
   AssemblyAI, Perplexity, Ollama Cloud, Local, YouTube Ingestion), and a Usage row.
   Each shows a status ("Not set up" / "Connected" / etc.). *Screenshot.*
2. Open **Ollama Cloud** and configure it (see G1). **Expected:** Status flips to
   connected/configured. *Screenshot.*
3. Back to Providers → **Models**. **Expected:** Per-role provider+model pickers
   (e.g., Agent/Thinking/Fast/Speech). *Screenshot.*
4. Set the agent role to Ollama + `deepseek-v4-flash:cloud`. **Expected:** Persists.
   *Screenshot.*
5. Check the **Local** provider row. **Expected:** Status "No models" / "N models"
   depending on on-device downloads. *Screenshot.*

## Acceptance Criteria
- The Providers list shows all providers with accurate status labels.
- A provider can be connected and its status updates.
- Models lets you assign provider+model per role and it persists.
- The BYOK Vault hero is present (multi-provider approval entry point).

## Known Issues / Watch Points
- BYOK opens `byok.f7z.io` for consent — a full BYOK flow needs that web step; for
  local testing, manual keys per provider are simpler.
- Per MEMORY (project_local_model_provider): Local is a per-role provider; on-device
  engine-load may still be unwired (inference can return "not loaded"). Note if so.

## Notes
