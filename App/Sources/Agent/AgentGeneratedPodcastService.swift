import Foundation
import os.log

// MARK: - AgentGeneratedPodcastService
//
// Manages the "Agent Generated" virtual `Podcast` and its on-disk episode
// audio files.
//
// The podcast is feed-less (`feedURL == nil`) so it is excluded from OPML
// export, feed-refresh scheduling, and the download service's auto-download
// evaluator. Callers branch on `Podcast.feedURL == nil` rather than comparing
// URL strings.
//
// The user is NOT auto-subscribed to this podcast — it has a `Podcast` row but
// no `PodcastSubscription`. Episodes still appear in the library through the
// normal episode list; the show simply doesn't show up in the user's
// followed-podcasts list.
//
// Audio files live under:
//   Application Support / podcastr / agent-episodes / <episodeID>.m4a
//
// Each call to `publishEpisode` creates a new `Episode` with
// `downloadState = .downloaded` and a `file://` `enclosureURL` pointing at
// the stitched m4a. The player reads local URLs exactly like remote ones
// through the existing AudioEngine local-file fallback.

struct AgentGeneratedPodcastService: Sendable {

    private static let logger = Logger.app("AgentGeneratedPodcastService")

    // MARK: - Storage

    /// Root directory for agent-generated episode audio files.
    static func audioDirectory() throws -> URL {
        let support = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let dir = support
            .appendingPathComponent("podcastr", isDirectory: true)
            .appendingPathComponent("agent-episodes", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    /// Returns the canonical m4a URL for an agent episode by its UUID.
    /// The file need not exist yet — callers write here before calling
    /// `publishEpisode`.
    static func audioFileURL(episodeID: UUID) throws -> URL {
        try audioDirectory().appendingPathComponent("\(episodeID.uuidString).m4a")
    }

    // MARK: - Podcast row management

    /// Returns the stable id of the default "Agent Generated" podcast, seeding
    /// the feed-less row into the Rust kernel store (the source of truth). Safe
    /// to call repeatedly: `create_podcast` is idempotent on id in the kernel.
    ///
    /// Seeding the kernel row is what lets the show — and the episodes added
    /// under it via `kernelAddEpisode` — survive the `applyKernelState`
    /// full-replace tick. The row appears in the UI on the next projection push.
    @MainActor
    static func ensurePodcastID(in store: AppStateStore) -> UUID? {
        guard let descriptor = agentGeneratedPodcastDescriptor(in: store),
              let id = UUID(uuidString: descriptor.podcastID) else {
            return nil
        }
        // Kernel SSOT: idempotent insert keyed on the stable id.
        store.kernelCreatePodcast(
            podcastId: id.uuidString,
            title: descriptor.title,
            description: descriptor.description,
            author: descriptor.author,
            feedUrl: nil,
            artworkUrl: nil,
            language: nil,
            categories: descriptor.categories,
            visibility: descriptor.visibility,
            titleIsPlaceholder: descriptor.titleIsPlaceholder
        )
        return id
    }

    @MainActor
    private static func agentGeneratedPodcastDescriptor(in store: AppStateStore) -> AgentGeneratedPodcastDescriptor? {
        guard let response = store.kernel?.agentGeneratedPodcastDescriptorEnvelope(),
              let data = response.data(using: .utf8),
              let envelope = try? KernelDecoding.makeDecoder().decode(AgentGeneratedPodcastDescriptorEnvelope.self, from: data),
              envelope.error == nil else {
            return nil
        }
        return envelope.result
    }

    // MARK: - Episode publishing

    /// Registers a finished m4a file as an episode on the agent-generated
    /// podcast and inserts it into `AppStateStore`. The file at `audioURL`
    /// must already exist on disk.
    ///
    /// - Parameter imageURL: Optional artwork to attach directly to the
    ///   episode. The TTS composer passes the source-clip artwork from the
    ///   first snippet chapter so the produced episode has meaningful art
    ///   even though the feed-less podcast itself has none.
    @MainActor
    @discardableResult
    static func publishEpisode(
        title: String,
        description: String,
        audioURL: URL,
        durationSeconds: TimeInterval?,
        imageURL: URL? = nil,
        generationSource: Episode.GenerationSource? = nil,
        targetPodcastID: UUID? = nil,
        in store: AppStateStore
    ) throws -> Episode {
        guard let podcastID = targetPodcastID ?? ensurePodcastID(in: store) else {
            throw AgentGeneratedPodcastError.descriptorUnavailable
        }
        let episodeID = UUID()
        // Kernel SSOT: the audio is already on disk (a `file://` enclosure), so
        // `add_episode` marks it Downloaded + wires the local-path side-map. The
        // episode rides the next projection push back into `store.episodes`.
        store.kernelAddEpisode(
            podcastId: podcastID.uuidString,
            episodeId: episodeID.uuidString,
            title: title,
            enclosureUrl: audioURL.absoluteString,
            description: description,
            durationSecs: durationSeconds,
            imageUrl: imageURL?.absoluteString,
            chapters: [],
            transcript: nil
        )
        logger.info("Published agent episode '\(title, privacy: .public)' id=\(episodeID, privacy: .public)")
        // A transient in-memory value for callers that need the produced
        // `Episode` synchronously (it is NOT written to the store — the
        // projection delivers the persisted copy on the next push).
        return Episode(
            id: episodeID,
            podcastID: podcastID,
            guid: episodeID.uuidString,
            title: title,
            description: description,
            pubDate: Date(),
            duration: durationSeconds,
            enclosureURL: audioURL,
            enclosureMimeType: "audio/mp4",
            imageURL: imageURL,
            downloadState: .downloaded(
                localFileURL: audioURL,
                byteCount: (try? FileManager.default.attributesOfItem(atPath: audioURL.path)[.size] as? Int64) ?? 0
            ),
            generationSource: generationSource
        )
    }

    private struct AgentGeneratedPodcastDescriptorEnvelope: Decodable {
        let result: AgentGeneratedPodcastDescriptor?
        let error: String?
    }

    private struct AgentGeneratedPodcastDescriptor: Decodable {
        let podcastID: String
        let title: String
        let description: String
        let author: String
        let visibility: String
        let titleIsPlaceholder: Bool
        let categories: [String]

        enum CodingKeys: String, CodingKey {
            case podcastID = "podcast_id"
            case title
            case description
            case author
            case visibility
            case titleIsPlaceholder = "title_is_placeholder"
            case categories
        }
    }
}

enum AgentGeneratedPodcastError: LocalizedError {
    case descriptorUnavailable

    var errorDescription: String? {
        switch self {
        case .descriptorUnavailable:
            return "Kernel did not provide the agent-generated podcast descriptor."
        }
    }
}
