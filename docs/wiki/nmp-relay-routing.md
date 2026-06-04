---
title: NMP Relay Routing
slug: nmp-relay-routing
summary: All Nostr relay communication is routed through NMP's relay pool — the iOS shell never opens URLSessionWebSocketTask connections to Nostr relays
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

# NMP Relay Routing

## Relay Routing Architecture

All Nostr relay communication is routed through NMP's relay pool — the iOS shell never opens URLSessionWebSocketTask connections to Nostr relays. NMP owns the relay pool and all WebSocket connections; no iOS URLSessionWebSocketTask connections to Nostr relays are permitted. The podcast app never specifies relay URLs at publish or subscribe time — it sets app relays once at startup and lets NMP drive all routing.

<!-- citations: [^c43d5-22] [^c43d5-15] -->
## Publish Operations

All publish operations for user-identity-key-signed events must use PublishRaw dispatched to NMP, which signs with the active signer and routes via PublishTarget::Auto. This covers kind:10064 author claims, kind:1111 comments, kind:1 agent notes, and kind:0/9802 social events — no explicit relay URLs and no Rust secret access needed. Feedback publishing routes through the kernel like FeedbackStore instead of opening WebSocket connections to relay.tenex.chat from Swift. Profile photos, agent artwork, and shake-feedback uploads that require Blossom auth (kind:24242 signed event for an HTTP Authorization header) must throw 'unavailable' until the kernel exposes nmp_app_sign_event_for_return for in-process signing. Per-podcast NIP-F4 events (kind:10154/54) signed with non-active per-podcast keys must be registered as non-active NMP accounts (nmp_app_create_new_account with make_active:false) so NMP can sign and publish them, eliminating app-Rust nostr-crate signing.

<!-- citations: [^c43d5-23] [^c43d5-33] [^c43d5-16] -->
## Subscribe Operations

All subscribe operations must use NMP's EnsureInterest/push_interest with KernelEventObservers, not iOS WebSocket connections. Comments (kind:1111), agent notes (kind:1), kind:0 profiles, and kind:10154 show discovery all route through NMP's relay pool without specifying relay URLs. Comments fetching dispatches podcast.fetch_comments to the kernel (which uses push_interest + CommentsObserver) instead of calling NostrCommentService via WebSocket.

<!-- citations: [^c43d5-24] [^c43d5-34] [^c43d5-17] -->
## Profile and Auth Operations

NMP is the sole signer for all events; Swift holds no Nostr signing code — no RemoteSigner, no LocalKeySigner, no NostrKeyPair, no schnorr/secp256k1/P256K, and no Nip46/ directory. Profile fetching uses the kernel's claimProfile action (nmp_app_claim_profile) via EnsureInterest kind:0 instead of Swift WebSocket connections. NIP-46 nostrConnect pairing is handled entirely by the kernel via nmp_app_signin_bunker; Swift dispatches the URI and receives handshake state reactively via the identity projection. Swift must never hold a RemoteSigner or manage NIP-46 WebSocket connections.

<!-- citations: [^c43d5-25] [^c43d5-35] [^c43d5-18] -->
## See Also

