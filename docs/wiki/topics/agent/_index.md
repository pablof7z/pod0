# Agent Wiki Index

> Embedded agent runtime, tool surface, voice loop, and Nostr command safety.

Last updated: 2026-05-09

## Statistics

- Sources: 3 raw notes
- Articles: 13 compiled wiki articles
- Inventory records: 0 tracked items
- Datasets: 0 manifests
- Outputs: 0 generated artifacts
- Last compiled: 2026-05-09
- Last lint: never

## Quick Navigation

- [All Sources](raw/_index.md)
- [Concepts](wiki/concepts/_index.md)
- [Topics](wiki/topics/_index.md)
- [References](wiki/references/_index.md)
- [Outputs](output/_index.md)

## Contents

| File | Summary | Tags | Updated |
|------|---------|------|---------|
| [agent-runtime-and-context.md](wiki/topics/agent-runtime-and-context.md) | Prompt and runtime strategy for an agent that works over a large podcast library. | agent, context | 2026-05-09 |
| [agent-tool-platform.md](wiki/topics/agent-tool-platform.md) | Full platform design for dynamic tools, permissions, jobs, artifacts, and evals. | agent, tools, infrastructure | 2026-05-09 |
| [tool-surface.md](wiki/concepts/tool-surface.md) | The podcast-specific tool set the agent needs. | tools, agent | 2026-05-09 |
| [lifetime-tool-catalog.md](wiki/concepts/lifetime-tool-catalog.md) | Full lifetime tool families across the podcast product. | agent, tools, catalog | 2026-05-09 |
| [tenex-delegate-tool.md](wiki/concepts/tenex-delegate-tool.md) | TENEX-compatible async delegation contract for the agent. | agent, tenex, delegation | 2026-05-09 |
| [tool-execution-infrastructure.md](wiki/concepts/tool-execution-infrastructure.md) | Gateway, registry, execution context, and result envelope design. | agent, infrastructure | 2026-05-09 |
| [tool-permissions-and-approvals.md](wiki/concepts/tool-permissions-and-approvals.md) | Permission classes and approval gates for tool calls. | agent, permissions | 2026-05-09 |
| [background-agent-operations.md](wiki/concepts/background-agent-operations.md) | Durable job queue design for long-running agent work. | agent, background, jobs | 2026-05-09 |
| [voice-briefing-loop.md](wiki/concepts/voice-briefing-loop.md) | Interruptible STT/TTS conversation and generated briefing flow. | voice, briefing | 2026-05-09 |
| [nostr-command-safety.md](wiki/concepts/nostr-command-safety.md) | Safety model for Nostr-mediated agent commands. | nostr, safety | 2026-05-09 |
| [implementation-map.md](wiki/references/implementation-map.md) | Where agent-related implementation should land in the repo. | implementation, swift | 2026-05-09 |
| [tool-family-matrix.md](wiki/references/tool-family-matrix.md) | Tool families mapped to services, stores, permissions, and priority. | tools, implementation | 2026-05-09 |
| [in-episode-agent.md](wiki/concepts/in-episode-agent.md) | Context-aware one-tap voice drop from Now Playing: seek, clip, annotate, or research without leaving the player. | agent, voice, now-playing, clip | 2026-05-09 |

## Categories

- **topics**: agent-runtime-and-context.md, agent-tool-platform.md
- **concepts**: tool-surface.md, lifetime-tool-catalog.md, tenex-delegate-tool.md, tool-execution-infrastructure.md, tool-permissions-and-approvals.md, background-agent-operations.md, voice-briefing-loop.md, nostr-command-safety.md, in-episode-agent.md
- **references**: implementation-map.md, tool-family-matrix.md

## Recent Changes

- 2026-05-09: Compiled initial agent wiki from architecture, voice, project context, and template research.
- 2026-05-09: Added lifetime tool platform design.
- 2026-05-09: Removed unwanted tool families and added TENEX-compatible delegate contract.
- 2026-05-09: Updated tool surface and implementation map for concrete action-tool implementation.
