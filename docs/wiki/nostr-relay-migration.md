---
title: Nostr Relay Migration
slug: nostr-relay-migration
summary: NostrRelayCapability.swift and its transport extension are dead code to be deleted — nothing dispatches to them from Rust
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-03
updated: 2026-06-04
verified: 2026-06-03
compiled-from: conversation
sources:
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
---

# Nostr Relay Migration

## Dead Code Removal

NostrRelayCapability.swift, NostrRelayService, NostrCommentService, NIP65RelayFetcher, NostrProfileFetcher, NostrThreadFetcher, NostrEventPublisher, and the Nip46/ directory (except NostrSigner.swift) are dead code and must be deleted.

<!-- citations: [^c43d5-26] [^c43d5-36] -->
## Tag and Relay Construction

Swift must not parse or construct Nostr event tags; it passes semantic values (recipient, root, inbound ID, channel anchors) and Rust constructs all NIP-10 tags. Swift does not encode default relays or event coordinate tagging logic — relay URLs come from the kernel snapshot and tag construction lives in Rust.

<!-- citations: [^c43d5-27] [^c43d5-37] -->
## Agent Responder Migration Sequence

Agent responder migration to Rust follows a 4-step sequence: (1) kill NostrEventPublisher by dispatching publish_agent_note via kernel, (2) kill NostrProfileFetcher via kind:0 EnsureInterest observer, (3) kill NostrThreadFetcher via kind:1 #e EnsureInterest one-shot, (4) move the LLM responder loop to Rust. The responder loop (inbound kind:1 notes, dedup, per-root turn cap, LLM call, outbound publish) moves from Swift to Rust in nmp-app-podcast; NostrAgentResponder.swift and NostrRelayService.swift are deleted once migration completes.

<!-- citations: [^c43d5-28] [^c43d5-38] [^c43d5-26] -->
## See Also

