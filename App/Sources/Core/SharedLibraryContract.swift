import Foundation
import Pod0Core

struct SharedLibrarySnapshot {
    let podcasts: [PodcastRecord]
    let subscriptions: [PodcastSubscriptionRecord]
    let episodes: [EpisodeRecord]
    let operations: [OperationProjection]
}

enum SharedLibraryError: Error, LocalizedError, Equatable {
    case invalidURL
    case malformedFeed
    case alreadySubscribed
    case notFound
    case unavailable
    case cancelled
    case invalidNote
    case revisionConflict

    init(_ code: CoreFailureCode?) {
        self = switch code {
        case .invalidFeedUrl: .invalidURL
        case .feedMalformed: .malformedFeed
        case .alreadySubscribed: .alreadySubscribed
        case .notFound: .notFound
        case .cancelled: .cancelled
        case .invalidNote: .invalidNote
        case .revisionConflict: .revisionConflict
        default: .unavailable
        }
    }

    var errorDescription: String? {
        switch self {
        case .invalidURL: "That doesn't look like a valid feed URL."
        case .malformedFeed: "Pod0 couldn't read a podcast feed at that address."
        case .alreadySubscribed: "You're already subscribed to this podcast."
        case .notFound: "That podcast is no longer in your library."
        case .unavailable: "Your library is temporarily unavailable."
        case .cancelled: "The library request was cancelled."
        case .invalidNote: "The note is empty or contains an invalid anchor."
        case .revisionConflict: "That note changed before this edit could be saved."
        }
    }
}
