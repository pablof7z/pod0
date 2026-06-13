---
title: Kernel Auto Responder
slug: kernel-auto-responder
topic: agent-system
summary: "The kernel kind:1 auto-responder uses `llm::complete_for_role` for trusted inbound notes, deduplication via a bounded `RespondedIds` ring (`VecDeque` + `HashSet"
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

# Kernel Auto Responder

## Core Behavior

The kernel kind:1 auto-responder uses `llm::complete_for_role` for trusted inbound notes, deduplication via a bounded `RespondedIds` ring (`VecDeque` + `HashSet`, cap 4096, evicting oldest when over capacity, persisted across process restarts via an atomic tmp-rename sidecar), max outgoing turns per root of 10 (`MAX_OUTGOING_TURNS_PER_ROOT=10`), and a wtd-end tag gate to end conversations. Agent trust is computed live at projection time as (followed || approved) && !blocked, not frozen at receipt. Block is an absolute override in the trust predicate: even a followed pubkey, if explicitly blocked, is untrusted and gets no auto-reply. The approved-peer trust predicate fails closed: if the `ApprovedPeerStore` mutex is poisoned, `trust_predicate()` returns `false` for every pubkey, and the responder gate also denies auto-reply.

<!-- citations: [^c1691-232] [^c1691-242] [^c1691-253] [^c1691-283] -->

## Kernel-Dispatched Operations

The kernel auto-responder dispatches `podcast.agent` ops (`send`/`clear`) with `#[serde(tag = "op", rename_all = "snake_case")]` on `AgentChatAction`. AI chapters/ad-spans generation is kernel-owned (D0): `podcast.chapters.compile` and `podcast.settings.set_auto_skip_ads` are dispatched through the kernel, with the `overlapsAd` extension relocated to `Episode+AdOverlap.swift`. <!-- [^c1691-284] -->
