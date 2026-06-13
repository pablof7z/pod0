---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: architecture
status: active
subjects:
  - agent-notes-projection
  - podcast-tui
  - nostr-conversations
supersedes:
  - 2026-06-13-3-flat-agent-notes-projection-retirement-redundant
related_claims: []
source_lines:
  - 9040-9070
  - 9175-9224
  - 9333-9365
  - 9454-9467
captured_at: 2026-06-13T19:46:29Z
---

# Episode: Retire flat agent_notes projection (subsumed by nostr_conversations)

## Prior State

AgentNoteSummary was a flat wire field on PodcastUpdate, consumed by iOS, Android, and podcast-tui. The same data was already available through the richer nostr_conversations projection.

## Trigger

The flat agent_notes projection was redundant with nostr_conversations (which feeds the conversations view with richer trust/counterparty context). Removing it reduces wire overhead and maintenance surface. Opus review then discovered podcast-tui was a live consumer that the initial PR missed (orphan grep was scoped only to nmp-app-podcast, not the whole workspace).

## Decision

Retire AgentNoteSummary and PodcastUpdate.agent_notes across all three platforms (iOS, Android, Rust TUI), migrating podcast-tui onto Vec<NostrConversationDTO>. The inbound agent_notes Slot and kind:1 transport handler are preserved (they feed the conversations projection). The golden fixture was legitimately unchanged (skip_serializing_if on empty Vec).

## Consequences

- PR #435 auto-merged with a broken cargo build --workspace because CI's Migration lint only ran -p nmp-app-podcast and missed the podcast-tui consumer
- PR #437 was required to repair main by migrating podcast-tui onto nostr_conversations
- Swift codegen drift gate caught a hand-edit mismatch in PodcastSocialTypes.generated.swift (doc-comment still referenced the retired field); fixed at emit.rs source
- Conversations projection is confirmed intact: nostr_conversations_snapshot reads directly from the agent_notes Slot, never the deleted projection method
- Durable lesson recorded: FFI-DTO removals must grep the ENTIRE workspace, not just the app crate

## Open Tail

- CI workspace-build gate (separate arc) needed to prevent this class of regression from recurring

## Evidence

- transcript lines 9040-9070
- transcript lines 9175-9224
- transcript lines 9333-9365
- transcript lines 9454-9467

