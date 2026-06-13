---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: superseded
subjects:
  - podcast-social-domain
  - nostr-conversations-projection
  - agent-notes-to-conversations
supersedes:
  - 2026-06-13-3-conversation-projection-uses-new-socialstate-model
  - 2026-06-13-2-podcast-social-domain-replaces-misc-blob
related_claims: []
source_lines:
  - 7366-7388
  - 7419-7434
  - 7456-7480
  - 7496-7504
captured_at: 2026-06-13T02:42:14Z
---

# Episode: NIP-10 threaded conversations replace flat agent_notes via podcast.social domain

## Prior State

Agent notes were a flat list in the misc domain; Swift NostrConversationsView consumed compat-empty surfaces; misc payload included social/agent_notes fields

## Trigger

BACKLOG item nostr-conversations-real-projection. Architect found ConversationActor/NostrConversation is explicitly LLM-chat, not peer-Nostr — rejected as the model for this projection

## Decision

New podcast.social domain (8th delta sidecar) groups inbound AgentNoteSummary + OutboundTurn entries by root_event_id into NIP-10-threaded conversations, sorted newest-first, with live trusted computation against the follow set. agent_notes moved from misc to social; build_misc_payload no longer includes social/agent_notes fields

## Consequences

- Swift NostrConversationsView now binds kernel-projected conversations via projectSnapshotDerivedState
- OutboundTurnCache (bounded ring MAX=200, dedup by event_id, atomic tmp-rename write) captures sent messages
- iOS bridge maps rootEventId → rootEventID (uppercase) to avoid the #371 snake_case freeze class
- SocialDomainFrame has no explicit CodingKeys (inherits FFI snake_case contract)
- The misc→social move is a structural field migration, not a duplication — verified no orphaned reads

## Open Tail

- recordNostrTurn marked LEGACY — deletable once kernel push is confirmed live end-to-end

## Evidence

- transcript lines 7366-7388
- transcript lines 7419-7434
- transcript lines 7456-7480
- transcript lines 7496-7504

