---
title: "Transcript Source Ladder"
category: concepts
sources:
  - raw/notes/2026-05-09-knowledge-source-map.md
created: 2026-05-09
updated: 2026-05-09
tags: [transcripts, elevenlabs, scribe, podcasting-2]
aliases: [Transcript Ingestion Strategy]
confidence: medium
volatility: warm
verified: 2026-05-09
summary: "Transcript ingestion should prefer publisher transcript metadata, then use ElevenLabs Scribe for timestamped diarized fallback, with on-device transcription reserved for privacy mode."
---

# Transcript Source Ladder

The transcript source ladder orders sources by usefulness, cost, and trust.

## Priority Order

1. Publisher transcript in the feed, especially Podcasting 2.0 JSON or VTT.
2. Publisher SRT when timestamped enough to preserve episode spans.
3. ElevenLabs Scribe fallback for missing usable transcripts.
4. Other cloud providers behind a `TranscriptionProvider` protocol if Scribe cost or quality fails.
5. On-device SpeechAnalyzer or local transcription for privacy mode, with clear limits around diarization.

HTML or plain-text transcripts are useful for display, but weak for timestamp jump and should generally trigger fallback if the product needs segment-level retrieval.

## Internal Contract

All sources normalize to the same internal transcript model:

- episode id
- language
- transcript source and model
- speakers
- ordered segments
- segment start and end
- text
- optional words
- confidence

The normalized model is the source of truth for chunking, wiki compilation, timestamp chips, and clip creation.

## See Also

- [[knowledge-pipeline|Knowledge Pipeline]] ([Knowledge Pipeline](../topics/knowledge-pipeline.md)) - where transcript ingestion sits.
- [[retrieval-and-citation-model|Retrieval And Citation Model]] ([Retrieval And Citation Model](retrieval-and-citation-model.md)) - why timestamp preservation matters.

## Sources

- [Knowledge source map](../../raw/notes/2026-05-09-knowledge-source-map.md)
