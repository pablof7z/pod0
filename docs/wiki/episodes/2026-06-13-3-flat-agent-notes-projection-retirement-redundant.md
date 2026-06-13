---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - agent-notes-projection
  - nostr-conversations-snapshot
  - podcast-tui
  - ffi-dto-removal
supersedes:
  - 2026-06-13-2-nip-10-threaded-conversations-replace-flat
related_claims: []
source_lines:
  - 9059-9066
  - 9175-9227
  - 9333-9361
captured_at: 2026-06-13T19:33:27Z
---

# Episode: Flat agent_notes projection retirement — redundant wire subsumed by nostr_conversations

## Prior State

A flat `agent_notes`/`AgentNoteSummary` projection existed in `PodcastUpdate`, duplicating data now carried by the canonical `nostr_conversations` projection. No iOS or Android view rendered it.

## Trigger

Conversations projection now carries the same data; the flat projection was dead wire in both mobile shells. The retirement was an explicit cycle-11 work item.

## Decision

Remove `AgentNoteSummary` DTO, `agentNotes` field from `PodcastUpdate`, and all shell-side references (iOS generated types, Android DomainFrames, Rust TUI). Keep the inbound `SocialState.agent_notes` Slot and `agent_note_handler.rs` transport intact (they feed the conversations projection).

## Consequences

- Three-platform removal complete: Rust DTO, Swift generated types, Kotlin DomainFrames, TUI migrated to `NostrConversationDTO`
- Golden fixture byte-identical (field was `skip_serializing_if = Vec::is_empty` with empty default)
- `podcast-tui` was an undiscovered live consumer — its workspace breakage revealed CI's `-p nmp-app-podcast`-scoped lint cannot catch cross-crate FFI-DTO consumers
- CI auto-merged #435 before the TUI fix was applied, breaking `cargo build --workspace` on main — required follow-up #437 to repair
- Durable lesson recorded: FFI-DTO removals must grep the entire workspace including the TUI; CI needs a workspace-build gate

## Open Tail

- CI workspace-build gate not yet implemented — tracked as a cycle-12 item

## Evidence

- transcript lines 9059-9066
- transcript lines 9175-9227
- transcript lines 9333-9361

