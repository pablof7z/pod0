// Compat stubs for Library views copied verbatim from App/Sources/.
// Full implementations replace these as the NMP snapshot projection lands.

import Foundation
import SwiftUI
import Observation

// MARK: - Episode

struct Episode: Codable, Sendable, Identifiable, Hashable {
    var id: UUID = UUID()
    var podcastID: UUID = UUID()
    var guid: String = ""
    var title: String = ""
    var description: String = ""
    var pubDate: Date = Date()
    var duration: TimeInterval?
    var enclosureURL: URL = URL(string: "about:blank")!
    var enclosureMimeType: String?
    var imageURL: URL?
    var played: Bool = false
    var isStarred: Bool = false
    var playbackPosition: TimeInterval = 0
    var downloadState: DownloadState = .notDownloaded
    var transcriptState: TranscriptState = .none
    var publisherTranscriptURL: URL?
    var chaptersURL: URL?
}

// MARK: - DownloadState

enum DownloadState: Codable, Sendable, Hashable {
    case notDownloaded
    case queued
    case downloading(Double, Double?)
    case downloaded
    case failed
}

// MARK: - TranscriptState

enum TranscriptState: Codable, Sendable, Hashable {
    case none
    case queued
    case fetchingPublisher
    case transcribing(Double)
    case ready
    case failed
}

// MARK: - PodcastCategory

struct PodcastCategory: Identifiable, Hashable, Sendable, Codable {
    var id: UUID = UUID()
    var name: String = ""
    var colorHex: String?
}

// MARK: - Subscription episode access

extension Subscription {
    var episodes: [Episode] { [] }
}

// MARK: - PlaybackState

@Observable
@MainActor
final class PlaybackState {
    static let shared = PlaybackState()
    var episode: Episode?
    var isPlaying: Bool = false
    var currentTime: TimeInterval = 0
    var duration: TimeInterval = 0
    var speed: Float = 1.0
    private var queue: [UUID] = []

    func isQueued(_ id: UUID) -> Bool { queue.contains(id) }
    func enqueue(_ id: UUID) { if !queue.contains(id) { queue.append(id) } }
    func removeFromQueue(_ id: UUID) { queue.removeAll { $0 == id } }
    func setEpisode(_ episode: Episode) { self.episode = episode }
    func play() { isPlaying = true }
    func pause() { isPlaying = false }
}

// MARK: - AppStateStore

@Observable
@MainActor
final class AppStateStore {
    var allPodcasts: [Podcast] = []
    var allEpisodesSorted: [Episode] = []
    var state: KernelState = KernelState()
    var unplayedCountByShow: [UUID: Int] = [:]
    var hasDownloadedByShow: Set<UUID> = []
    var hasTranscribedByShow: Set<UUID> = []

    func subscription(podcastID: UUID) -> Subscription? { nil }
    func subscription(for podcast: Podcast) -> Subscription? { nil }
    func episodes(for podcast: Podcast) -> [Episode] { [] }
    func podcast(id: UUID) -> Podcast? { allPodcasts.first { $0.id == id } }
    func deletePodcast(_ podcast: Podcast) {}
    func markEpisodePlayed(_ id: UUID) {}
    func markEpisodeUnplayed(_ id: UUID) {}
    func toggleEpisodeStarred(_ id: UUID) {}
    func setSubscriptionAutoDownload(_ enabled: Bool, for podcast: Podcast) {}
    func setSubscriptionAutoDownload(_ podcastID: UUID, policy: AutoDownloadPolicy) {}
    func setSubscriptionNotificationsEnabled(_ enabled: Bool, for podcast: Podcast) {}
    func setSubscriptionNotificationsEnabled(_ podcastID: UUID, enabled: Bool) {}
    func podcast(feedURL: URL) -> Podcast? { nil }
    func episodes(forPodcast id: UUID) -> [Episode] { [] }
    func deletePodcast(podcastID id: UUID) {}
    @discardableResult func addSubscriptions(_ podcasts: [Podcast]) -> [Podcast] { [] }
    @discardableResult func addSubscriptions(_ payloads: [SubscriptionImportPayload]) -> ImportResult { ImportResult() }
}

struct ImportResult {
    var imported: Int = 0
    var skipped: Int = 0
}

// MARK: - EpisodeDownloadService

@Observable
@MainActor
final class EpisodeDownloadService {
    static let shared = EpisodeDownloadService()

    var progress: [UUID: Double] = [:]

    func downloadState(for id: UUID) -> DownloadState { .notDownloaded }
    func attach(appStore: AppStateStore) {}
    func download(episodeID: UUID) {}
    func cancel(episodeID: UUID) {}
    func delete(episodeID: UUID) {}
}

// MARK: - ITunesSearchClient

@Observable
@MainActor
final class ITunesSearchClient {
    struct Result: Identifiable, Hashable, Sendable {
        var id: Int { collectionId }
        var collectionId: Int
        var collectionName: String
        var artistName: String?
        var feedURL: URL?
        var artworkURL: URL?
        var trackCount: Int?
        var primaryGenreName: String?
    }

    static func search(_ term: String) async throws -> [Result] { [] }
    static func topPodcasts() async throws -> [Result] { [] }
}

// MARK: - NostrPodcastDiscoveryService

@Observable
@MainActor
final class NostrPodcastDiscoveryService {
    struct ShowResult: Identifiable, Hashable, Sendable {
        var id: String { coordinate }
        var coordinate: String
        var title: String
        var description: String = ""
        var author: String = ""
        var imageURL: URL?
        var feedURL: URL?
        var authorPubkey: String?
        var language: String?
        var categories: [String] = []
    }

    static func podcastID(for coordinate: String) -> UUID { UUID() }
    func fetchShows(relayURL: URL?) async -> [ShowResult] { [] }
    func subscribe(to show: ShowResult, store: AppStateStore, relayURL: URL?) async -> Podcast { Podcast() }
}

// MARK: - DeepLinkHandler additions

extension DeepLinkHandler {
    static func episodeGUIDDeepLink(guid: String) -> String? { nil }
    static func episodeGUIDURL(guid: String, startTime: TimeInterval) -> URL? { nil }
}

// MARK: - PlayerShareSheet

struct PlayerShareSheet: View {
    var url: URL
    var title: String
    var body: some View { EmptyView() }

    static func isMeaningfulPlayhead(_ time: TimeInterval) -> Bool {
        time > 5
    }
}

// MARK: - Haptics additions

extension Haptics {
    static func itemReopen() {}
    static func itemComplete() {}
}

// MARK: - SubscriptionImportPayload

struct SubscriptionImportPayload: Sendable {
    var podcast: Podcast
}

// MARK: - OPMLImport

struct OPMLImport {
    func parseOPML(data: Data) throws -> [Podcast] { [] }
}

// MARK: - EpisodeDetailView

struct EpisodeDetailView: View {
    let episodeID: UUID
    var body: some View { Text("Episode").navigationTitle("Episode") }
}

// MARK: - EpisodeShowNotesFormatter

enum EpisodeShowNotesFormatter {
    static func plainText(from html: String) -> String {
        guard let data = html.data(using: .utf8) else { return html }
        let opts: [NSAttributedString.DocumentReadingOptionKey: Any] = [
            .documentType: NSAttributedString.DocumentType.html,
            .characterEncoding: String.Encoding.utf8.rawValue
        ]
        let str = (try? NSAttributedString(data: data, options: opts, documentAttributes: nil))?.string
        return str ?? html
    }
}
