# Scenario K2: Create a clip and verify the raw kind:9802 event on the relay

## Goal
End-to-end NIP-84 verification: create a user-visible clip in the app, then prove
the kernel published a kind:9802 event to `relay.primal.net` carrying the correct
tag set. This supersedes the BLOCKED F2 run, which could not inspect the raw event.

**Critical correction to F2's assumptions:** the app does NOT use an `a`-tag anchor.
Per the kernel (`apps/nmp-app-podcast/src/social_publish_handler.rs`,
`build_highlight_tags`), the event anchors the source with an **`i`-tag** in
NIP-73 form and stores the highlighted text in a **`context`** tag, the caption in
an **`alt`** tag. The exact tag order produced is:
```
["r",  <enclosure_url>]                                  (audio file; if known)
["r",  <feed_url>]                                       (podcast feed; if known)
["i",  "podcast:item:guid:<guid>#t=<start_sec>,<end_sec>"]
["context", <highlighted text>]                          (always present)
["alt", <caption>]                                       (only if caption non-empty)
```
`content` = the highlighted transcript text (same string as the `context` tag).
Times are **integer seconds** (f64 rounded). Only **non-agent** clips with
non-empty `transcript_text` auto-publish (agent clips are skipped — see L5/G4).

## Prerequisites
- K1 done; `$HEX` recorded.
- App signed in; a transcribed episode available (e.g. a "The Daily" episode whose
  transcript is indexed — E4 confirmed The Daily episodes are indexed).
- Auto-publish path intact (clip must be user-visible, source != "agent", and have
  transcript text). Manual clips from the composer satisfy this.

## Steps
1. Note the wall-clock time on the host now (`date -u +%s`) → `$T0`. Used to filter
   only events created during this run.
2. In the app, create a NON-agent clip with transcript text (the manual composer
   path; see K3 for the long-press composer mechanics). Give it a distinctive
   caption you will recognize on the relay, e.g. `K2-VERIFY-<short-random>`. Save.
   *Screenshot of the saved clip in Clippings.*
3. On the host, fetch this author's recent highlights:
   ```
   nak req -k 9802 -a <HEX> -s <T0> -l 10 wss://relay.primal.net
   ```
   **Expected:** at least one kind:9802 JSON event whose `alt` tag (or `content`)
   contains your `K2-VERIFY-…` marker. *Paste the full event JSON into Notes.*
4. Verify the tag set on that event:
   - `kind` == `9802`.
   - exactly one `["context", …]` tag; its value == `content` (the highlighted text).
   - an `["i", "podcast:item:guid:<guid>#t=<a>,<b>"]` tag where `<a>` and `<b>` are
     integers and `<a> < <b>` (the clip's start/end in seconds).
   - an `["alt", …]` tag matching your caption (present because you set a caption).
   - `["r", …]` for the enclosure URL (an `.mp3`/audio URL) — present when the
     episode has an enclosure; a feed `["r", …]` may also appear.
   - There is **NO** `["a", …]` tag. If you see one, that's a contract change worth
     flagging — record it; it does not match the current kernel.
5. Cross-check the time fragment: the `#t=<a>,<b>` in the `i`-tag must equal the
   clip's displayed range in Clippings (rounded to whole seconds). *Screenshot.*

## Acceptance Criteria
- A kind:9802 event for `$HEX` is retrievable from `relay.primal.net`, created
  after `$T0`, matching your caption marker.
- The event has: one `context` tag == `content`; an `i`-tag in
  `podcast:item:guid:<guid>#t=<int>,<int>` form; an `alt` tag == your caption; and
  at least one `r` (enclosure) tag.
- The `#t=` integer range equals the in-app clip range.
- No `a`-tag is present (matches the current kernel contract).

## Known Issues / Watch Points
- Agent-created clips and clips without transcript text do NOT auto-publish — use a
  manual, transcribed clip here (agent publishing is its own path; see L5/G4).
- Publishing rides the NIP-65 outbox via `PublishTarget::Auto`; if the author has
  no kind:10002, it falls back to locally configured write relays. The default
  seed makes `relay.primal.net` a write relay (role `both,indexer`), so it should
  receive the event. If nothing shows after ~10s, also try `wss://purplepag.es`
  (indexer) and note the discrepancy.
- Use `-s <T0>` to avoid matching stale highlights from earlier sessions. If you
  omit it you may grab an old clip and mis-attribute the result.
- Round-trip can take a few seconds; re-run the `nak req` 2–3 times before calling
  it a FAIL.

## Notes
