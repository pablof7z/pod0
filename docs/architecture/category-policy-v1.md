# Category policy v1

Task 4.3 completes the Rust category aggregate. Task 4.4 connects native views
to typed category intents and bounded projections; until that cutover lands,
the legacy Swift category fields remain an import source and UI compatibility
projection, not an alternative design for the Rust owner.

## Durable state

Schema version 47 adds one `pod0_category_settings` row for every category.
Category identity, metadata, membership, settings, and collection revisions
share the core SQLite transaction boundary. Existing Rust category rows are
backfilled with the exact permissive legacy defaults:

- no category auto-download override;
- RAG enabled; and
- notifications enabled.

A category settings write names the category revision it observed. A stale
revision is recorded as a rejected transition and cannot overwrite the accepted
settings. Unsupported auto-download wire values are rejected before mutation.

## Auto-download resolution

No category override means the subscription policy remains authoritative. If a
podcast belongs to multiple categories with overrides, Rust chooses the most
recent membership; equal timestamps break by stable category identity. Any
other category carrying a different override is returned as bounded conflict
evidence. Native code does not choose or silently merge competing policies.

The later playback/download policy task consumes this resolved value when it
moves download consequences into Rust. This task owns the durable preference
and its deterministic resolution, not native transfer execution.
