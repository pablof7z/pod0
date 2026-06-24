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
**Tested: 2026-06-24, 8:40 AM**

Unable to locate the owned-podcast / create-podcast surface (publisher flow). Explored the following locations:

**Areas Checked:**
1. **Sidebar Navigation:** Home, Library, Podcasts, Bookmarks, Clippings - no Publisher tab found
2. **Podcasts Tab (Add Show dialog):**
   - Search tab: Shows existing podcasts from Apple Podcasts directory with subscribe (+) buttons
   - Nostr tab: Shows existing NIP-F4 shows already published to relay.primal.net (Mock Podcast entries visible with subscribe buttons), but NO "Create new show" button found
   - From URL tab: Input field for podcast feed URL with Subscribe button (for adding existing feeds)
   - OPML tab: Not explored due to time constraints, but likely for importing podcast lists
3. **Settings:** Checked gear icon - appears to be general settings, no publisher option
4. **Agent Interface:** Opened Agent chat interface - shows suggested tasks (Summarize episode, What was I listening to?, Where did I leave off?) but no "Create Podcast" option visible

**Observations:**
- The Nostr tab does show existing NIP-F4 shows from the relay, confirming relay connectivity and NIP-F4 discovery works
- No visible UI button or flow to create/publish a new owned podcast
- The create-podcast surface may be unimplemented in the UI, or behind a debug/owner surface not accessible through normal navigation

**Next Steps for Implementation Team:**
- Check if publisher feature is behind a debug flag or requires special credentials
- Verify if there's a separate publisher app/view not yet integrated into the main UI
- Review BACKLOG.md for publisher feature status and whether UI scaffolding is complete

Unable to proceed with Steps 2-5 (create podcast, publish event, verify relay, check discovery) due to missing UI entry point.
