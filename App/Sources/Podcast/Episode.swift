import Foundation

/// A persisted podcast episode. `guid` is stable across feed refreshes.
struct Episode: Codable, Sendable, Identifiable, Hashable {
    /// Stable local identifier. Distinct from `guid`.
    var id: UUID
    /// Foreign key to the parent `Podcast.id` (including synthetic podcasts).
    var podcastID: UUID
    /// Publisher's `<guid>` (or synthetic fallback). Used to dedupe across feed
    /// re-fetches and to key cross-system records (vector store, Spotlight).
    var guid: String

    var title: String
    /// Show notes / episode description (HTML or plain text).
    var description: String
    /// `<pubDate>` parsed via RFC 822 / RFC 1123.
    var pubDate: Date
    /// Episode duration in seconds. Comes from `<itunes:duration>`; format is
    /// either `H:MM:SS`, `MM:SS`, or raw seconds.
    var duration: TimeInterval?

    /// Direct media URL (`<enclosure url="…">`). Required for playback.
    var enclosureURL: URL
    /// Optional `<enclosure type="…">`, e.g. `audio/mpeg`.
    var enclosureMimeType: String?

    /// Per-episode artwork override (`<itunes:image>`). Falls back to the
    /// subscription's `imageURL` when nil.
    var imageURL: URL?

    // MARK: - Podcasting 2.0 enrichment

    /// Replaceable native projection of the Rust-selected chapter artifact.
    /// Legacy metadata still decodes this field once for bootstrap import, but
    /// Swift never persists it after shared-core authority activates.
    var chapters: [Chapter]?
    /// `<podcast:person>` entries (host, guest, etc.) attached to the episode.
    var persons: [Person]?
    /// `<podcast:soundbite>` callouts: short hype clips authored by publisher.
    var soundBites: [SoundBite]?

    /// `<podcast:transcript url=…>` resolved URL when the publisher ships one.
    /// Lane 5 fetches and parses based on `publisherTranscriptType`.
    var publisherTranscriptURL: URL?
    /// Classified MIME type for `publisherTranscriptURL`. See `TranscriptKind`.
    var publisherTranscriptType: TranscriptKind?
    /// `<podcast:chapters url=… type=application/json+chapters>` JSON URL.
    var chaptersURL: URL?

    // MARK: - User-mutable playback state

    /// Last-known playback position in seconds. Persisted across launches.
    var playbackPosition: TimeInterval
    /// `true` once the user (or auto-played threshold) marked the episode done.
    var played: Bool
    /// User-flagged "starred" / favorite. Toggled from Library context menu.
    var isStarred: Bool
    /// Lifecycle of the local enclosure download.
    var downloadState: DownloadState
    /// Lifecycle of transcript ingestion.
    var transcriptState: TranscriptState
    /// Optional user-selected provider for the next durable ingest attempt.
    var requestedTranscriptProvider: STTProvider?
    /// Replaceable native projection of Rust-selected ad spans. `nil` means
    /// the selected artifact has not evaluated ads; an empty array means Rust
    /// evaluated the episode and found none. Legacy state remains decode-only.
    var adSegments: [AdSegment]?
    /// Non-nil for agent-generated episodes. Records where the generation was
    /// commissioned from so the player can surface a tappable source link.
    var generationSource: GenerationSource?


    init(
        id: UUID = UUID(),
        podcastID: UUID,
        guid: String,
        title: String,
        description: String = "",
        pubDate: Date,
        duration: TimeInterval? = nil,
        enclosureURL: URL,
        enclosureMimeType: String? = nil,
        imageURL: URL? = nil,
        chapters: [Chapter]? = nil,
        persons: [Person]? = nil,
        soundBites: [SoundBite]? = nil,
        publisherTranscriptURL: URL? = nil,
        publisherTranscriptType: TranscriptKind? = nil,
        chaptersURL: URL? = nil,
        playbackPosition: TimeInterval = 0,
        played: Bool = false,
        isStarred: Bool = false,
        downloadState: DownloadState = .notDownloaded,
        transcriptState: TranscriptState = .none,
        requestedTranscriptProvider: STTProvider? = nil,
        adSegments: [AdSegment]? = nil,
        generationSource: GenerationSource? = nil
    ) {
        self.id = id
        self.podcastID = podcastID
        self.guid = guid
        self.title = title
        self.description = description
        self.pubDate = pubDate
        self.duration = duration
        self.enclosureURL = enclosureURL
        self.enclosureMimeType = enclosureMimeType
        self.imageURL = imageURL
        self.chapters = chapters
        self.persons = persons
        self.soundBites = soundBites
        self.publisherTranscriptURL = publisherTranscriptURL
        self.publisherTranscriptType = publisherTranscriptType
        self.chaptersURL = chaptersURL
        self.playbackPosition = playbackPosition
        self.played = played
        self.isStarred = isStarred
        self.downloadState = downloadState
        self.transcriptState = transcriptState
        self.requestedTranscriptProvider = requestedTranscriptProvider
        self.adSegments = adSegments
        self.generationSource = generationSource
    }

    // MARK: - Codable (forward-compat decoding)

    private enum CodingKeys: String, CodingKey {
        case id, podcastID, guid, title, description, pubDate, duration
        case enclosureURL, enclosureMimeType, imageURL
        case chapters, persons, soundBites
        case publisherTranscriptURL, publisherTranscriptType, chaptersURL
        case playbackPosition, played, isStarred, downloadState, transcriptState
        case requestedTranscriptProvider
        case adSegments, generationSource
        // Legacy key from the pre-split shape. Decoded as a fallback when
        // `podcastID` is absent. Never written.
        case legacy_subscriptionID = "subscriptionID"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        if let pid = try c.decodeIfPresent(UUID.self, forKey: .podcastID) {
            podcastID = pid
        } else {
            // Pre-split persisted record: the FK was named `subscriptionID`.
            // Values remain valid because Podcast.id reuses the legacy UUID.
            podcastID = try c.decode(UUID.self, forKey: .legacy_subscriptionID)
        }
        guid = try c.decode(String.self, forKey: .guid)
        title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        pubDate = try c.decodeIfPresent(Date.self, forKey: .pubDate) ?? Date(timeIntervalSince1970: 0)
        duration = try c.decodeIfPresent(TimeInterval.self, forKey: .duration)
        enclosureURL = try c.decode(URL.self, forKey: .enclosureURL)
        enclosureMimeType = try c.decodeIfPresent(String.self, forKey: .enclosureMimeType)
        imageURL = try c.decodeIfPresent(URL.self, forKey: .imageURL)
        let loadLegacyChapterAdjuncts =
            decoder.userInfo[.loadLegacyChapterAdjuncts] as? Bool ?? true
        chapters = loadLegacyChapterAdjuncts
            ? try c.decodeIfPresent([Chapter].self, forKey: .chapters)
            : nil
        persons = try c.decodeIfPresent([Person].self, forKey: .persons)
        soundBites = try c.decodeIfPresent([SoundBite].self, forKey: .soundBites)
        publisherTranscriptURL = try c.decodeIfPresent(URL.self, forKey: .publisherTranscriptURL)
        publisherTranscriptType = try c.decodeIfPresent(TranscriptKind.self, forKey: .publisherTranscriptType)
        chaptersURL = try c.decodeIfPresent(URL.self, forKey: .chaptersURL)
        playbackPosition = try c.decodeIfPresent(TimeInterval.self, forKey: .playbackPosition) ?? 0
        played = try c.decodeIfPresent(Bool.self, forKey: .played) ?? false
        isStarred = try c.decodeIfPresent(Bool.self, forKey: .isStarred) ?? false
        downloadState = try c.decodeIfPresent(DownloadState.self, forKey: .downloadState) ?? .notDownloaded
        transcriptState = try c.decodeIfPresent(TranscriptState.self, forKey: .transcriptState) ?? .none
        requestedTranscriptProvider = try c.decodeIfPresent(
            STTProvider.self,
            forKey: .requestedTranscriptProvider
        )
        adSegments = loadLegacyChapterAdjuncts
            ? try c.decodeIfPresent([AdSegment].self, forKey: .adSegments)
            : nil
        generationSource = try c.decodeIfPresent(GenerationSource.self, forKey: .generationSource)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(podcastID, forKey: .podcastID)
        try c.encode(guid, forKey: .guid)
        try c.encode(title, forKey: .title)
        try c.encode(description, forKey: .description)
        try c.encode(pubDate, forKey: .pubDate)
        try c.encodeIfPresent(duration, forKey: .duration)
        try c.encode(enclosureURL, forKey: .enclosureURL)
        try c.encodeIfPresent(enclosureMimeType, forKey: .enclosureMimeType)
        try c.encodeIfPresent(imageURL, forKey: .imageURL)
        // Rust owns durable chapters; the legacy key remains decode-only.
        try c.encodeIfPresent(persons, forKey: .persons)
        try c.encodeIfPresent(soundBites, forKey: .soundBites)
        try c.encodeIfPresent(publisherTranscriptURL, forKey: .publisherTranscriptURL)
        try c.encodeIfPresent(publisherTranscriptType, forKey: .publisherTranscriptType)
        try c.encodeIfPresent(chaptersURL, forKey: .chaptersURL)
        try c.encode(playbackPosition, forKey: .playbackPosition)
        try c.encode(played, forKey: .played)
        try c.encode(isStarred, forKey: .isStarred)
        // Rust owns durable download state; the legacy key remains decode-only.
        // Rust owns durable readiness; decode the legacy key but never re-encode it.
        try c.encodeIfPresent(requestedTranscriptProvider, forKey: .requestedTranscriptProvider)
        // Rust owns durable ad spans; the legacy key remains decode-only.
        try c.encodeIfPresent(generationSource, forKey: .generationSource)
    }
}

// MARK: - Podcasting 2.0 substructs

extension BidirectionalCollection where Element == Episode.Chapter {
    /// Returns the chapter that contains `playheadSeconds` — the last chapter
    /// whose `startTime <= playheadSeconds`. Falls back to the first chapter
    /// when the playhead sits before any chapter starts (typical at t=0
    /// before playback begins) so consumers always have an active indicator
    /// instead of a phantom none-selected state.
    func active(at playheadSeconds: TimeInterval) -> Episode.Chapter? {
        if let hit = self.last(where: { $0.startTime <= playheadSeconds }) {
            return hit
        }
        return self.first
    }
}

extension Episode {
    /// A chapter marker. Either inline (parsed from a `<podcast:chapter>`-like
    /// element) or hydrated later from `chaptersURL` JSON.
    struct Chapter: Codable, Sendable, Hashable, Identifiable {
        var id: UUID
        /// Start time in seconds from the beginning of the episode.
        var startTime: TimeInterval
        /// Optional end time. When `nil`, chapter ends at the next chapter or EOF.
        var endTime: TimeInterval?
        var title: String
        /// Optional `img` URL from Podcasting 2.0 chapters JSON.
        var imageURL: URL?
        /// Optional `url` reference (publisher show notes link).
        var linkURL: URL?
        /// `toc=false` chapters are non-navigable (e.g. ad markers).
        var includeInTableOfContents: Bool
        /// `true` when the Rust-selected artifact was generated from a
        /// transcript rather than parsed from a publisher source.
        /// Defaults to `false`; the player surfaces a subtle "AI" tag for true.
        var isAIGenerated: Bool
        /// 1–2 sentence summary selected by the shared core alongside generated
        /// boundaries or a publisher enrichment pass. `nil` until enrichment.
        var summary: String?
        /// UUID string of the source episode when this chapter corresponds to a
        /// clip from another episode (e.g. an agent-generated TTS episode with
        /// snippet turns). Drives the clip-source chip in `PlayerView` and the
        /// mid-play artwork swap.
        var sourceEpisodeID: String?

        init(
            id: UUID = UUID(),
            startTime: TimeInterval,
            endTime: TimeInterval? = nil,
            title: String,
            imageURL: URL? = nil,
            linkURL: URL? = nil,
            includeInTableOfContents: Bool = true,
            isAIGenerated: Bool = false,
            summary: String? = nil,
            sourceEpisodeID: String? = nil
        ) {
            self.id = id
            self.startTime = startTime
            self.endTime = endTime
            self.title = title
            self.imageURL = imageURL
            self.linkURL = linkURL
            self.includeInTableOfContents = includeInTableOfContents
            self.isAIGenerated = isAIGenerated
            self.summary = summary
            self.sourceEpisodeID = sourceEpisodeID
        }

        private enum CodingKeys: String, CodingKey {
            case id, startTime, endTime, title, imageURL, linkURL
            case includeInTableOfContents, isAIGenerated, summary, sourceEpisodeID
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decode(UUID.self, forKey: .id)
            startTime = try c.decode(TimeInterval.self, forKey: .startTime)
            endTime = try c.decodeIfPresent(TimeInterval.self, forKey: .endTime)
            title = try c.decodeIfPresent(String.self, forKey: .title) ?? ""
            imageURL = try c.decodeIfPresent(URL.self, forKey: .imageURL)
            linkURL = try c.decodeIfPresent(URL.self, forKey: .linkURL)
            includeInTableOfContents = try c.decodeIfPresent(Bool.self, forKey: .includeInTableOfContents) ?? true
            isAIGenerated = try c.decodeIfPresent(Bool.self, forKey: .isAIGenerated) ?? false
            summary = try c.decodeIfPresent(String.self, forKey: .summary)
            sourceEpisodeID = try c.decodeIfPresent(String.self, forKey: .sourceEpisodeID)
        }
    }

    /// `<podcast:person>` — host / guest / cohost / etc.
    /// See podcasting2.org/docs/podcast-namespace/tags/person.
    struct Person: Codable, Sendable, Hashable, Identifiable {
        var id: UUID
        /// Display name (element text).
        var name: String
        /// `role` attribute: host, guest, cohost, …. Stored verbatim
        /// (case-insensitive); lower-cased for comparison.
        var role: String?
        /// `group` attribute (e.g. cast, writing). Optional.
        var group: String?
        /// `img` attribute — headshot URL.
        var imageURL: URL?
        /// `href` attribute — link to the person's homepage / social.
        var linkURL: URL?

        init(
            id: UUID = UUID(),
            name: String,
            role: String? = nil,
            group: String? = nil,
            imageURL: URL? = nil,
            linkURL: URL? = nil
        ) {
            self.id = id
            self.name = name
            self.role = role
            self.group = group
            self.imageURL = imageURL
            self.linkURL = linkURL
        }
    }

    /// `<podcast:soundbite>` — a short, publisher-curated highlight clip.
    struct SoundBite: Codable, Sendable, Hashable, Identifiable {
        var id: UUID
        /// `startTime` attribute, seconds.
        var startTime: TimeInterval
        /// `duration` attribute, seconds.
        var duration: TimeInterval
        /// Optional element text — a human-friendly title.
        var title: String?

        init(
            id: UUID = UUID(),
            startTime: TimeInterval,
            duration: TimeInterval,
            title: String? = nil
        ) {
            self.id = id
            self.startTime = startTime
            self.duration = duration
            self.title = title
        }
    }

    /// Transient native projection of an ad span in the Rust-selected chapter
    /// artifact. Rust owns auto-skip decisions; native uses this value to draw
    /// chapter overlap affordances.
    struct AdSegment: Codable, Sendable, Hashable, Identifiable {
        var id: UUID
        /// Start of the ad in seconds from the beginning of the episode.
        var start: TimeInterval
        /// End of the ad in seconds. Always greater than `start`.
        var end: TimeInterval
        /// Where in the episode this ad sits — pre-roll, mid-roll, or
        /// post-roll. Drives the "Skip 30s ad" pre-roll affordance.
        var kind: AdKind

        init(
            id: UUID = UUID(),
            start: TimeInterval,
            end: TimeInterval,
            kind: AdKind
        ) {
            self.id = id
            self.start = start
            self.end = end
            self.kind = kind
        }

        private enum CodingKeys: String, CodingKey {
            case id, start, end, kind
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
            start = try c.decode(TimeInterval.self, forKey: .start)
            end = try c.decode(TimeInterval.self, forKey: .end)
            kind = try c.decodeIfPresent(AdKind.self, forKey: .kind) ?? .midroll
        }
    }

    /// Classification for an `AdSegment`. `preroll` ads anchor the
    /// "Skip 30s ad" button above the scrubber; `midroll` is the common
    /// case; `postroll` segments are flagged but don't drive the pre-roll UI.
    enum AdKind: String, Codable, Sendable, Hashable, CaseIterable {
        case preroll
        case midroll
        case postroll
    }

    /// Records where an agent-generated episode was commissioned from.
    /// Stored on the episode so the player can surface a tappable source link.
    enum GenerationSource: Sendable, Equatable, Hashable {
        case inAppChat(conversationID: UUID)
    }
}
