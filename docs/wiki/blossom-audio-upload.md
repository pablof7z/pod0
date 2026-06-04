---
title: Blossom Audio Upload
slug: blossom-audio-upload
summary: "Blossom audio upload is wired end-to-end: Rust base64-encodes audio, iOS HttpCapability decodes to binary Data for upload."
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-31
updated: 2026-06-04
verified: 2026-05-31
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
---

# Blossom Audio Upload

## Audio Upload

Blossom audio upload is a core requirement for NIP-F4 episode publishing — a kind:54 episode event without an audio URL is useless, not an enhancement. Blossom upload implements BUD-01 HTTP POST /upload with a kind:24242 Nostr auth event containing tags for t=upload, x=sha256, expiration, and size. If the Blossom upload fails, the system falls back to the RSS enclosure URL. HTTP binary body transport uses body_base64 field on HttpRequest — Rust emits base64-encoded audio, iOS decodes to binary Data for upload; D6 doctrine requires returning error on malformed input rather than silently sending garbage. Blossom blob uploads (profile photos, podcast artwork) must route through a kernel action that signs kind:24242 with the active signer and performs the HTTP PUT, returning the URL via snapshot; Swift does not sign or construct Nostr auth events.

<!-- citations: [^14943-101] [^14943-103] [^14943-139] [^c43d5-22] -->
