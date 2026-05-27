import Foundation

// MARK: - PlatformCapability wire types (Swift mirrors of the Rust schema)
//
// Lifted out of `PlatformCapability.swift` so the capability file stays
// under the soft 300-line limit (AGENTS.md). The types remain top-level —
// the tests + the iOS observers reference them as `WidgetSnapshot`,
// `HandoffState`, `HandoffUserInfoKey` directly, so moving them changes
// nothing for callers.
//
// Source of truth (Rust):
//   - `apps/nmp-app-podcast/src/ffi/snapshot.rs` — `WidgetSnapshot`
//   - `apps/podcast-core/src/types/handoff.rs`   — `HandoffState`
//
// Tests pinning the JSON shape: `PlatformCapabilityWireTests.swift`.

/// Swift mirror of `apps/nmp-app-podcast/src/ffi/snapshot.rs`
/// `WidgetSnapshot`. Hand-mirrored (not generated) until the
/// `dump_projection_schemas` codegen catches up — keep the field
/// names + `Codable` semantics in lock-step with the Rust source.
///
/// The widget extension decodes the JSON payload this struct
/// encodes; the extension defines its own copy of the type because
/// the two targets don't share Swift sources.
struct WidgetSnapshot: Codable, Equatable {
    var nowPlayingEpisodeTitle: String?
    var nowPlayingPodcastTitle: String?
    var nowPlayingArtworkURL: String?
    var isPlaying: Bool
    /// `0.0..=1.0`; pre-computed by Rust so the widget can render
    /// a ring without dividing by a possibly-zero duration.
    var positionFraction: Float
    var unplayedCount: Int

    enum CodingKeys: String, CodingKey {
        case nowPlayingEpisodeTitle = "now_playing_episode_title"
        case nowPlayingPodcastTitle = "now_playing_podcast_title"
        case nowPlayingArtworkURL = "now_playing_artwork_url"
        case isPlaying = "is_playing"
        case positionFraction = "position_fraction"
        case unplayedCount = "unplayed_count"
    }
}

/// Swift mirror of `apps/podcast-core/src/types/handoff.rs`
/// `HandoffState`. The Rust side guarantees `activityType` is
/// one of the known string ids; `isKnownActivityType` performs
/// the defensive check on the iOS side (D6 — unknown wire data
/// is dropped, not thrown).
struct HandoffState: Codable, Equatable {
    /// `io.f7z.podcast.playing` — playback in progress.
    static let activityPlaying = "io.f7z.podcast.playing"
    /// `io.f7z.podcast.browsing` — non-player surface foregrounded.
    static let activityBrowsing = "io.f7z.podcast.browsing"

    var activityType: String
    var episodeID: String?
    var podcastID: String?
    var positionSecs: Double?

    enum CodingKeys: String, CodingKey {
        case activityType = "activity_type"
        case episodeID = "episode_id"
        case podcastID = "podcast_id"
        case positionSecs = "position_secs"
    }

    /// `true` when `activityType` matches one of the platform
    /// capability's known activity ids.
    var isKnownActivityType: Bool {
        switch activityType {
        case Self.activityPlaying, Self.activityBrowsing:
            return true
        default:
            return false
        }
    }
}

/// `userInfo` keys the iOS executor populates on a donated
/// `NSUserActivity`. The receiving side (the same app on another
/// device) reads back via these keys.
///
/// Field names mirror `HandoffState`'s snake-case `CodingKeys`
/// because the kernel's wire shape is the contract; tests in
/// `PlatformCapabilityWireTests` pin this equivalence.
enum HandoffUserInfoKey {
    static let episodeID = "episode_id"
    static let podcastID = "podcast_id"
    static let positionSecs = "position_secs"
}
