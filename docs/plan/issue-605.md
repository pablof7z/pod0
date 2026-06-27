# Issue 605 — Adopt NMP input-intent and open_search for Nostr-facing discovery surfaces

**Target:** v0.8.0+

**Priority:** High

**Blocked on:** #597 (NMP integration with open_search APIs)

## Goal

Route every Nostr-facing text-entry/discovery surface through NMP's framework-level input-intent classifier and open_search action. Eliminate ad-hoc Nostr parsing (NIP-05, nprofile, nevent, relay-URL detection) in native shells. Keep podcast RSS/iTunes/library/vector search in podcast-owned modules — only Nostr-protocol inputs move to the framework path.

## Context (current state)

### Nostr-facing text-entry surfaces that need changes:

1. **AddByURLForm** (App/Sources/Features/Library/AddShowSheet.swift) — RSS-URL only. Pasting npub1..., nprofile1..., or user@domain.com gets an RSS error instead of a Nostr subscribe.

2. **NostrDiscoverForm.swift** (App/Sources/Features/Library/) — client-side text filter over fetched NIP-F4 results. No relay-targeted NIP-50 search, no NIP-05 resolution.

3. **AddFriendSheet.swift** (App/Sources/Features/Settings/Agent/) — calls NostrNpub.pubkeyHex(from:) which wraps nmp_app_podcast_parse_pubkey (app-local Rust). Accepts npub/hex only; no NIP-05, no nprofile relay hints.

4. **TUI handle_subscribe_input** (apps/podcast-tui/src/input.rs ~line 121) — raw RSS URL dispatched via runtime.subscribe(&url). No Nostr detection.

5. **Android** — no Nostr text-entry subscribe surface exists yet. Phase 0 audit required.

### App-local Nostr parser to eventually replace:
- enum NostrNpub in App/Sources/Domain/NostrConversation.swift
- nmp_app_podcast_parse_pubkey / nmp_app_podcast_npub_from_hex in apps/nmp-app-podcast/src/ffi/identity_format.rs
- Currently handles npub/hex only; NIP-05 and nprofile are unhandled

### Not changing:
- iTunes catalog search (DiscoverSearchForm.swift)
- library/transcript/knowledge search (PodcastSearchView.swift / podcast.knowledge)
- RSS subscribe-by-URL (SubscriptionService)

## Implementation Plan

### Phase 0 — Pre-#597 (audit, can start now)

**Step 1:** Inspect NMP open_search API
- Read crates/nmp-core/src/ (rev 1e1445973 once #597 lands)
- Record open_search action signature, InputIntent enum variants, NIP-50 relay targeting, kind:10007 preference types
- Confirm exact types before Phase 1 begins

**Step 2:** Audit Android text-entry screens
- Check android/Podcast/app/src/main/java/io/f7z/podcast/ui/ for any Subscribe/Add-Friend/search screens
- Record scope for Phase 3

### Phase 1 — Rust kernel: podcast.open_search handler (partially scaffolded; blocked on #597 for real classifier/result behavior)

**Step 3:** Finish apps/nmp-app-podcast/src/open_search_handler.rs
- Current `main` has a structural handler/action scaffold and nsec rejection.
- Replace placeholder `pending`/`nip05_pending` responses with real NMP
  classifier/resolver/search behavior once #597's typed NMP baseline lands.
- Calls NMP input-intent classifier
- Routes by intent:
  - npub / nprofile / hex pubkey → subscribe_nostr with extracted author_pubkey_hex and relay hints from nprofile TLV
  - NIP-05 address (user@domain.com) → call NMP NIP-05 resolver → subscribe_nostr with resolved pubkey
  - NIP-50 plain-text query → dispatch relay-targeted search through NMP NIP-50; write results to nostr_search_results slot
  - Unrecognised input → return "nostr_not_recognised" error for UI fallback to RSS

**Step 4:** Keep `PodcastAction::OpenSearch` authoritative
- The variant and dispatch arm already exist on `main`.
- Refresh decode/typed-byte details after #597 lands so shells do not grow
  duplicate Nostr parsing.

**Step 5:** Complete OpenSearch dispatch semantics
- The action arm currently calls the placeholder handler.
- Replace placeholder returns with terminal results/projection updates that
  native shells can await.

**Step 6:** Optional: Add nostr_search_results projection
- If NIP-50 results need separate slot from nostr_results:
  - Add nostr_search_results: Vec<NostrSearchResult> to PodcastHandle
  - Expose in apps/nmp-app-podcast/src/ffi/projections/
  - Add Swift-codegen struct + PodcastUpdate field
  - Confirm against NMP API in Step 1

**Step 7:** Rust unit tests (apps/nmp-app-podcast/src/open_search_tests.rs)
- test_open_search_npub_routes_subscribe_nostr
- test_open_search_nip05_resolves_and_subscribes
- test_open_search_plain_text_returns_unsupported

### Phase 2 — iOS surfaces

**Step 8:** Add kernel bridge method
- File: App/Sources/Bridge/AppStateStore+KernelActions.swift
- Add: kernelNostrOpenSearch(input:) → DispatchResult
- Dispatches podcast.open_search via typed bytes (post-#597)

**Step 9:** Update AddByURLForm
- File: App/Sources/Features/Library/AddShowSheet.swift
- In submit(), before SubscriptionService.addSubscription(feedURLString:):
  - Call NostrNpub.looksLikeNostrInput(trimmed)
  - If true, dispatch kernelNostrOpenSearch and await nostrResults snapshot push
  - On "nostr_not_recognised", fall through to RSS path

**Step 10:** Add NostrNpub.looksLikeNostrInput(_:) → Bool
- File: App/Sources/Domain/NostrConversation.swift
- Pure prefix check (no FFI)
- Returns true if string starts with: npub1, nprofile1, nevent1, nsec1, or contains @ without URL scheme
- Routing guard only

**Step 11:** Update NostrDiscoverForm.swift
- Wire query field submit to dispatch kernelNostrOpenSearch(input: query) when looksLikeNostrInput() is true
- Client-side NIP-F4 filter continues unchanged for plain-text queries
- Add "Searching Nostr..." indicator during open_search dispatch

**Step 12:** Update AddFriendSheet.swift
- If NMP open_search returns pubkey synchronously for npub/hex:
  - Replace NostrNpub.pubkeyHex(from:) guard with kernelNostrOpenSearch
- If NIP-05 is async:
  - Keep nmp_app_podcast_parse_pubkey for immediate npub/hex validation
  - Add NIP-05-in-AddFriend as BACKLOG follow-up
- Either way, remove any Swift-side bech32 string manipulations

**Step 13:** Add changelog entry
- File: App/Resources/changelog/<UTC timestamp>.json
- Content: { "shipped_at": "<UTC ISO-8601>", "lines": ["Nostr identifiers and NIP-05 addresses now resolve directly in the Add Show sheet."] }

### Phase 3 — TUI and Android

**Step 14:** Update TUI handle_subscribe_input
- File: apps/podcast-tui/src/input.rs (~line 121)
- Before runtime.subscribe(&url), check for Nostr input prefix
- If match, call runtime.nostr_open_search(&input)
- Keep RSS subscribe as fallback

**Step 15:** Add TUI AppRuntime::nostr_open_search()
- File: apps/podcast-tui/src/runtime_actions.rs
- Signature: AppRuntime::nostr_open_search(&str) → Result<String>
- Dispatches { op: "open_search", input: … } to namespace "podcast" via typed bytes path

**Step 16:** Android Nostr text-entry (conditional on Phase 0 findings)
- If Phase 0 audit found a subscribe text-entry screen:
  - Apply looksLikeNostrInput check
  - Route to podcast.open_search via KernelBridge
- If none exists:
  - Add TODO #605 comment
  - Create BACKLOG entry for future work

### Phase 4 — Cleanup and docs

**Step 17:** Retire app-local Nostr parsers
- Grep App/Sources/ for "nprofile", "nevent", "_well-known/nostr"
- Confirm no app-local parsers remain outside of Rust FFI calls

**Step 18:** Update docs/plan.md
- Add row for this issue under Active Work (or move to Complete)

**Step 19:** Update docs/BACKLOG.md
- Add: "nostr-search-relay-preferences (kind:10007) UI follow-up"
- If Android gap exists: "Android Nostr subscribe input surface"

**Step 20:** Final checks
- Run git diff --check before opening PR

## Validation

```bash
# Rust tests
cargo test -p nmp-app-podcast
cargo test -p nmp-app-podcast open_search

# iOS build
cargo build -p nmp-app-podcast --target aarch64-apple-ios-sim
xcodebuild build-for-testing -workspace App/Podcast.xcworkspace -scheme Podcastr \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -skipPackagePluginValidation 2>&1 | tail -10

# TUI build
cargo check -p nmp-app-podcast --target aarch64-linux-android
cargo build -p podcast-tui

# Lint
git diff --check

# Manual proof (record in PR):
#   a. Type npub1... into Add Show > From URL → subscribe via Nostr path
#   b. Type user@domain.com → NIP-05 resolves → subscribe
#   c. Enter relay-targeted query in Nostr tab → NIP-50 result appears
```

## Files Changed

### Core implementation
- apps/nmp-app-podcast/src/open_search_handler.rs (new)
- apps/nmp-app-podcast/src/lib.rs — mod open_search_handler
- apps/nmp-app-podcast/src/ffi/actions/podcast_module.rs — PodcastAction::OpenSearch
- apps/nmp-app-podcast/src/host_op_handler/podcast_action_dispatch.rs — OpenSearch arm
- apps/nmp-app-podcast/src/ffi/projections/ — NostrSearchResult + nostr_search_results (if needed)

### iOS
- App/Sources/Bridge/AppStateStore+KernelActions.swift — kernelNostrOpenSearch(input:)
- App/Sources/Features/Library/AddShowSheet.swift — Nostr prefix check in AddByURLForm.submit()
- App/Sources/Domain/NostrConversation.swift — NostrNpub.looksLikeNostrInput(_:)
- App/Sources/Features/Library/NostrDiscoverForm.swift — relay-targeted search on submit
- App/Sources/Features/Settings/Agent/AddFriendSheet.swift — narrow Swift bech32 parsing
- App/Resources/changelog/<timestamp>.json (new)

### TUI
- apps/podcast-tui/src/input.rs — Nostr prefix check in handle_subscribe_input
- apps/podcast-tui/src/runtime_actions.rs — AppRuntime::nostr_open_search()

### Docs
- docs/plan.md
- docs/BACKLOG.md

## Notes

- **Hard dependency:** #597 must merge first; NMP v0.7.2 does not expose open_search, input-intent classification, or NIP-50/kind:10007 APIs
- **File size:** open_search_handler.rs must stay < 300 lines; split if needed
- **No ad-hoc parsing:** all Nostr protocol detection moves through NMP, never in native code
- **Changelog:** user-visible: "Nostr identifiers and NIP-05 addresses now resolve directly in the Add Show sheet"
- **Testing:** unit tests in Rust cover input-intent routing; manual proof required for iOS UI flow
