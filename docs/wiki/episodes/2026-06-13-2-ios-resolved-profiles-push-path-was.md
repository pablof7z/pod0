---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: superseded
subjects:
  - ios-resolved-profiles
  - kernel-domain-frames
  - codingkeys-snakecase-hazard
supersedes: []
related_claims: []
source_lines:
  - 10182-10208
  - 10345-10347
  - 10427-10440
captured_at: 2026-06-13T22:49:46Z
---

# Episode: iOS resolved-profiles push path was a dead loop — now decodes kernel data

## Prior State

iOS KernelDomainFrames.decode never read the top-level projections["resolved_profiles"] key, so KernelIdentityProjection.from(domainFrames:) always returned resolvedProfiles: [:]. mergeResolvedProfiles([:]) was a no-op every tick. iOS conversation participants rendered hex/short-npub while Android (post-#439) showed real names and avatars.

## Trigger

Cycle-13 planner confirmed the dead loop end-to-end: the kernel resolves profiles and ships them in projections["resolved_profiles"], but iOS drops the data. Additionally, explicit CodingKeys mapping display → "display_name" causes keyNotFound under .convertFromSnakeCase (the #371 hazard).

## Decision

Decode the top-level projections["resolved_profiles"] map in KernelDomainFrames (mirroring Android's DomainFrames.kt:340-341). Remove explicit CodingKeys from ResolvedProfile (rename display → displayName to match .convertFromSnakeCase decoder). Make mergeResolvedProfiles additive (preserves activeAccount).

## Consequences

- iOS conversation participants now show real names and avatars (Android parity)
- The #371 CodingKeys snake_case hazard is fixed at source — future explicit CodingKeys under .convertFromSnakeCase will silently keyNotFound
- The push path is now the canonical snapshot-output seam for identity data
- 4 new iOS fixture tests mirror Android's DomainFrameWireTest

## Open Tail

- Monitor for other structs with explicit CodingKeys under .convertFromSnakeCase that could silently drop fields

## Evidence

- transcript lines 10182-10208
- transcript lines 10345-10347
- transcript lines 10427-10440

