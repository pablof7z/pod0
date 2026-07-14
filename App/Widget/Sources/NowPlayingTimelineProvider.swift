import Foundation
import WidgetKit

// MARK: - NowPlayingSnapshot (widget side)

/// Wire-compatible mirror of the app-side `NowPlayingSnapshot`. The two
/// targets compile separately so we can't share the type — keeping the
/// fields identical (and JSON-encoded) lets either side migrate
/// independently.
struct NowPlayingSnapshot: Codable, Equatable {
    var episodeTitle: String
    var showName: String
    var imageURLString: String?
    var position: TimeInterval
    var duration: TimeInterval
    /// Title of the chapter at the playhead, when the episode has
    /// navigable chapters. Optional so older snapshots decode cleanly.
    var chapterTitle: String?
    /// `true` while the engine is playing (or buffering). Optional with
    /// a `false` fallback so snapshots written before this field decode
    /// without misreporting playback state.
    var isPlaying: Bool?
}

// MARK: - NowPlayingEntry

/// Single timeline entry the Now Playing widget renders against. `nil`
/// `snapshot` means nothing's currently playing — the widget shows its
/// empty state instead of stale metadata.
struct NowPlayingEntry: TimelineEntry {
    let date: Date
    let snapshot: NowPlayingSnapshot?
}

// MARK: - NowPlayingTimelineProvider

/// Reads the latest `NowPlayingSnapshot` out of the App Group's shared
/// `UserDefaults` on every timeline refresh. The app side calls
/// `WidgetCenter.shared.reloadAllTimelines()` after each snapshot write,
/// so the 60s system fallback is just a backstop for the case where the
/// app is suspended and a passive position update would be stale.
struct NowPlayingTimelineProvider: TimelineProvider {

    /// Must match `NowPlayingSnapshotStore.appGroupID` on the app side.
    static let appGroupID = "group.com.podcastr.app"
    /// Must match `NowPlayingSnapshotStore.defaultsKey` on the app side.
    static let defaultsKey = "now-playing-snapshot.v1"

    func placeholder(in context: Context) -> NowPlayingEntry {
        NowPlayingEntry(date: .now, snapshot: nil)
    }

    func getSnapshot(in context: Context, completion: @escaping (NowPlayingEntry) -> Void) {
        completion(NowPlayingEntry(date: .now, snapshot: readCurrent()))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<NowPlayingEntry>) -> Void) {
        let entry = NowPlayingEntry(date: .now, snapshot: readCurrent())
        // Re-poll after 60s — the app pushes reloads on every snapshot
        // write, so this is the upper bound on staleness when the app is
        // suspended (e.g. background audio paused for 10 minutes).
        let nextFire = Date().addingTimeInterval(60)
        completion(Timeline(entries: [entry], policy: .after(nextFire)))
    }

    /// Decodes the snapshot from App Group `UserDefaults`. Returns `nil`
    /// when the suite is unavailable, the key is missing, or the JSON
    /// fails to decode — all of which collapse into "show empty state".
    private func readCurrent() -> NowPlayingSnapshot? {
        guard let defaults = UserDefaults(suiteName: Self.appGroupID),
              let data = defaults.data(forKey: Self.defaultsKey)
        else { return nil }
        return try? JSONDecoder().decode(NowPlayingSnapshot.self, from: data)
    }
}
