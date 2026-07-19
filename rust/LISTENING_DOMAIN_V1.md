# Listening domain v1

Issue #78 froze the target listening semantics used by the first Rust-backed
iOS slice. The import and library/playback cutovers (#79, #81, #82) now make
Rust authoritative for these facts; #83 removes the disabled pre-cutover Swift
implementations while preserving the read-only importer.

## Identity

- Native migration code supplies every `PodcastId`, `EpisodeId`, and
  `QueueEntryId`; Rust validation never generates an identity.
- RSS podcast identity uses the current Swift v1 comparison key: lowercase the
  complete absolute HTTP(S) URL. It does not trim a trailing slash and it also
  lowercases the path. That behavior is preserved for migration safety, not
  endorsed as a future URL-normalization policy.
- A feed refresh preserves the existing podcast ID when one v1 comparison key
  matches. Multiple matching IDs or reuse of an ID for another feed fail with
  a typed diagnostic.
- Episode external identity is the exact, case-sensitive publisher GUID scoped
  to its parent podcast. The existing Swift `synth::` fallback is treated as a
  publisher GUID after parsing. A match preserves the existing episode ID.
- Modern `podcastID` wins over legacy `id`/`subscriptionID`, matching Swift
  Codable. If neither exists, import fails rather than inventing a parent.

## Listening and queue behavior

- Positions and durations are integer milliseconds. Playback rates are integer
  thousandths in the supported 0.5x–3.0x host range.
- A completed episode has a zero resume position. Completion comes from the
  natural media-item end or an explicit user action; v1 has no percentage
  threshold. Disabling auto-mark preserves the final resume position as
  in-progress.
- Queue order is meaningful. Slot identity is independent from episode
  identity, so repeated bounded segments from one episode remain distinct.
  Whole-episode admission deduplication remains a command-policy concern for
  the vertical-slice reducer.
- Sleep modes are off, positive duration, end of episode, or an explicit
  forward-compatible unsupported code. Active timers are session-scoped and
  clear to off when the facade reopens; queue, resume, rate, and preferences
  remain durable.

## Artifact boundary

Download and transcript payloads are not part of this slice. Listening records
carry only unavailable/available/unsupported status plus a versioned opaque
artifact reference. Host file URLs and transcript contents remain with their
own workflow until those domains migrate.

The checked-in `Fixtures/CoreListening/listening-domain-v1.properties` is read
by Rust tests, real Swift Codable migration tests through the generated FFI,
and the generated Kotlin runtime smoke test. Unknown fixture properties are
ignored; unknown enum wire codes remain explicit unsupported values.
