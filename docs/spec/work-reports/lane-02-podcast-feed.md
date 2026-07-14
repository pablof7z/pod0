# Lane 02 — RSS Parser, OPML, Subscription + Episode Models

> Worktree branch: `worktree-agent-a9fcedb73bae1fe39`
> Status: green build, 32/32 tests passing on iPhone 17 / iOS 26.4 simulator.

## Files added or replaced

All paths relative to repo root.

- `App/Sources/Podcast/PodcastSubscription.swift` — replaced the stub. Full
  field set per the brief plus forward-compat `Codable`.
- `App/Sources/Podcast/Episode.swift` — replaced the stub. Full field set,
  Podcasting 2.0 substructs (`Chapter`, `Person`, `SoundBite`),
  forward-compat `Codable`.
- `App/Sources/Podcast/AutoDownloadPolicy.swift` — `Mode` (off / latestN /
  allNew) + `wifiOnly`.
- `App/Sources/Podcast/DownloadState.swift` — five-state lifecycle.
- `App/Sources/Podcast/TranscriptState.swift` — six-state lifecycle, nested
  `Source` enum.
- `App/Sources/Podcast/TranscriptKind.swift` — MIME → enum classifier.
- `App/Sources/Podcast/RSSParser.swift` — pure-Foundation RSS reader, public
  surface.
- `App/Sources/Podcast/RSSParserDelegate.swift` — `XMLParserDelegate` impl.
- `App/Sources/Podcast/RSSItemAccumulator.swift` — per-item scratch state.
- `App/Sources/Podcast/RSSDateParsing.swift` — RFC 822 / ISO 8601 cascade.
- `App/Sources/Podcast/FeedClient.swift` — `URLSession` fetcher with
  conditional GET (ETag / Last-Modified).
- `App/Sources/Podcast/OPMLImport.swift` — OPML 2.0 reader.
- `App/Sources/Podcast/OPMLExport.swift` — OPML 2.0 writer.
- `AppTests/Sources/RSSParserTests.swift` — 15 tests against an inline
  fixture XML payload.

File-size discipline: every file is below the 300-line soft cap; the largest
is `RSSParserDelegate.swift` at 247 lines.

## Public surface (consumers in Lanes 3, 5, 6, 7)

```swift
struct PodcastSubscription: Codable, Sendable, Identifiable, Hashable {
    var id: UUID
    var feedURL: URL
    var title: String
    var author: String
    var imageURL: URL?
    var description: String
    var language: String?
    var categories: [String]
    var subscribedAt: Date
    var lastRefreshedAt: Date?
    var etag: String?
    var lastModified: String?
    var autoDownload: AutoDownloadPolicy
    var notificationsEnabled: Bool
    var defaultPlaybackRate: Double?
}

struct Episode: Codable, Sendable, Identifiable, Hashable {
    var id: UUID
    var subscriptionID: UUID
    var guid: String          // non-optional; synthesized when absent
    var title: String
    var description: String
    var pubDate: Date
    var duration: TimeInterval?
    var enclosureURL: URL
    var enclosureMimeType: String?
    var imageURL: URL?
    var chapters: [Chapter]?
    var persons: [Person]?
    var soundBites: [SoundBite]?
    var publisherTranscriptURL: URL?
    var publisherTranscriptType: TranscriptKind?
    var chaptersURL: URL?
    var playbackPosition: TimeInterval
    var played: Bool
    var downloadState: DownloadState
    var transcriptState: TranscriptState
}

struct RSSParser: Sendable {
    struct ParsedFeed: Sendable { var subscription; var episodes }
    enum ParseError: Error, Sendable { case malformedXML, missingChannel, missingFeedURL }
    func parse(data: Data, feedURL: URL, subscriptionID: UUID = UUID()) throws -> ParsedFeed
    static func synthesizedGUID(enclosure: URL?, pubDateRaw: String?) -> String
}

struct FeedClient: Sendable {
    enum FeedFetchResult: Sendable { case notModified, .updated(subscription, episodes, lastRefreshedAt) }
    enum FeedFetchError: Error, Sendable { case transport, http(Int), parse(RSSParser.ParseError) }
    init(session: URLSession = .shared)
    func fetch(_ subscription: PodcastSubscription) async throws -> FeedFetchResult
}

struct OPMLImport: Sendable {
    enum OPMLError: Error, Sendable { case malformedXML }
    func parseOPML(data: Data) throws -> [PodcastSubscription]
}

struct OPMLExport: Sendable {
    func exportOPML(subscriptions:, title:, dateCreated:) -> Data
}
```

`Sendable` everywhere; no `@MainActor` because none of these write to
`AppStateStore`. Consumers that *do* should hop to main themselves.

## Parser coverage

### Base RSS 2.0
- `<channel>`: `<title>`, `<description>`, `<language>`, `<image><url>`.
- `<item>`: `<title>`, `<description>` (CDATA-aware), `<content:encoded>`
  (preferred over plain description), `<pubDate>` (RFC 822 / 1123 cascade
  with ISO 8601 fallback), `<guid>`, `<enclosure url type>`.

### iTunes namespace
- `<itunes:author>` (channel)
- `<itunes:summary>` (both, used as `description` fallback)
- `<itunes:image href="…">` (channel + item)
- `<itunes:duration>` parsing `H:MM:SS`, `MM:SS`, raw seconds
- `<itunes:category text="…">` (channel; deduped, ordered)

### Podcasting 2.0 namespace
- `<podcast:transcript url type>` — multiple tags supported; rank JSON > VTT
  > SRT > HTML > text picks the best one. Both `publisherTranscriptURL`
  *and* `publisherTranscriptType` populated for Lane 5.
- `<podcast:chapters url>` — JSON URL captured; inline parsing left to a
  follow-up.
- `<podcast:person role group img href>` — element body becomes `name`.
- `<podcast:soundbite startTime duration>` — element body becomes optional
  `title`.
- `<podcast:value>` and `<podcast:location>` — tolerated (no error), not
  yet exploded into structured fields. Lane 6 (V4V) and Lane 9 may extend.

### GUID synthesis
Items missing `<guid>` get a deterministic synthetic id of the form
`synth::<enclosureURL>::<pubDateRaw>`. Stable across re-fetches; Lane 6 keys
embedding rows off `Episode.guid`.

### Items skipped
Items without `<enclosure>` are skipped. Hybrid blog/podcast feeds emit
text-only items the player cannot play.

## Test coverage (15 tests)

All in `AppTests/Sources/RSSParserTests.swift`, against a single inline
fixture exercising every documented namespace tag:

1. Channel metadata extraction (title, author, language, image, categories,
   description).
2. Episode base fields (title, guid, enclosure, MIME, duration, image,
   description with CDATA).
3. `pubDate` parsed as RFC 822 GMT.
4. Multiple `<podcast:transcript>` tags → highest-rank kind wins.
5. `<podcast:chapters>` URL captured.
6. `<podcast:person>` and `<podcast:soundbite>` parsed with attributes +
   element body.
7. Synthetic GUID stability across re-parse.
8. `subscriptionID` propagates from input to subscription.id and every
   episode.subscriptionID.
9. `Episode` and `PodcastSubscription` Codable round-trip.
10. OPML export → import round-trip preserves order, escapes `&`, unescapes
    on import.
11. OPML import skips outline nodes without `xmlUrl` (typical folder rows).
12. Malformed XML throws.
13. Feed without `<channel>` throws `.missingChannel`.
14. `TranscriptKind.from(mimeType:)` classifies common values + tolerates
    parameter suffixes.

Also covered indirectly: `FeedClient` types compile; `OPMLExport` XML
escaping; `AutoDownloadPolicy` Codable via subscription round-trip.

## What consumers should know

### Lane 3 — Library UI
- Subscriptions are `Codable + Sendable + Identifiable + Hashable` — drive
  `ForEach`/`List` directly with the model.
- `PodcastSubscription.id` is the SwiftUI identity; `feedURL` is the dedupe
  key on import (`OPMLImport` already dedupes).
- Episode status capsules read `downloadState` and `transcriptState`. Both
  enums are exhaustive and `Codable`; switch over them in the row view.
- `lastRefreshedAt == nil` distinguishes "freshly imported, never fetched"
  from "imported and confirmed empty" — useful for the OPML import progress
  UI in wireframe D.

### Lane 5 — Transcript ingestion
- `Episode.publisherTranscriptURL` *plus* `publisherTranscriptType` is the
  hand-off. Type is already a `TranscriptKind` enum so the dispatcher is a
  switch, not another MIME parse.
- Rank-picking already biased toward JSON > VTT > SRT > HTML > text per
  `transcription-stack.md` §2.
- `TranscriptState` is the lifecycle to drive; the `.ready(source:)` case
  carries the discriminator the Library badge needs.

### Lane 6 — Embeddings
- `Episode.guid` is non-optional and stable across re-fetches (see synthesis
  rule). Use it as the embedding row key.
- `Episode.subscriptionID` is the foreign-key handle — index alongside
  `guid` for show-scoped queries.

### Lane 7 — Wiki (cross-episode)
- `PodcastSubscription.categories` is deduped, ordered list of iTunes
  categories — useful as a coarse topic prior.
- `Episode.persons` carries hosts + guests for speaker resolution.

## Operational notes

- `FeedClient` honors `If-None-Match` (ETag) and `If-Modified-Since`
  (Last-Modified) and emits `.notModified` so the caller can short-circuit.
  This is the path most ~hourly polls will take after the first fetch.
- `RSSParser` is pure: no networking, no global state. Safe to run from a
  detached task.
- All XML parsing uses `Foundation.XMLParser`. No SPM dependency added.
- `RSSItemAccumulator.transcriptRank(_:)` exposes the same rank order as
  `TranscriptKind` — kept as a static helper so future namespace handlers
  can extend rather than fork.

## Constraints honored

- No SPM deps added.
- All models `Codable + Sendable + Identifiable + Hashable`.
- Swift 6 strict concurrency (`SWIFT_STRICT_CONCURRENCY=complete`) — clean.
- File-size soft 300 / hard 500 — every file ≤ 247 lines.
- Did not touch other lanes' folders (`Audio/`, `Features/Library`,
  `Features/Player`, `Transcript/`, `Knowledge/`, `Voice/`, `Briefing/`,
  `Features/Briefings`, `Agent/AgentTools+Podcast.swift`, `Project.swift`,
  `App/Resources/Info.plist`).

## Build / test

- `tuist generate` → success.
- `xcodebuild build -destination 'generic/platform=iOS Simulator'` → success.
- `xcodebuild test -destination 'platform=iOS Simulator,name=iPhone 17'`
  → 32 tests passed (15 new + 17 pre-existing). The brief specified
  iPhone 16 which is not provisioned on this host; iPhone 17 (iOS 26.4) was
  used as a like-for-like substitute.
