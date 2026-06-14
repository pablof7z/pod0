---
title: Agent Relay Bridge
slug: agent-relay-bridge
topic: agent-system
summary: "The dead legacy `reply(to:from:)` path on AgentRelayBridge, which had zero callers outside AgentRelayBridge.swift itself and did not attach peer context (causin"
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-05-13
updated: 2026-06-14
verified: 2026-05-13
compiled-from: conversation
sources:
  - session:0f3f24f7-54de-49f8-b160-a92f735f6a00
  - session:16a9893c-f4c6-486d-ade2-e290ff0ca5d9
  - session:14943b9b-5bf3-4317-bc44-298a773bc75e
  - session:c43d5e77-d667-4e71-a574-47aaab5b6a7a
  - session:6706236b-c94a-4458-aa7b-6f71098aa55b
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Agent Relay Bridge

## Legacy Reply Path Removal

The dead legacy `reply(to:from:)` path on AgentRelayBridge, which had zero callers outside AgentRelayBridge.swift itself and did not attach peer context (causing peer tools to silently error), has been deleted. <!-- [^0f3f2-8] -->

## End Conversation Tool Behavior

NostrAgentResponder.swift was deleted in PR #248, meaning agent-to-agent auto-reply is dead functionality that needs kernel restoration rather than refactoring. The kernel kind:1 auto-responder restores it: trusted inbound kind:1 triggers complete_for_role reply via handle_publish_agent_note with dedup (bounded RespondedIds ring capped at 4096 entries via a VecDeque+HashSet ring that evicts the oldest when over cap, persisting across identity switches — global/account-agnostic by design since dedup can only suppress, never over-reply), max 10 outgoing turns per root, and wtd-end conversation-termination gating.

<!-- citations: [^16a98-1] [^c1691-174] [^c1691-201] -->
## Relay Reactivity

Relay reactivity is Rust-side via a DispatchHostOp rev-bump; no optimistic local mirror is needed in Swift. <!-- [^14943-3] -->

Relay reactivity is Rust-side via a DispatchHostOp rev-bump; no optimistic local mirror is needed in Swift. Feedback publishing routes through the kernel's NMP relay pool; FeedbackRelayClient's WebSocket actor is deleted. <!-- [^c43d5-1] -->

No WebSocket awareness (URLSessionWebSocketTask, direct relay connections) belongs in Swift; all Nostr relay communication is NMP's responsibility. (Previously: The agent connects via WebSocket to wss://relay.tenex.chat using the agent key 5f6280b5d948… and subscribes to kind:1 Nostr events mentioning that key. <!--  -->, superseded — see nostr-rust-ffi.)

## Action Results Registry

ActionResultsRegistry in Swift mirrors SignedEventsRegistry with drain-once semantics and NSLock-protected buffering so a result frame arriving between dispatchSilent and awaitResult registration is not lost. <!-- [^c1691-175] -->

## Stale Comment Rewrite

The register.rs comment block at lines 233–262 was stale and contradicted the code (relay persistence via C-ABI path now exists at ffi/data_dir.rs:112 and host_op_handler/settings_actions.rs:391), and was rewritten to reflect the actual state. <!-- [^c1691-202] -->

## Orphaned Approval Scaffolding

iOS NostrPendingApprovals/NostrApprovalPresenter are orphaned dead scaffolding — nothing populates the pending queue, and the allow/block sets gate nothing in the kernel. They are deleted in the v1 conversations vertical. <!-- [^c1691-236] -->

## Agent Notes Wire Field Retirement

The flat agent_notes wire field and AgentNoteSummary DTO are retired across Rust, iOS, and Android; the conversations projection carries the data and the kind:1 transport handler is untouched.

<!-- citations: [^c1691-413] [^c1691-429] -->
