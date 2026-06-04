---
title: Rust-Owned Business Logic
slug: rust-owned-business-logic
summary: All business logic must be Rust-owned per NMP guidelines — Swift-owned business logic must be migrated to the kernel
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-01
updated: 2026-06-04
verified: 2026-06-01
compiled-from: conversation
sources:
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:4dd36f3c-199e-4d1b-9f63-2f86c41e2f2a
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
---

# Rust-Owned Business Logic

## Rust-Owned Business Logic

All business logic must be Rust-owned per NMP guidelines — Swift-owned business logic must be migrated to the kernel. The preserved-state block in AppStateStore+KernelProjection.swift is fully deleted — all episode state flows through the Rust projection. The UserIdentityStore.shared singleton is deleted — identity is owned by AppStateStore via let identity = UserIdentityStore(). The agent chat feature, including the provider enum, credential resolution, API calls, and tool loop, is fully migrated from Swift into the Rust kernel, leaving Swift only to render streamed tokens. Swift must pass only semantic data (recipient pubkey, root event ID, content, channel anchors) when dispatching to the kernel; Rust constructs all Nostr event tags (including NIP-10), coordinates, and relay logic. No Nostr signing, cryptographic, or key management logic may exist in Swift; NMP owns the active signer (local nsec or bunker) and signs all events. NIP-46 bunker pairing is handled entirely by the kernel via nmp_app_signin_bunker; Swift never holds a RemoteSigner or performs NIP-46 protocol operations. Per-podcast NIP-F4 keys (kind:10154/54) must be registered as non-active NMP accounts via nmp_app_create_new_account(make_active: false) or nmp_app_signin_nsec, so NMP signs per-podcast events without the app handling secret keys.

<!-- citations: [^14943-155] [^4dd36-11] [^c43d5-20] [^c43d5-27] -->
