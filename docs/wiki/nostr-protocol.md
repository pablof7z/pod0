---
title: Per-Podcast NIP-F4 Signing Migration
slug: nostr-protocol
topic: nostr-protocol
summary: "The per-podcast NIP-F4 signing migration routes signing through the kernel's existing AddSigner{make_active:false} + PublishRaw{signer_pubkey} + nmp.blossom.upl"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-13
updated: 2026-06-13
verified: 2026-06-13
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Per-Podcast NIP-F4 Signing Migration

## Per-Podcast NIP-F4 Signing Migration

The per-podcast NIP-F4 signing migration routes signing through the kernel's existing AddSigner{make_active:false} + PublishRaw{signer_pubkey} + nmp.blossom.upload{signer_pubkey} seams, deleting blossom.rs::upload_to_blossom and host_op_publish.rs::sign_event. <!-- [^c1691-300] -->
