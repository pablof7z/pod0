---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: root-cause
status: active
subjects:
  - ios-resolved-profiles
  - swift-decode
  - coding-keys-hazard
supersedes:
  - 2026-06-13-1-ios-resolved-profiles-dead-loop-codingkeys
related_claims: []
source_lines:
  - 10345-10347
captured_at: 2026-06-14T00:26:33Z
---

# Episode: iOS ResolvedProfile: snake_case CodingKeys + .convertFromSnakeCase silently drops all profiles

## Prior State

iOS conversation participants showed no names/avatars (Android parity missing); ResolvedProfile had explicit snake_case CodingKeys mapping display → display_name

## Trigger

Diagnosis found the #371 hazard: explicit CodingKeys with snake_case property names under a .convertFromSnakeCase decoder causes keyNotFound — every profile silently dropped because the decoder looks for the already-decoded key name

## Decision

Removed the CodingKeys entirely, renamed property to displayName (matching the camelCase JSON key directly), decoded the top-level resolved_profiles key, made merge additive (preserves activeAccount), handled resolved_profiles arriving on any tick. 4 tests mirroring Android

## Consequences

- iOS conversation participants now show real names/avatars (Android parity achieved)
- The snake_case CodingKeys + .convertFromSnakeCase interaction is a reusable hazard pattern for Swift/Codable
- Additive merge prevents activeAccount loss on subsequent ticks

## Open Tail

*(none)*

## Evidence

- transcript lines 10345-10347

