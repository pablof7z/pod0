---
title: "Background Agent Operations"
category: concepts
sources:
  - raw/notes/2026-05-09-agent-tool-platform-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [agent, background, jobs, lifecycle]
aliases: [Agent Job Queue, Background Tools]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Long-running agent work should run as durable jobs with progress, retry, artifacts, cancellation, and BGTaskScheduler integration."
---

# Background Agent Operations

Many lifetime tools cannot complete inside an LLM tool call. Transcription, embedding, wiki compilation, briefings, exports, and research reports need durable jobs.

## Job Types

- feed refresh and episode ingestion
- transcript discovery and transcription
- speaker resolution and correction
- transcript chunking and embedding
- wiki page compilation and verification
- concept refresh after new episodes
- generated briefing script, TTS, and audio stitching
- clip rendering and share-card generation
- Markdown and JSON exports
- model or embedding migration reindex
- scheduled topic/person tracking

## Job Record

Each job should store:

- `job_id`, type, status, priority, progress, and current phase
- input handles and output artifact handles
- requester and surface
- cost budget and provider choice
- retry count, backoff, and last error
- cancellation state
- user-visible summary
- creation, start, finish, and expiration timestamps

## Agent Interaction

Synchronous tools should enqueue jobs and return `job_id`. The agent can call `get_job_status`, `cancel_job`, or `open_job_result`. When a job finishes, the app can surface a notification, activity card, or briefing row, depending on permission and context.

## Infrastructure

- SwiftData job table for durable state.
- App Group artifact directory for generated files.
- BGTaskScheduler for feed, transcription, embedding, and exports.
- Provider broker for OpenRouter, ElevenLabs, Perplexity, and future vendors.
- Retry and quota manager shared across all paid/network jobs.

## See Also

- [[tool-execution-infrastructure|Tool Execution Infrastructure]] ([Tool Execution Infrastructure](tool-execution-infrastructure.md)) - job result envelopes.
- [[knowledge-pipeline|Knowledge Pipeline]] ([Knowledge Pipeline](../../../knowledge/wiki/topics/knowledge-pipeline.md)) - pipeline jobs.
- [[voice-briefing-loop|Voice Briefing Loop]] ([Voice Briefing Loop](voice-briefing-loop.md)) - briefing jobs used by voice mode.

## Sources

- [Agent tool platform source map](../../raw/notes/2026-05-09-agent-tool-platform-source-map.md)
