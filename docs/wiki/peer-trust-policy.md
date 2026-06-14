---
title: Peer Trust Policy
slug: peer-trust-policy
topic: nostr-protocol
summary: The trust predicate for Nostr conversations is (followed || approved) && !blocked, with block as an absolute kill-switch override.
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-14
updated: 2026-06-14
verified: 2026-06-14
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Peer Trust Policy

## Trust Policy

The trust predicate for Nostr conversations is (followed || approved) && !blocked, with block as an absolute kill-switch override.

<!-- citations: [^c1691-422] [^c1691-440] [^c1691-457] -->
## ApprovedPeerStore

ApprovedPeerStore is a kernel-owned, per-account, disk-persisted allow-list + block-list of hex pubkeys persisted under the bound data directory with atomic tmp-rename writes and load-empty-on-corrupt semantics. Approved peers persist per data-dir and must not be routed through clear_for_account_switch.

<!-- citations: [^c1691-423] [^c1691-441] [^c1691-458] -->
## ActiveFollowSet

ActiveFollowSet is upstream (nmp-nip02) and must not be forked; the trust predicate composes ActiveFollowSet with ApprovedPeerStore inside SocialState, never forking the upstream NMP crate.

<!-- citations: [^c1691-424] [^c1691-443] -->
## Approve/Block Actions

The approve/block action MUST call state.social.infra.bump() at the real action site (never a test-only fetch_add). <!-- [^c1691-425] -->


ApprovePeer/BlockPeer/RemoveApproval/RemoveBlock are SocialAction variants routed through the existing podcast.social arm, where Approve clears any block for that hex and vice-versa. The approve/block action MUST call state.social.infra.bump() at the real action site (never a test-only fetch_add). <!-- [^c1691-442] -->
## Profile Hydration

kind:0 profile hydration rides the existing resolved_profiles seam and must NOT be duplicated into podcast.social. <!-- [^c1691-426] -->

## Dead Scaffolding Removal

The orphaned nostrPendingApprovals/NostrPendingApproval/NostrApprovalPresenter must be deleted in v1 because nothing populates the queue and leaving dead approval scaffolding contradicts the no-scaffold mandate. <!-- [^c1691-427] -->
