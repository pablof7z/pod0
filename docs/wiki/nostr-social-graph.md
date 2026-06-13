---
title: Nostr Social Graph
slug: nostr-social-graph
topic: nostr-protocol
summary: The social graph replaced the one-shot 8s-timeout `subscribe_until_eose` pull path (hardcoded `relay.primal.net`) with the reactive `FollowListProjection` ridin
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-12
updated: 2026-06-13
verified: 2026-06-12
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Nostr Social Graph

## Social Graph Adoption

The social graph replaced the one-shot 8s-timeout `subscribe_until_eose` pull path (hardcoded `relay.primal.net`) with the reactive `FollowListProjection` riding the standing `account_profile_interest` subscription (kind:0+3+10002) with no extra relay subscription or polling. FetchContacts is now just a refresh trigger returning refreshed/pending.

Trust is computed live at projection time as `(followed || approved) && !blocked`, with block as an absolute override — a followed-then-blocked pubkey returns untrusted. The `ActiveFollowSet` from the upstream `nmp-nip02` crate is composed (not forked) inside `SocialState` alongside `ApprovedPeerStore` to form the unified trust predicate, and both the responder gate and the social projection consume the same composed predicate from `SocialState`.

On account switch, `clear_for_account_switch()` clears `social_slot` (set to `None`) and `agent_notes` (cleared) so no cross-account state leaks from A into B's session. Approved peers persist per data-dir (per-account, stored as `approved-peers.json` with atomic tmp-rename) and are NOT cleared on account switch (unlike session-scoped social state); they reload from the new account's data dir on switch.

Nostr conversations are a kernel-owned podcast.social domain projection grouping inbound AgentNoteSummary and OutboundTurn entries by root_event_id, with trusted computed live at projection time and outbound turns captured at publish time via handle_publish_agent_note. The `OutboundTurnCache` is durable (bounded ring at MAX=200, dedup by event_id, atomic tmp-rename write for crash safety) and loaded into `SocialState.outbound_turns` slot on init via `data_dir.rs`.

The `SocialAction` enum includes `ApprovePeer`, `BlockPeer`, `RemoveApproval`, and `RemoveBlock` variants dispatched through `podcast.social`; the handler calls `state.social.infra.bump()` (the real per-domain re-emit site) to flip the trusted verdict.

The `ApprovedPeerStore` mutex fails closed: if poisoned, the trust predicate returns false for every pubkey (deny-all), and the responder gate also denies auto-reply.

Android decodes the podcast.social domain frame into NostrConversationDto but renders nothing — zero social/conversations/friends Composables exist; iOS has the complete slice.

Social publishing routes through `nmp_dispatch.rs` with `target: Auto` (NMP pool-aware), not a hardcoded relay — the `social-publish-relay-target` BACKLOG item's premise was partially wrong.

<!-- citations: [^c1691-167] [^c1691-168] [^c1691-169] [^c1691-186] [^c1691-198] [^c1691-213] [^c1691-226] [^c1691-234] [^c1691-246] [^c1691-257] [^c1691-270] [^c1691-290] -->
