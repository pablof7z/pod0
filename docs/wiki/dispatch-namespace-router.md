---
title: Dispatch Namespace Router
slug: dispatch-namespace-router
topic: agent-system
summary: The dispatch namespace router (PR #375) replaced the try-parse cascade with a 19-arm namespaced-envelope router and a helper `dispatch_host_op(ns, action, corre
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-12
updated: 2026-06-14
verified: 2026-06-12
compiled-from: conversation
sources:
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
  - session:rollout-2026-05-25T12-50-00-019e5e8a-9307-7903-9302-dbc867f91c61
---

# Dispatch Namespace Router

## Dispatch Namespace Router

The dispatch namespace router (PR #375) replaced the try-parse cascade with a 19-arm namespaced-envelope router and a helper `dispatch_host_op(ns, action, correlation_id)`, killing 5 silent action hijacks including the wiki/knowledge collision (where `WikiAction::Search{query}` and `KnowledgeAction::Search{query}` share an identical wire shape and wiki is tried first in dispatch, silently hijacking `podcast.knowledge.search`), with per-collision regression tests and no iOS/Android shell code relying on the buggy routing. The design carries the namespace through dispatch (wrapping as `{ns, action}`, matching on `ns`) to kill the wiki/knowledge collision and the broader class of silent misroutes. The `conversation_history` tool is routed through `AgentTools.dispatch` at the top level, not through `dispatchPodcast`. Social publishing routes through `nmp_dispatch` with `target: Auto` (NMP's pool-aware routing), not a hardcoded `relay.primal.net`; the doc comment in `host_op_publish.rs` stating otherwise is stale. ActionResultsRegistry mirrors SignedEventsRegistry with buffered-before-await handling under NSLock and drain-once semantics, so a result frame arriving between dispatch and await registration is consumed without loss. On iOS, the push path uses pull as the cold-start/fallback so a broken domain envelope degrades to old pull behavior rather than blanking the UI. The kernel kind:1 auto-responder uses complete_for_role for LLM replies, deduplicates via a bounded RespondedIds ring (cap 4096, evict-oldest), caps turns per root at 10, and suppresses replies on the wtd-end tag. The RespondedIds ring is a global/account-agnostic dedup store (dedup by globally-unique event-id), which is fail-safe because cross-account carryover can only suppress a reply, never cause an over-reply.

<!-- citations: [^c1691-220] [^rollo-170] [^c1691-38] [^c1691-51] [^c1691-66] [^c1691-86] [^c1691-98] [^c1691-112] [^c1691-204] [^c1691-266] [^c1691-352] -->
