---
title: YouTube Ingestion Skill
slug: youtube-ingestion-skill
topic: agent-system
summary: "The ingest_youtube_video tool POSTs {\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"url\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\": \\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"...\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"} to the configured extractor endpoint and expects back {\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"audio_url\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\": \\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"...\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\", \\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"title\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\": \\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"...\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\", \\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"author\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\": \\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\"...\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\", \"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-05-13
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:9692d124-a1a0-411c-91f9-9d6ebc0b29b1
---

# YouTube Ingestion Skill

## YouTube Ingestion Skill

The ingest_youtube_video tool POSTs {"url": "..."} to the configured extractor endpoint and expects back {"audio_url": "...", "title": "...", "author": "...", "duration_seconds": 1234}, accepting either "audio_url" or "url" as the response field name. Its parameters are url (required), title (optional), and transcribe (optional, default true). The resulting episode is published to the "Agent Generated" podcast and optionally transcribed, making it available to the query_transcripts, summarize_episode, and generate_tts_episode agent snippet turns. <!-- [^9692d-6] -->

## YouTube Search Tool

The search_youtube tool has parameters query (required) and limit (optional, default 5, max 20). It POSTs {"search": "...", "limit": N} to the same configured extractor endpoint and expects back {"results": [{url, title, author, duration_seconds}]}, which is a custom extension beyond cobalt's standard API (cobalt does not natively support search). <!-- [^9692d-7] -->

## Configuration

The YouTube extractor URL is stored in Settings as youtubeExtractorURL (String?), configured via Settings → Providers → YouTube Ingestion. <!-- [^9692d-8] -->

## Implementation Notes

New tool adapters go in their own files rather than in LivePodcastAgentToolDeps.swift because that file was already 644 lines (over the 500-line hard limit from AGENTS.md). <!-- [^9692d-9] -->
