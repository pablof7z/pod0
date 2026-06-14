---
title: Kernel Auto Responder
slug: kernel-auto-responder
topic: agent-system
summary: "The kernel kind:1 auto-responder uses `llm::complete_for_role` for trusted inbound notes"
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

# Kernel Auto Responder

## Core Behavior

The kernel kind:1 auto-responder uses `llm::complete_for_role` for trusted inbound notes. Deduplication is via a bounded `RespondedIds` ring (`VecDeque` + `HashSet`, capped at `MAX_RESPONDED_IDS=4096`, evicting oldest when over capacity, persisted across process restarts via an atomic tmp-rename sidecar). The max outgoing turns per root is 10 (`MAX_OUTGOING_TURNS_PER_ROOT=10`), with a wtd-end tag gate to end conversations. The responder loop (agent-to-agent-kind1) is already fully shipped in the kernel (`agent_note_responder.rs`: async off-actor spawn, wtd-end gate, dedup cache, owner-consult ask, trust verdict). Agent trust is computed live at projection time against the ActiveFollowSet, not frozen at receipt. The trust predicate is (followed OR approved) AND NOT blocked; block is an absolute override even over a followed pubkey (a followed pubkey that is explicitly blocked is untrusted and gets no auto-reply). The approved-peer trust predicate fails closed: if the `ApprovedPeerStore` mutex is poisoned, `trust_predicate()` returns `false` for every pubkey, and the responder gate also denies auto-reply. The responder cache is intentionally global/account-agnostic (dedup by globally-unique event-id, turn-cap by global root-id); it must persist across identity switches unlike account-scoped social state. The OutboundTurnCache is durable with a bounded ring at MAX=200, dedup by event_id, evict-oldest at capacity, and atomic tmp-rename write for crash safety.

<!-- citations: [^c1691-232] [^c1691-242] [^c1691-253] [^c1691-283] [^c1691-309] [^c1691-353] -->
## Kernel-Dispatched Operations

The kernel auto-responder dispatches `podcast.agent` ops (`send`/`clear`) with `#[serde(tag = "op", rename_all = "snake_case")]` on `AgentChatAction`. AI chapters/ad-spans generation is kernel-owned (D0): `podcast.chapters.compile` and `podcast.settings.set_auto_skip_ads` are dispatched through the kernel, with the `overlapsAd` extension relocated to `Episode+AdOverlap.swift`. <!-- [^c1691-284] -->

## Account Switching

Social slot and `agent_notes` are cleared on account switch to prevent cross-account state leakage. `ApprovedPeerStore` is per-account durable and must NOT be cleared on account switch; it reloads from the account-scoped data dir on rebind. <!-- [^c1691-310] -->

## Retired Functionality

The agent-to-agent responder is dead functionality since `NostrAgentResponder.swift` was deleted in PR #248; its restoration in the kernel is feature restoration, not refactor. The flat `agent_notes` wire field and `AgentNoteSummary` DTO are retired since the conversations projection now carries the data. `agent_note_handler.rs` (kind:1 transport) and `SocialState.agent_notes` Slot are kept as data source. <!-- [^c1691-311] -->

## Behavioral Tests

A behavioral trust test must drive observers directly to verify that following an author flips an existing note's trusted status. <!-- [^c1691-312] -->
