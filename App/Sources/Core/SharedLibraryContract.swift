import Foundation
import Pod0Core

struct SharedLibrarySnapshot {
    let podcasts: [PodcastRecord]
    let subscriptions: [PodcastSubscriptionRecord]
    let episodes: [EpisodeRecord]
    let chaptersByEpisodeID: [UUID: SharedChapterSnapshot]
    let operations: [OperationProjection]

    func hasSameReadModel(as other: SharedLibrarySnapshot) -> Bool {
        podcasts == other.podcasts
            && subscriptions == other.subscriptions
            && episodes == other.episodes
            && chaptersByEpisodeID == other.chaptersByEpisodeID
    }
}

enum SharedLibraryError: Error, LocalizedError, Equatable {
    case invalidURL
    case malformedFeed
    case alreadySubscribed
    case notFound
    case unavailable
    case cancelled
    case invalidMemory
    case invalidNote
    case invalidClip
    case invalidTranscript
    case invalidChapter
    case revisionConflict

    init(_ code: CoreFailureCode?) {
        self = switch code {
        case .invalidFeedUrl: .invalidURL
        case .feedMalformed: .malformedFeed
        case .alreadySubscribed: .alreadySubscribed
        case .notFound: .notFound
        case .cancelled: .cancelled
        case .invalidMemory: .invalidMemory
        case .invalidNote: .invalidNote
        case .invalidClip: .invalidClip
        case .invalidTranscript: .invalidTranscript
        case .invalidChapter: .invalidChapter
        case .revisionConflict: .revisionConflict
        default: .unavailable
        }
    }

    var errorDescription: String? {
        switch self {
        case .invalidURL: "That doesn't look like a valid feed URL."
        case .malformedFeed: "Pod0 couldn't read a podcast feed at that address."
        case .alreadySubscribed: "You're already subscribed to this podcast."
        case .notFound: "That item is no longer in your library."
        case .unavailable: "Your library is temporarily unavailable."
        case .cancelled: "The library request was cancelled."
        case .invalidMemory: "That memory is empty or too large."
        case .invalidNote: "The note is empty or contains an invalid anchor."
        case .invalidClip: "That clip has invalid timestamps or transcript context."
        case .invalidTranscript: "That transcript contains invalid timing or provenance."
        case .invalidChapter: "Those chapters contain invalid timing or provenance."
        case .revisionConflict: "That item changed before this edit could be saved."
        }
    }
}
