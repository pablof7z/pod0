---
title: NMP Relay Configuration
slug: nmp-relay-configuration
summary: NMP v0.2.1 allows apps to configure app-level relays (indexer and content roles) via set_initial_relays_for_start
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
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
---

# NMP Relay Configuration

## Relay Configuration

Relay configuration must be provided to the NMP app builder at startup, not at publish or subscription time. NMP v0.2.1 allows apps to configure app-level relays (indexer and content roles) via set_initial_relays_for_start. The UI must allow users to configure app relays, their own NIP-65 relays, and agent NIP-65 relays. The podcast app must not specify relay URLs at publish or subscribe time; it must configure relays at startup and let NMP drive all relay routing via PublishTarget::Auto. Nostr publishing for events signed with the user's key (kind:10064, kind:1111, kind:1) must use NMP's PublishRaw dispatch so NMP handles signing and relay routing without the app touching secrets. Per-podcast events (kind:10154/54) are pre-signed in Rust with per-podcast keys and dispatched via nmp.publish with PublishTarget::Auto until NMP exposes a sign-as-non-active-account API. Nostr subscriptions for comments (kind:1111) and agent notes (kind:1) must use NMP's push_interest and KernelEventObserver pattern instead of iOS WebSocket capability connections. The Swift/iOS layer must not contain any WebSocket awareness for Nostr relay communication; all relay connectivity is NMP's responsibility. Event publishing must use PublishTarget::Auto via nmp_app_dispatch_action("nmp.publish", ...) so NMP routes through configured relays with zero relay URLs specified by the podcast app. dispatch_nostr_relay must read write-capable relays from NmpApp::configured_relays_handle() instead of hardcoding wss://relay.primal.net.

<!-- citations: [^14943-150] [^c43d5-6] [^c43d5-9] [^c43d5-11] [^c43d5-14] -->
