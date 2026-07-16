import Foundation

extension UserIdentityStore {
    func publishProfile(
        name _: String,
        displayName _: String,
        about _: String,
        picture _: String
    ) async throws -> SignedNostrEvent {
        throw Pod0HumanPublicationError.durableCorrelationUnavailable(issue: 591)
    }

    func publishUserNote(
        _: Note,
        episodeCoord _: String?
    ) async throws -> SignedNostrEvent {
        throw Pod0HumanPublicationError.durableCorrelationUnavailable(issue: 591)
    }

    func publishUserClip(
        _: Clip,
        episode _: Episode? = nil,
        podcast _: Podcast? = nil
    ) async throws -> SignedNostrEvent {
        throw Pod0HumanPublicationError.durableCorrelationUnavailable(issue: 591)
    }

}

enum Pod0HumanPublicationError: LocalizedError, Equatable {
    case durableCorrelationUnavailable(issue: Int)

    var errorDescription: String? {
        switch self {
        case .durableCorrelationUnavailable(let issue):
            "Publishing is unavailable until NMP issue #\(issue) provides crash-safe durable delivery tracking. Nothing was signed or queued."
        }
    }
}
