import Foundation

enum TranscriptStartPolicy: String, Codable, Sendable, Hashable, CaseIterable, Identifiable {
    case automatic
    case whenPlayed

    var id: String { rawValue }

    var label: String {
        switch self {
        case .automatic: "Automatically"
        case .whenPlayed: "When played"
        }
    }
}

/// User's follow state for a specific `Podcast`.
///
/// Holds purely user-mutable preferences — auto-download, notifications,
/// preferred playback rate. Identity, metadata, and feed HTTP cache live
/// on `Podcast` (one-to-one via `podcastID`). A `Podcast` without a
/// matching `PodcastSubscription` is "known but not followed" — exactly
/// the state for agent-added external episodes.
///
/// Migration note: pre-split installs serialized everything (feedURL,
/// title, imageURL, autoDownload, …) inside `PodcastSubscription`. The
/// persistence layer splits each legacy row into a `Podcast` + a slim
/// `PodcastSubscription`, with the legacy UUID preserved as
/// `Podcast.id` AND `PodcastSubscription.podcastID` so existing
/// `Episode.podcastID` foreign keys keep working through the rename.
struct PodcastSubscription: Codable, Sendable, Identifiable, Hashable {
    /// Foreign key to `Podcast.id`. Also serves as `Identifiable.id`
    /// since the user can subscribe to a podcast at most once.
    var podcastID: UUID
    /// When the user subscribed.
    var subscribedAt: Date

    // MARK: - User preferences

    /// Per-show download policy (off / latest-N / all-new + Wi-Fi guard).
    var autoDownload: AutoDownloadPolicy
    /// Per-show notification toggle.
    var notificationsEnabled: Bool
    /// Optional per-show playback rate override; falls back to
    /// `Settings.defaultPlaybackRate` when `nil`.
    var defaultPlaybackRate: Double?
    /// Determines when the shared state machine may start transcript work.
    var transcriptStartPolicy: TranscriptStartPolicy

    var id: UUID { podcastID }

    init(
        podcastID: UUID,
        subscribedAt: Date = Date(),
        autoDownload: AutoDownloadPolicy = .default,
        notificationsEnabled: Bool = true,
        defaultPlaybackRate: Double? = nil,
        transcriptStartPolicy: TranscriptStartPolicy = .automatic
    ) {
        self.podcastID = podcastID
        self.subscribedAt = subscribedAt
        self.autoDownload = autoDownload
        self.notificationsEnabled = notificationsEnabled
        self.defaultPlaybackRate = defaultPlaybackRate
        self.transcriptStartPolicy = transcriptStartPolicy
    }

    // MARK: - Codable (forward-compat decoding)

    private enum CodingKeys: String, CodingKey {
        case podcastID, subscribedAt
        case autoDownload, notificationsEnabled, defaultPlaybackRate, transcriptStartPolicy
        // Legacy keys from the pre-split shape. Decoded only as a fallback
        // when `podcastID` is absent so a freshly-installed app reading a
        // pre-split persisted file recovers cleanly. Never written.
        case legacy_id = "id"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        if let pid = try c.decodeIfPresent(UUID.self, forKey: .podcastID) {
            podcastID = pid
        } else {
            // Pre-split file: the row's `id` was the podcast's identity.
            podcastID = try c.decode(UUID.self, forKey: .legacy_id)
        }
        subscribedAt = try c.decodeIfPresent(Date.self, forKey: .subscribedAt) ?? Date()
        autoDownload = try c.decodeIfPresent(AutoDownloadPolicy.self, forKey: .autoDownload) ?? .default
        notificationsEnabled = try c.decodeIfPresent(Bool.self, forKey: .notificationsEnabled) ?? true
        defaultPlaybackRate = try c.decodeIfPresent(Double.self, forKey: .defaultPlaybackRate)
        transcriptStartPolicy = try c.decodeIfPresent(
            TranscriptStartPolicy.self,
            forKey: .transcriptStartPolicy
        ) ?? .automatic
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(podcastID, forKey: .podcastID)
        try c.encode(subscribedAt, forKey: .subscribedAt)
        try c.encode(autoDownload, forKey: .autoDownload)
        try c.encode(notificationsEnabled, forKey: .notificationsEnabled)
        try c.encodeIfPresent(defaultPlaybackRate, forKey: .defaultPlaybackRate)
        try c.encode(transcriptStartPolicy, forKey: .transcriptStartPolicy)
    }
}
