---
title: LLM Provider Credential Resolver
slug: llm-provider-credential-resolver
topic: agent-system
summary: AgentLLMClient requires a non-empty API key via LLMProviderCredentialResolver before dispatching any request, even for keyless local Ollama instances.
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-06-14
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:0f3f24f7-54de-49f8-b160-a92f735f6a00
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:6706236b-c94a-4458-aa7b-6f71098aa55b
  - session:rollout-2026-05-11T09-10-30-019e15a8-96ed-76a3-9539-607404bb9a31
---

# LLM Provider Credential Resolver

## API Key Requirements

AgentLLMClient requires a non-empty API key via LLMProviderCredentialResolver before dispatching any request, even for keyless local Ollama instances. LLMProviderCredentialResolver.requiresAPIKey(for:ollamaChatURL:) centralizes the cloud-vs-local decision: an ollama.com host requires an API key, while any other host makes the key optional. Perplexity is supported as a Provider with BYOK and manual key storage through Keychain; no raw key is stored in app settings. Provider/settings rows check actual Keychain availability where touched, rather than trusting metadata only. The `perplexity_search` agent tool schema is not hidden when no Perplexity key is present. LLM dispatch consolidation (PR #386) collapses 6 copy-pasted resolve-model → validate-credentials → dispatch blocks into complete_for_role, and 3 duplicate extract_json implementations into canonical versions in src/llm/. The LLM client is dispatched via backend_for(store, model) which routes to Ollama, OpenRouter, or LocalModelBackend based on settings — no hardcoded localhost:11434.

The auto-advance function must bail entirely if the actor lock is poisoned, rather than skipping staging while still dispatching Load+Play, because the unconditional dispatch on a poisoned lock causes episode_id to remain None and position/mark-played updates to silently drop. <!-- [^c1691-385] -->

<!-- citations: [^0f3f2-47] [^0f3f2-48] [^c1691-4] [^67062-7] [^rollo-117] -->
