---
title: Agent Memory Compilation
slug: agent-memory-compilation
topic: agent-system
summary: The memory compilation model is invoked after every agent turn that reaches a final response without pending tool calls, but short-circuits (is a no-op) when ac
tags:
  - capture
volatility: warm
confidence: medium
created: 2026-06-06
updated: 2026-06-14
verified: 2026-06-06
compiled-from: conversation
sources:
  - session:6c13924f-853a-4fae-a7f9-298f3723c56c
  - session:c1691db0-d63e-4062-adad-1cfa0d679d09
---

# Agent Memory Compilation

## Invocation and Short-Circuit Behavior

The memory compilation model is invoked after every agent turn that reaches a final response without pending tool calls, but short-circuits (is a no-op) when active memory IDs already match the compiled source IDs. <!-- [^6c139-1] -->

## Retirement of Agent Notes

The flat agent_notes wire field and AgentNoteSummary DTO have been retired from all three platforms (Rust, iOS, Android) and from podcast-tui, with conversations as the sole data source. <!-- [^c1691-396] -->
