import Foundation
import Pod0Core

extension SubscriptionService {
    /// Feed commands commit durably at dispatch. `executeCommitted` reads the
    /// terminal pre-commit outcome (invalid URL, already subscribed) without
    /// ever parking a continuation on the fetch round trip.
    func executeShared(_ command: ApplicationCommand) async throws -> OperationResult? {
        guard let sharedLibrary = store.sharedLibrary else {
            throw AddError.transport("Shared library unavailable")
        }
        do {
            return try await sharedLibrary.executeCommitted(command)
        } catch let error as SharedLibraryError {
            switch error {
            case .invalidURL: throw AddError.invalidURL
            case .malformedFeed: throw AddError.parse(error.localizedDescription)
            case .alreadySubscribed:
                throw AddError.alreadySubscribed(title: "this podcast")
            case .notFound, .unavailable, .cancelled, .invalidMemory, .invalidNote, .invalidClip,
                 .invalidTranscript, .invalidChapter, .revisionConflict:
                throw AddError.transport(error.localizedDescription)
            }
        }
    }

    /// Resolves the committed podcast record. The native cache updates
    /// asynchronously, so fall back to reading the just-committed row
    /// straight from the facade projection.
    func resolvedPodcast(from result: OperationResult?) async throws -> Podcast {
        guard case .podcast(let podcastID) = result, let uuid = podcastID.uuid
        else { throw AddError.transport("Shared library projection unavailable") }
        if let podcast = store.podcast(id: uuid) { return podcast }
        guard let sharedLibrary = store.sharedLibrary else {
            throw AddError.transport("Shared library unavailable")
        }
        let envelope = await sharedLibrary.coreSnapshot(ProjectionRequest(
            scope: .podcastDetail(podcastId: podcastID),
            offset: 0,
            maxItems: 1
        ))
        guard case .podcastDetail(let detail) = envelope.projection,
              let record = detail.podcast
        else { throw AddError.transport("Shared library projection unavailable") }
        return record.swiftValue
    }
}
