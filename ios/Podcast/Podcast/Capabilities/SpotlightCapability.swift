@preconcurrency import CoreSpotlight
import Foundation
import UniformTypeIdentifiers
import os.log

// MARK: - SpotlightCapability
//
// Indexes the kernel's library projection into iOS Spotlight so system
// search can deep-link back into the app. Passive shape — like
// `PlatformCapability`, it is not routed through `handleJSON(_:)`. The
// hook is the snapshot poll in `KernelModel.startSnapshotPoll`, which
// calls `indexLibrary(_:)` after assigning a new `library`.
//
// All items live in a single Spotlight domain
// (`io.f7z.podcast.library`) so `clearAll()` wipes the index in one
// call. Item kind is distinguished by a string prefix on
// `uniqueIdentifier`; the deep-link router decodes the same scheme.
//
// D6: `CSSearchableIndex` errors are logged and swallowed.
// D7: iOS only mirrors the kernel's library into the OS index.

@MainActor
final class SpotlightCapability {

    // MARK: - Constants

    /// Capability namespace. Reserved for future request/response
    /// wiring; today this capability is purely tick-driven.
    static let namespace = "pcst.spotlight.capability"

    /// Single domain identifier used for every item this capability
    /// writes. A single domain lets `clearAll()` wipe everything in one
    /// call and lets `deindex(podcastId:)` target a known prefix set
    /// without per-item bookkeeping.
    static let domainIdentifier = "io.f7z.podcast.library"

    /// Process-wide instance. The capability holds no per-instance
    /// state besides the `lastIndexedLibrary` snapshot used for delta
    /// detection — sharing the same instance across the snapshot-poll
    /// caller and the (test) suite avoids a parallel "did we
    /// reindex" cache.
    static let shared = SpotlightCapability()

    // MARK: - Identifier scheme

    private static let podcastPrefix = "podcast:"
    private static let episodePrefix = "episode:"

    /// Build the Spotlight `uniqueIdentifier` for a podcast row.
    static func podcastIdentifier(_ id: String) -> String { podcastPrefix + id }
    /// Build the Spotlight `uniqueIdentifier` for an episode row.
    static func episodeIdentifier(_ id: String) -> String { episodePrefix + id }

    // MARK: - Deep-link decoding

    /// Decoded result of a Spotlight tap. Carries the raw kernel id so
    /// the navigation layer can look the row up in
    /// `KernelModel.library`.
    enum DeepLink: Equatable, Hashable {
        case podcast(String)
        case episode(String)
    }

    /// Decode a Spotlight `uniqueIdentifier` back into a `DeepLink`.
    /// Returns nil for malformed / foreign identifiers (defence in
    /// depth — the OS shouldn't hand us identifiers we didn't issue,
    /// but we treat that case as data, not as a crash).
    static func deepLink(fromIdentifier identifier: String) -> DeepLink? {
        if identifier.hasPrefix(podcastPrefix) {
            let raw = String(identifier.dropFirst(podcastPrefix.count))
            return raw.isEmpty ? nil : .podcast(raw)
        }
        if identifier.hasPrefix(episodePrefix) {
            let raw = String(identifier.dropFirst(episodePrefix.count))
            return raw.isEmpty ? nil : .episode(raw)
        }
        return nil
    }

    /// Decode a Spotlight continuation `NSUserActivity` into a
    /// `DeepLink`. The OS hands the app one of these via
    /// `.onContinueUserActivity(CSSearchableItemActionType)` when a
    /// search result is tapped; the identifier lives in
    /// `userInfo[CSSearchableItemActivityIdentifier]`.
    static func deepLink(fromActivity activity: NSUserActivity) -> DeepLink? {
        guard activity.activityType == CSSearchableItemActionType,
              let identifier = activity.userInfo?[CSSearchableItemActivityIdentifier] as? String
        else { return nil }
        return deepLink(fromIdentifier: identifier)
    }

    // MARK: - State

    private static let logger = Logger(subsystem: "io.f7z.podcast", category: "Spotlight")

    /// Cached copy of the library the index was last built against.
    /// Used by `indexLibrary(_:)` to skip work when the caller hands
    /// us the same snapshot twice (the snapshot poll fires every
    /// 500ms even when only the player-tick fields changed).
    private var lastIndexedLibrary: [PodcastSummary] = []

    private var started: Bool = false

    // MARK: - Lifecycle (symmetry with the other capabilities)

    /// Idempotent. No-op besides flipping the flag — the
    /// `CSSearchableIndex` is process-wide and is lazily contacted on
    /// the first `indexLibrary(_:)` call.
    func start() {
        guard !started else { return }
        started = true
    }

    /// Idempotent. Marks the capability inactive. Does **not** clear
    /// the Spotlight index — the user expects search results to
    /// survive an app process restart. Call `clearAll()` explicitly
    /// when wiping user data.
    func stop() {
        started = false
        lastIndexedLibrary = []
    }

    var isStarted: Bool { started }

    // MARK: - Indexing

    /// Replace the library's slice of the Spotlight index with
    /// `library`. Idempotent: short-circuits when the library is
    /// equal to the snapshot already indexed.
    ///
    /// Strategy: re-build the whole library domain on each delta. The
    /// library is small (tens of podcasts × tens of episodes); the
    /// cost of a full rebuild is negligible compared to the
    /// bookkeeping a per-row incremental indexer would need to track
    /// what is present in the OS index.
    func indexLibrary(_ library: [PodcastSummary]) {
        // Library-delta throttle: the snapshot poll runs at 2 Hz, but
        // most ticks only update player position, not the library.
        // Equatable comparison is cheap (struct-of-strings) and saves
        // a disk write per tick.
        if library == lastIndexedLibrary { return }
        lastIndexedLibrary = library

        let items = buildItems(for: library)
        let index = CSSearchableIndex.default()

        // Delete-then-insert keeps the OS index in lock-step with the
        // current library: rows the user unsubscribed from disappear,
        // re-titled rows refresh. The two operations are ordered by
        // the system (callbacks fire in submission order on the same
        // queue), so the empty-then-fill window is short.
        index.deleteSearchableItems(withDomainIdentifiers: [Self.domainIdentifier]) { error in
            if let error {
                Self.logger.error("spotlight: delete-domain failed: \(error, privacy: .public)")
            }
            guard !items.isEmpty else { return }
            index.indexSearchableItems(items) { error in
                if let error {
                    Self.logger.error("spotlight: index \(items.count) items failed: \(error, privacy: .public)")
                }
            }
        }
    }

    /// Remove a single podcast (and its episodes) from the index.
    /// Called when the user unsubscribes — the next library delta
    /// would do the same work via the full rebuild, but firing the
    /// targeted delete keeps the Spotlight surface in sync inside the
    /// snapshot-poll window instead of waiting up to 500 ms.
    func deindex(podcastId: String) {
        let podcastUID = Self.podcastIdentifier(podcastId)
        // Walk the cached library to find this podcast's episode ids;
        // the cache is the same data the OS index was built from, so
        // the prefix set is exact.
        let episodeUIDs: [String] = lastIndexedLibrary
            .first(where: { $0.id == podcastId })
            .map { $0.episodes.map { Self.episodeIdentifier($0.id) } } ?? []
        let identifiers = [podcastUID] + episodeUIDs

        // Also drop the row from the cached snapshot so a subsequent
        // `indexLibrary(library)` with the same library array still
        // triggers a rebuild (defence against the caller skipping
        // the delete-then-rebuild handshake).
        lastIndexedLibrary.removeAll { $0.id == podcastId }

        CSSearchableIndex.default().deleteSearchableItems(withIdentifiers: identifiers) { error in
            if let error {
                Self.logger.error("spotlight: deindex podcast \(podcastId, privacy: .public) failed: \(error, privacy: .public)")
            }
        }
    }

    /// Remove every item this capability has put into the index.
    /// Wired into a future sign-out / clear-data path; today only the
    /// test suite calls it.
    func clearAll() {
        lastIndexedLibrary = []
        CSSearchableIndex.default().deleteSearchableItems(
            withDomainIdentifiers: [Self.domainIdentifier]
        ) { error in
            if let error {
                Self.logger.error("spotlight: clearAll failed: \(error, privacy: .public)")
            }
        }
    }

    // MARK: - Item builders

    /// Public for test access. Builds the `CSSearchableItem` array
    /// that `indexLibrary(_:)` submits to the OS. Pure: no side
    /// effects, no `CSSearchableIndex` calls.
    func buildItems(for library: [PodcastSummary]) -> [CSSearchableItem] {
        var items: [CSSearchableItem] = []
        items.reserveCapacity(library.count + library.reduce(0) { $0 + $1.episodes.count })
        for podcast in library {
            items.append(makeSearchable(from: podcast))
            for episode in podcast.episodes {
                items.append(makeSearchable(from: episode, showName: podcast.title))
            }
        }
        return items
    }

    private func makeSearchable(from podcast: PodcastSummary) -> CSSearchableItem {
        let attrs = CSSearchableItemAttributeSet(contentType: UTType.audio)
        attrs.title = podcast.title
        // Podcast row description: "12 episodes · Author Name" when we
        // have both; degrade gracefully when fields are absent. The
        // kernel doesn't project a podcast description today (tracked
        // alongside `pr-episode-description`), so this is the most
        // useful snippet we can show the user in Spotlight.
        attrs.contentDescription = podcastDescription(for: podcast)
        if let author = podcast.author, !author.isEmpty {
            attrs.artist = author
        }
        if let urlString = podcast.artworkUrl, let url = URL(string: urlString) {
            attrs.thumbnailURL = url
        }
        attrs.keywords = ["podcast", "show"] + (podcast.author.map { [$0] } ?? [])

        let item = CSSearchableItem(
            uniqueIdentifier: Self.podcastIdentifier(podcast.id),
            domainIdentifier: Self.domainIdentifier,
            attributeSet: attrs
        )
        return item
    }

    private func podcastDescription(for podcast: PodcastSummary) -> String {
        var parts: [String] = []
        if podcast.episodeCount > 0 {
            let suffix = podcast.episodeCount == 1 ? "episode" : "episodes"
            parts.append("\(podcast.episodeCount) \(suffix)")
        }
        if let author = podcast.author, !author.isEmpty {
            parts.append(author)
        }
        return parts.joined(separator: " · ")
    }

    private func makeSearchable(from episode: EpisodeSummary, showName: String) -> CSSearchableItem {
        let attrs = CSSearchableItemAttributeSet(contentType: UTType.audio)
        attrs.title = episode.title
        // Episode rows have no description field on `EpisodeSummary`
        // today (tracked in `pr-episode-description`). Until then the
        // most useful snippet is the parent show name — Spotlight
        // shows it under the episode title so the user can tell
        // similarly-named episodes apart.
        if !showName.isEmpty {
            attrs.contentDescription = "From \(showName)"
            // `album` + `artist` render under the title in the
            // Spotlight result row, which is exactly the
            // "which show is this from" hint the user expects.
            attrs.album = showName
            attrs.artist = showName
        }
        if let urlString = episode.artworkUrl, let url = URL(string: urlString) {
            attrs.thumbnailURL = url
        }
        if let published = episode.publishedAt {
            attrs.contentCreationDate = Date(timeIntervalSince1970: TimeInterval(published))
        }
        if let duration = episode.durationSecs {
            attrs.duration = NSNumber(value: duration)
        }
        attrs.keywords = ["podcast", "episode", showName].filter { !$0.isEmpty }

        let item = CSSearchableItem(
            uniqueIdentifier: Self.episodeIdentifier(episode.id),
            domainIdentifier: Self.domainIdentifier,
            attributeSet: attrs
        )
        return item
    }
}
