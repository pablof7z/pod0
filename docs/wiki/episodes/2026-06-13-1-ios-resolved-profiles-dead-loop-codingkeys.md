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
  - codingkeys-hazard
supersedes:
  - 2026-06-13-2-ios-resolved-profiles-push-path-was
related_claims: []
source_lines:
  - 10229-10346
captured_at: 2026-06-14T00:00:10Z
---

# Episode: iOS resolved-profiles dead loop — CodingKeys + convertFromSnakeCase silently dropped all profiles

## Prior State

iOS conversation participants showed hex pubkeys instead of names/avatars. The kernel correctly resolved profiles and shipped them in projections["resolved_profiles"], but iOS's KernelDomainFrames.decode never read that top-level key, so mergeResolvedProfiles([:]) no-oped every tick. An explicit snake_case CodingKeys (display = "display_name") combined with .convertFromSnakeCase caused keyNotFound, silently dropping every profile.

## Trigger

Investigation of the confirmed live parity defect found the CodingKeys hazard — the exact #371 class of bug (explicit snake_case CodingKeys under .convertFromSnakeCase causes keyNotFound).

## Decision

Remove the explicit CodingKeys, rename properties to camelCase (displayName), decode the top-level resolved_profiles key, and make the merge additive (preserves activeAccount). The #371 hazard is fixed at the source.

## Consequences

- iOS conversation participants now show real names/avatars (Android parity with #439)
- The convertFromSnakeCase + explicit CodingKeys pattern is identified as a hazard class
- 4 iOS fixture tests mirroring Android's coverage guard the decode path

## Open Tail

*(none)*

## Evidence

- transcript lines 10229-10346

