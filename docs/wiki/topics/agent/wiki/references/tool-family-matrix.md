---
title: "Tool Family Matrix"
category: references
sources:
  - raw/notes/2026-05-09-agent-tool-platform-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, tools, matrix, implementation]
aliases: [Tool Matrix, Tool Family Reference]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Reference matrix mapping tool families to services, stores, permission classes, and implementation priority."
---

# Tool Family Matrix

| Family | Example Tools | Primary Service | Store | Permission | Priority |
|--------|---------------|-----------------|-------|------------|----------|
| Delegation | `delegate` | `TENEXDelegationBridge` | conversation route store + audit | external delegation | near |
| Library/feed | `subscribe_to_feed`, `refresh_feed` | `LibraryService` | SwiftData + files | mutate_undoable / external_network | near |
| Playback | `play_episode_at`, `set_playback_rate`, `set_sleep_timer` | `PlaybackEngine` | AppState snapshot | attention | near |
| Queue/state | `queue_episode`, `mark_played`, `archive_episode` | `LibraryService` | SwiftData | mutate_undoable | near |
| Transcript | `request_transcription`, `get_transcript_window` | `TranscriptService` | SwiftData + artifact files | paid_provider | near |
| Retrieval | `query_transcripts`, `search_episodes`, `query_wiki` | `RAGQueryService` | sqlite-vec + wiki store | read_local | near |
| Wiki | `compile_episode_wiki`, `verify_claim`, `export_wiki_markdown` | `WikiGenerator` | WikiStorage + vectors | paid_provider | near |
| Briefing | `generate_briefing`, `synthesize_briefing_audio` | `BriefingComposer` | BriefingStorage + files | paid_provider / attention | near |
| Highlights | `create_highlight`, `create_clip_semantic`, `anchor_note` | `HighlightService` | SwiftData + clip files | mutate_undoable | near |
| External research | `perplexity_search`, `fact_check_claim` | `ResearchService` | research artifacts | external_network / paid_provider | mid |
| Social/Nostr | `send_nostr_reply`, `share_clip`, `read_episode_discussion` | `SocialService` | AppState + relay | public_or_social | mid |
| Export | `export_wiki_markdown` | `ExportService` | artifact files | sensitive_settings | mid |
| Diagnostics | `run_health_check` | `DiagnosticsService` | logs + artifacts | sensitive_settings | mid |
| Destructive | `delete_user_data`, `unsubscribe_from_feed` | app-owned flows | multiple | destructive | explicit UI only |

## Implementation Note

Near-term tools should reuse the existing protocol-dependency pattern from `PodcastAgentToolDeps`, but the final shape should split dependencies by domain. One giant dependency struct will become hard to review as the tool count grows.

## See Also

- [[lifetime-tool-catalog|Lifetime Tool Catalog]] ([Lifetime Tool Catalog](../concepts/lifetime-tool-catalog.md)) - detailed tool names.
- [[tenex-delegate-tool|TENEX Delegate Tool]] ([TENEX Delegate Tool](../concepts/tenex-delegate-tool.md)) - delegation family details.
- [[tool-permissions-and-approvals|Tool Permissions And Approvals]] ([Tool Permissions And Approvals](../concepts/tool-permissions-and-approvals.md)) - permission classes.
- [[implementation-map|Implementation Map]] ([Implementation Map](implementation-map.md)) - current repo placement.

## Sources

- [Agent tool platform source map](../../raw/notes/2026-05-09-agent-tool-platform-source-map.md)
