---
title: Inbox Triage
slug: inbox-triage
summary: Inbox triage uses a local LLM (Ollama) to assign a priority_score, reason, and categories to each unlistened podcast episode.
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-03
updated: 2026-06-04
verified: 2026-06-03
compiled-from: conversation
sources:
  - session:6706236b-c94a-4458-aa7b-6f71098aa55b
  - session:2a627da2-be7e-41cb-968e-79e23db03c36
---

# Inbox Triage

## Overview

Inbox triage must be redesigned from first principles as an agent that prioritizes the user's inbox with full context of the user's preferences. The old per-episode approach that sent sequential LLM calls with no user context is completely removed and replaced by the agent-based batch approach. (Previously: Inbox triage used a local LLM (Ollama) to assign a priority_score, reason, and categories to each unlistened podcast episode.) The LLM endpoint for inbox triage must be read from the settings store rather than being hardcoded to localhost:11434.

<!-- citations: [^67062-1] [^67062-6] [^67062-10] -->
## Trigger & Scheduling

A 10-minute cooldown (TRIAGE_RETRY_COOLDOWN_SECS) suppresses proactive re-triggering after a pass.

<!-- citations: [^67062-2] [^2a627-1] [^67062-8] -->
## LLM Invocation & Failure Handling

Inbox triage uses the same agent identity and memory context as the chat agent by calling build_system_prompt_with_memory, with a triage-specific task instruction appended rather than a separate prompt identity; build_system_prompt_with_memory and AGENT_SYSTEM_PROMPT are moved to agent_llm.rs as pub(crate), shared between chat and triage paths. The triage tool set includes get_memory_facts, search_library, and set_episode_priorities, excluding transcripts and get_podcast_info. The triage agent call uses backend_for(store, model) routing automatically via single_turn, inheriting Ollama, OpenRouter, or LocalModelBackend dispatch with no manual client construction. chat_with_tools signature remains unchanged; run_background_agent_task is a separate wrapper that invokes a private run_tool_loop core with empty history, TRIAGE_TOOL_INSTRUCTIONS, and MAX_TRIAGE_TOOL_TURNS = 6, structurally isolating the conversation transcript from background tasks. Inbox triage sends all needy episodes in a single user message to the agent, with no chunking; only episodes since the last triage check are included, not all unlistened episodes. The triage user message includes episode_id, podcast title, episode title, and published date for each episode, and instructs the agent to score 0.0–1.0 with a one-sentence reason and categories, recording all scores in a single set_episode_priorities call. The set_episode_priorities tool takes a batch scores array of {episode_id, score, reason, categories} in a single tool call to avoid blowing the turn budget with per-episode writes. The old inbox_llm.rs is gutted to only TriageResult/TriageStatus types (conceptually renamed to triage_types.rs), dropping the per-episode triage function and TRIAGE_PREAMBLE. When a triage call fails, the app stamps a Pending placeholder in the cache with attempted_at set to the current time. After each triage agent call, reconcile_pending stamps any episode still missing a fresh Ready cache entry as Pending to prevent hot-spawn loops.

<!-- citations: [^67062-3] [^2a627-2] [^67062-7] [^67062-11] [^67062-14] -->
## Heuristic Fallback

When LLM triage is unavailable, a heuristic fallback using recency buckets (Just published, Recent, This week) is used for inbox display. On cold start (empty memory and empty listening history), the triage agent is skipped entirely and the recency heuristic is used as fallback.

<!-- citations: [^67062-4] [^67062-13] -->
## See Also

