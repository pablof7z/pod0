import Foundation
import Pod0Core

extension SubscriptionService {
    func executeShared(_ command: ApplicationCommand) async throws -> OperationResult? {
        guard let sharedLibrary = store.sharedLibrary else {
            throw AddError.transport("Shared library unavailable")
        }
        do {
            return try await sharedLibrary.execute(command)
        } catch let error as SharedLibraryError {
            switch error {
            case .invalidURL: throw AddError.invalidURL
            case .malformedFeed: throw AddError.parse(error.localizedDescription)
            case .alreadySubscribed:
                throw AddError.alreadySubscribed(title: "this podcast")
            case .notFound, .unavailable, .cancelled:
                throw AddError.transport(error.localizedDescription)
            }
        }
    }

    func resolvedPodcast(from result: OperationResult?) throws -> Podcast {
        guard case .podcast(let podcastID) = result,
              let uuid = podcastID.uuid,
              let podcast = store.podcast(id: uuid)
        else { throw AddError.transport("Shared library projection unavailable") }
        return podcast
    }
}
