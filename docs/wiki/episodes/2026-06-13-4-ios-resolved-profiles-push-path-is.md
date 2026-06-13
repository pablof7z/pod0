---
type: episode-card
date: 2026-06-13
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: active
subjects:
  - ios-resolved-profiles
  - kernel-identity-projection
  - kernel-domain-frames
  - nostr-conversations
supersedes: []
related_claims: []
source_lines:
  - 10193-10227
captured_at: 2026-06-13T21:48:25Z
---

# Episode: iOS resolved-profiles push path is a dead loop — parity inversion vs Android

## Prior State

iOS claims Nostr profiles via ClaimNostrProfiles.swift → kernel.claimProfile() → nmp_app_claim_profile; the kernel resolves them and ships them in the top-level projections["resolved_profiles"] map. But iOS KernelDomainFrames.decode only iterates podcast.* schema keys and never reads the top-level resolved_profiles key. Both KernelIdentityProjection factory methods hardcode resolvedProfiles: [:]. The fold that would populate the cache (mergeResolvedProfiles) is called but fed empty data, so it no-ops every tick. Result: iOS conversation participants show hex/short-npub while Android (post-#439) shows real names + avatars.

## Trigger

Opus review of PR #439 found that iOS KernelIdentityProjection.from(domainFrames:) always returns resolvedProfiles: [:] (line 185) while Android's DomainFrames.kt:340-341 decodes projections["resolved_profiles"] directly. The entire claim→resolve→display pipeline is wired end-to-end on both platforms, but iOS drops the data on the floor at the decode step.

## Decision

Decode the top-level projections["resolved_profiles"] map in iOS KernelDomainFrames.decode (mirroring Android's [String: ResolvedProfile] decode), carry it onto PodcastDomainFrames, and surface it through KernelIdentityProjection.from(domainFrames:). The existing mergeResolvedProfiles consumer and setNostrProfile idempotency guard already exist — this closes the loop with no new state machine.

## Consequences

- iOS conversations will render resolved display names and avatars (parity with Android post-#439)
- Contract-sensitive: this is a top-level (non-podcast.*) projection decode requiring explicit CodingKeys for snake_case fields (display_name, picture_url) — per the ffi_decode_snakecase_contract memory lesson, a CodingKeys slip = silent keyNotFound = dropped frames
- A Rust-JSON→Swift-decoder golden fixture test is mandatory (parallel to Android's DomainFrameWireTest.kt:910-944)
- The NmpCore.h header is NOT stale (3-param nmp_app_claim_profile is correct on origin/main); the 4-param force variant was a misread of the pinned FFI rev, not a real header drift

## Open Tail

- Implementation launched as cycle-13 #1 (Swift-only worktree); Opus review + build-for-testing still pending
- The pull-path from(podcastUpdate:) also needs to surface resolvedProfiles if that struct carries the map

## Evidence

- transcript lines 10193-10227

