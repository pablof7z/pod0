---
title: App Launch Services
slug: app-launch-services
topic: project-setup
summary: "The app connects to Nostr relays once and lets NMP drive all relay routing automatically; the app must not specify relay URLs such as wss://relay.tenex.chat at"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-03
updated: 2026-06-13
verified: 2026-06-03
compiled-from: conversation
sources:
  - session:6706236b-c94a-4458-aa7b-6f71098aa55b
  - session:rollout-2026-05-17T17-57-58-019e3671-a863-7ab1-a96d-6ceb8b541971
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# App Launch Services

## App Launch Services

The app connects to Nostr relays once and lets NMP drive all relay routing automatically; the app must not specify relay URLs such as wss://relay.tenex.chat at publish or subscribe time. (Previously: iCloudSyncCapability and NostrRelayService (connecting to wss://relay.tenex.chat) start on app launch, superseded — see nostr-rust-ffi.) Subscribing to Nostr events uses EnsureInterest/push_interest with ViewDependencies; NMP routes through configured relays automatically, eliminating the need to wait for a connected relay in the NDK pool before issuing subscriptions. (Previously: The app must wait for at least one connected relay in the NDK pool before issuing discovery subscriptions, superseded — see nostr-rust-ffi.) Relay configuration persistence across app restarts is already shipped (commit 0dcf9680): load from `.nmp-relay-config.json` via `ffi/data_dir.rs:112` overriding the register seed, save via `ffi/relay_persist.rs`, with the BACKLOG entry and the register.rs comment block corrected from stale 'no persistence' claims.

<!-- citations: [^67062-2] [^rollo-163] [^c1691-190] [^c1691-277] -->
