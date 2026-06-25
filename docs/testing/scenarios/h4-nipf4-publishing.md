# Scenario H4: NIP-F4 publishing for an owned podcast

## Goal
Validate the NIP-F4 publishing path: creating/owning a podcast publishes a kind:10154
show event and a kind:10064 author claim, signed and accepted by the relay.

## Prerequisites
- App past onboarding with a signing identity (local key or remote signer).
- Network + relay (`relay.primal.net`) reachable.
- Access to the relay to verify published events (host-side `nak`), if possible.

## Steps
1. Locate the owned-podcast / create-podcast surface (publisher flow). **Expected:**
   A way to define an owned podcast (title, description, categories). *Screenshot.*
2. Create/publish an owned podcast. **Expected:** The app signs and publishes a
   kind:10154 show event; status reports success (relay accepted). *Screenshot.*
3. **Expected:** An author-claim kind:10064 event is published after create/update/
   delete (per `p0-nipf4-author-claim`). *Screenshot.*
4. Verify on the relay (host): `nak req -k 10154 -a <hex-pubkey> relay.primal.net`
   and `nak req -k 10064 -a <hex-pubkey> relay.primal.net`. **Expected:** The events
   exist with valid `id`/`pubkey`/`sig` and NIP-F4-conformant tags (no legacy
   `d`/`a`/`published_at`/`imeta` tags). Paste JSON into Notes. *Screenshot.*
5. Cross-check discovery (B3): the owned show is now discoverable in the Nostr tab.

## Acceptance Criteria
- Creating an owned podcast publishes a signed kind:10154 show event accepted by the
  relay (status reflects "published").
- A kind:10064 author claim is published after create/update/delete.
- Events are NIP-F4 conformant (no non-NIP-F4 tags) with valid signatures.
- The owned show becomes discoverable over Nostr.

## Known Issues / Watch Points
- Per BACKLOG, the NIP-F4 wire contract, signing, publishing, and author-claim are
  marked done — but a regression in `publish_outbox`/projection-rev has historically
  red'd e2e (MEMORY: NMP pin). Watch for a publish that reports success but never
  reaches the relay.
- Per-podcast secrets live in `podcast-keys.json` (no Keychain migration).
- The publisher/create-podcast UI may be limited or behind a debug/owner surface —
  if you can't find a create flow, mark BLOCKED and note where you looked.

## Notes

**Result: BLOCKED**
**Tested: 2026-06-24, 12:40 PM (follow-up)**

Confirmed: No owned-podcast / create-podcast UI surface exists in the app. Kernel-side NIP-F4 support is complete, but Swift UI has not been implemented.

**Areas Verified:**
1. **All Podcasts view:** Home → "See all podcasts" → "+" button opens "Add Show" dialog
2. **Add Show dialog tabs:**
   - **Search:** Shows Apple Podcasts directory results with subscribe buttons
   - **Nostr:** Actively searches relay.primal.net for kind:10154 shows; relay is reachable and working
   - **From URL:** Input field + Subscribe button for external podcast feeds (NOT for creating owned podcasts)
   - **OPML:** For importing podcast lists
   - **NO "Create owned podcast" or "Publish new show" option found in any tab**
3. **Settings:** General app settings; no publisher/creator options
4. **Agent interface:** Task suggestions only; no podcast creation tools

**Key Finding:**
- Kernel-side: `publish_show`, `publish_author_claim`, `sign_event`, `dispatch_nostr_relay` are all complete per BACKLOG
- File storage: `podcast-keys.json` ready for owned podcast secrets
- Swift UI: **Missing entire create/edit owned podcast form**
- The "From URL" tab is for subscribing to external feeds, NOT for creating/publishing owned podcasts

**Relay Verification:**
- Nostr tab successfully queries relay.primal.net and displays "Searching..." state
- Relay connectivity is working; relay discovery framework is implemented
- No owned podcasts are currently published from this test identity

**Unable to proceed with Steps 2-5** due to missing Swift UI implementation for podcast creation/publishing.

**Recommendation:**
The backend is ready. A Swift UI feature ticket is needed to implement:
- Owned podcast creation form (title, description, categories, cover art)
- UI entry point in the add-show dialog or a separate "My Shows" / "Publisher" section
- Edit/delete UI for managing owned podcasts
- Real-time publishing status feedback
