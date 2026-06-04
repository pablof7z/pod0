---
title: Agent System Prompt and Memory
slug: agent-system-prompt-and-memory
summary: The agent that the user chats with on the app has a system prompt (`AGENT_SYSTEM_PROMPT`) and stored MemoryFacts that are injected per turn via `build_system_pr
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
---

# Agent System Prompt and Memory

## Agent System Prompt and Memory

The agent that the user chats with on the app has a system prompt (`AGENT_SYSTEM_PROMPT`) and stored MemoryFacts that are injected per turn via `build_system_prompt_with_memory`. Both `AGENT_SYSTEM_PROMPT` and `build_system_prompt_with_memory` reside in `agent_llm.rs` as `pub(crate)` so that the chat handler and triage can share them.

<!-- citations: [^67062-9] -->
## See Also

