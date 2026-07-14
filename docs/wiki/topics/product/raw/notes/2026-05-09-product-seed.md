---
title: "Product Seed From User Prompt"
source: "Conversation on 2026-05-09"
type: notes
ingested: 2026-05-09
tags: [product, seed, podcast-player, agent]
summary: "The user wants an iOS podcast player based on the app template, with transcript ingestion, LLM-wiki-style knowledge generation, an embedded agent with RAG and generated wikis, voice STT/TTS control, Nostr communication, and a spectacular user experience."
---

# Product Seed From User Prompt

The copied prompt asked other agents to build a new podcast player from `ios-app-template`. For this wiki task, the prompt is source context only.

Durable product requirements from the prompt:

- Pull timestamped transcripts with speaker labels.
- Use ElevenLabs when publisher transcripts are unavailable.
- Generate LLM-wiki-like knowledge from podcast content.
- Give the embedded agent access to episodes, transcripts, generated wikis, and RAG.
- Use OpenRouter for embedding generation.
- Support agent communication over Nostr.
- Support STT/TTS voice orders and agent answers.
- Allow commands like playing an episode at the segment where a topic was discussed.
- Support fuzzy recall across listening history.
- Generate weekly TLDR audio briefings that can be interrupted for follow-up questions.
- Give the agent tools for knowledge lookup, online research, and UI actions.
- Make the UX a defining feature, not a thin shell around the agent.
