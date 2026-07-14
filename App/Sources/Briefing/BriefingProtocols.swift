import Foundation

// MARK: - Cross-lane protocols
//
// Lane 9 owns the briefing system but consumes RAG (Lane 6), wiki storage
// (Lane 7), TTS (Lane 8), and the audio playback host (Lane 1). Each
// dependency is expressed here as a protocol so this lane builds and ships
// in isolation; Lanes 1/6/7/8 will provide concrete implementations later.

// MARK: RAG search

/// Minimum surface the briefing composer needs from the embeddings/RAG stack.
///
/// Lane 6 ships the concrete implementation that hits the on-device vector
/// index plus reranker (`embeddings-rag-stack.md`). For this lane we only
/// need *retrieval* — write operations are out of scope.
protocol BriefingRAGSearchProtocol: Sendable {

    /// Hybrid (lexical + vector) search over transcript chunks and wiki pages.
    ///
    /// - Parameters:
    ///   - query: Natural-language query string. Composer hands it the user's
    ///     freeform brief plus the style template's seed query.
    ///   - scope: Narrow the corpus to a slice of the user's library.
    ///   - limit: Maximum results. The composer typically asks for 12–30.
    /// - Returns: Ranked candidates suitable for inclusion in a briefing.
    func search(
        query: String,
        scope: BriefingScope,
        limit: Int
    ) async throws -> [RAGCandidate]
}

/// A single retrieval result the composer can cite. Conceptually a row from
/// either a transcript chunk or a wiki page, normalised so the composer
/// doesn't need to special-case sources.
struct RAGCandidate: Sendable, Hashable, Identifiable {
    var id: UUID
    var sourceKind: SourceKind
    /// FK into the source episode (when `sourceKind == .episode`).
    var episodeID: UUID?
    /// Episode enclosure URL — the composer hands this through to
    /// `BriefingQuote.enclosureURL` so the stitcher never has to look up
    /// `Episode` itself.
    var enclosureURL: URL?
    /// FK into the source wiki page (when `sourceKind == .wiki`).
    var wikiPageID: UUID?
    /// Display label used by the attribution chip.
    var sourceLabel: String
    /// The retrieved text content.
    var text: String
    /// Time grounding inside the source episode (transcript chunks only).
    var startSeconds: TimeInterval?
    var endSeconds: TimeInterval?
    /// Reranker score; higher is more relevant.
    var score: Double

    enum SourceKind: String, Codable, Sendable, Hashable {
        case episode
        case wiki
    }
}

// MARK: Wiki storage

/// Read-only slice of Lane 7's wiki repository the composer needs.
///
/// The composer calls into wiki storage for *catch-up* and *topic deep-dive*
/// briefings where a wiki page provides the structural backbone of the script.
protocol BriefingWikiStorageProtocol: Sendable {

    /// Returns the wiki page with the given id, or `nil` if not found.
    func wikiPage(id: UUID) async throws -> WikiPage?

    /// Returns wiki pages whose title matches `titleQuery` (substring, case-
    /// insensitive). Used by the topic-deep-dive flow to discover entry pages.
    func wikiPages(matchingTitle titleQuery: String) async throws -> [WikiPage]
}

// MARK: TTS

/// Surface Lane 8 will provide for synthesising narration.
///
/// The protocol is intentionally tiny: hand it text + voice id, get back a
/// playable audio file URL. The fake (see `BriefingFakes.swift`) writes a
/// silent m4a of the correct length so the rest of the briefing pipeline can
/// exercise its real data flow with no ElevenLabs key present.
protocol TTSProtocol: Sendable {

    /// Synthesise `text` into a writable audio file at `outputURL`.
    ///
    /// - Parameters:
    ///   - text: Plain-text narration. SSML / phoneme tags are TTS-impl-defined.
    ///   - voiceID: Provider-specific voice identifier. May be empty to use a
    ///     default; concrete providers should not throw on empty.
    ///   - outputURL: Target file URL. Caller owns the path; the implementation
    ///     overwrites any existing file.
    /// - Returns: Realised duration in seconds. The composer trusts this value
    ///   to populate `BriefingTrack.endInTrackSeconds`.
    func synthesize(
        text: String,
        voiceID: String,
        outputURL: URL
    ) async throws -> TimeInterval
}

// MARK: Audio playback host

/// Surface Lane 1 (`AudioEngine`) will provide so the briefing player can
/// hand off playback rather than spawning a competing AVPlayer. The briefing
/// engine adapts a sequence of `BriefingTrack`s into the host's expected
/// "single asset URL + position" model.
@MainActor
protocol BriefingPlayerHostProtocol: AnyObject {

    /// Begin playback of `assetURL`. The host owns lock-screen / Now Playing /
    /// CarPlay integration; the briefing engine never touches `MPNowPlayingInfoCenter`.
    func play(assetURL: URL, startAt seconds: TimeInterval) async

    /// Pause whatever is playing, leaving the asset loaded so resume is sample-
    /// accurate (the *pause-and-resume* branch contract).
    func pause() async

    /// Resume from the current pause position.
    func resume() async

    /// Seek to an absolute time inside the loaded asset.
    func seek(to seconds: TimeInterval) async

    /// Current playback time in the loaded asset, sampled when called.
    var currentTimeSeconds: TimeInterval { get }

    /// Invoked when the currently-loaded asset plays to completion. The
    /// engine wires this to the `.briefingPlaybackEnded` notification so the
    /// river coordinator (`BriefingRiverView`) can auto-advance to the next
    /// briefing without polling. Set by `BriefingPlayerEngine.load(_:tracks:host:)`;
    /// hosts should call it on the main actor exactly once per playthrough.
    var onPlaybackEnded: (@MainActor () -> Void)? { get set }
}
