---
type: episode-card
date: 2026-06-14
session: c1691db0-d63e-4062-adad-1cfa0d679d09
transcript: /Users/pablofernandez/.claude/projects/-Users-pablofernandez-Work-podcast-player/c1691db0-d63e-4062-adad-1cfa0d679d09.jsonl
salience: product
status: active
subjects:
  - android-edit-profile
  - publish-profile-seam
  - android-identity-parity
supersedes: []
related_claims: []
source_lines:
  - 11246-11260
  - 11315-11338
captured_at: 2026-06-14T04:04:34Z
---

# Episode: Android EditProfile (kind:0) on existing kernel seam — iOS parity

## Prior State

Android's `IdentityScreen.kt` had zero profile-edit UI — only a read-only display-name label (`IdentityScreen.kt:126`) and a `ModeBadge`. iOS has a full `EditProfileView` (display name, username, about, picture, sign+publish kind:0). The kernel seam for `SocialAction::PublishProfile` already exists and is tested in Rust; iOS reaches it via the generic dispatch seam (`namespace:"podcast.social", {"op":"publish_profile",...}`); Android already has `KernelBridge.dispatchAction(namespace, payloadJson)` for `podcast.identity`.

## Trigger

Cycle-16 planner identified Android EditProfile as the highest-conviction parity gap — an existing kernel seam with no Rust/JNI/contract risk, just Compose screens wired to the same dispatch path.

## Decision

Add Compose `EditProfileScreen` + `IdentityActions.publishProfile()` helper that builds the `{"op":"publish_profile",...}` payload on the existing `podcast.social` dispatch seam. No Rust or JNI changes. Form has 4 fields with char limits matching iOS (displayName=48, name=32, about=280 with counter at ≤50). Blank optionals are omitted (matching `skip_serializing_if = "Option::is_none"`), not sent as empty strings. `about` field is write-with-local-cache via SharedPreferences (matching iOS UserDefaults `kind0CachePrefix`) since `AccountSummary` doesn't project it.

## Consequences

- Android/iOS feature parity for kind:0 profile editing with zero Rust/contract risk
- Wire contract independently verified: Kotlin payload keys match Rust `SocialAction::PublishProfile` field names exactly
- 10 unit tests pin the wire shape (op discriminator, required `name`, omitted blank optionals)
- Publishing a profile does not locally apply the change to `AccountSummary` — both shells rely on local cache for immediate display; a future 'kind:0 self-echo observer' would be the durable fix

## Open Tail

- `handle_publish_profile` signs+queues the kind:0 event but never locally applies it to `IdentityStore` — published profile changes don't reflect in `AccountSummary` until a relay echo (pre-existing, affects both shells)
- Android NIP-46 bunker/nostrconnect remote signing is the next parity gap (requires new JNI bindings — not a pure Compose slice)

## Evidence

- transcript lines 11246-11260
- transcript lines 11315-11338

