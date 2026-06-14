---
title: Nostr Conversations Projection
slug: nostr-conversations-projection
topic: nostr-protocol
summary: "NIP-10 Nostr conversations are owned by the kernel via podcast.social domain and use per-domain rev bumps via Infra::bump() at the real mutation site, not manua"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-13
updated: 2026-06-14
verified: 2026-06-13
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Nostr Conversations Projection

## Nostr Conversations Projection

NIP-10 Nostr conversations are owned by the kernel via podcast.social domain and use per-domain rev bumps via Infra::bump() at the real mutation site, not manual counter fetch_add. The conversations projection groups cached kind:1 turns by root_event_id into NostrConversation-shaped projections via the podcast.social domain sidecar, using a unified trust predicate trust(pubkey) = (followed(pubkey) OR approved(pubkey)) AND NOT blocked(pubkey), composed in SocialState alongside the upstream ActiveFollowSet, with block as an absolute override even over a followed pubkey. ApprovedPeerStore is persisted per-account under the bound data dir with atomic tmp-rename writes and D6 load-empty-on-corrupt, and must NOT be cleared on account switch. The trust predicate fails closed on a poisoned ApprovedPeerStore mutex, denying all trust (returning false for every pubkey) rather than failing open. Kind:0 profile hydration for conversation participants rides the existing resolved_profiles projection rather than duplicating profile data into podcast.social, preserving single-source-of-truth.

<!-- citations: [^c1691-328] [^c1691-329] [^c1691-342] [^c1691-359] [^c1691-405] -->
## Android Conversation Screens

Android conversation screens consume the podcast.social domain frame for conversations (NostrConversationsScreen + NostrConversationDetailScreen), using NostrConversationDto with @SerialName annotations for all snake_case fields (no auto-conversion on Android) and profile names/avatars populated by the kernel resolved_profiles projection via claimProfile/releaseProfile JNI. NostrConversationsScreen uses DisposableEffect with distinct consumer IDs per screen to manage claim/release lifecycle, preventing over-release or leaks during list-to-detail navigation.

<!-- citations: [^c1691-387] [^c1691-330] [^c1691-343] [^c1691-374] [^c1691-406] -->
## Retired: Agent Notes Wire Field

The flat agent_notes wire field and AgentNoteSummary DTO are retired from all three platforms (Rust, iOS, Android). The podcast-tui crate must also be migrated since it is a full workspace member that imports the deleted type. The canonical path for agent notes is now nostr_conversations via the podcast.social domain projection. The AgentNotesView rendering local personal Note/NoteKind from store.activeNotes is NOT retired — only the flat agent_notes wire field + composite.agentNotes is the deletion target. SocialState.agent_notes Slot is kept (it feeds nostr_conversations_snapshot) but the projection is gone.

<!-- citations: [^c1691-360] [^c1691-386] [^c1691-449] -->

## iOS Resolved-Profiles Bug

The iOS resolved-profiles push path was a complete no-op: KernelIdentityProjection.from(domainFrames:) hardcoded resolvedProfiles to an empty dictionary, so conversation participants displayed raw hex pubkeys instead of real names, unlike Android which showed real names via the kernel resolved_profiles projection. <!-- [^c1691-450] -->
