---
title: Transcript Ingest Service
slug: transcript-ingest-service
summary: The transcript ingest service calls nmp_app_podcast_transcript_report after completing STT, ensuring that Rust owns the transcript state
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-31
updated: 2026-05-31
verified: 2026-05-31
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
---

# Transcript Ingest Service

## Transcript Ingest Service

The transcript ingest service calls nmp_app_podcast_transcript_report after completing STT, ensuring that Rust owns the transcript state. The toEpisode function derives transcriptState from the Rust-projected transcript and transcript_url fields rather than relying on preserved state. [^14943-113]

## See Also

