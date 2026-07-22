import Foundation
import Pod0Core

extension SharedLibraryClient {
    nonisolated func chapterModelPlan(
        episodeID: UUID,
        configuredModel: String
    ) -> ChapterModelPlan {
        facade.planChapterModelRequest(
            episodeId: EpisodeId(uuid: episodeID),
            configuredModel: configuredModel
        )
    }

    func executePendingHostRequests() {
        dispatcher.executePendingRequests(from: facade)
    }

    func cancelPendingHostRequests(cancellationID: CancellationId) {
        dispatcher.cancel(cancellationID: cancellationID)
    }

    func execute(_ command: ApplicationCommand) async throws -> OperationResult? {
        let commandID = CommandId(uuid: UUID())
        let cancellationID = CancellationId(uuid: UUID())
        return try await withCheckedThrowingContinuation { continuation in
            waiters[commandID] = Waiter(continuation: continuation)
            facade.dispatch(command: CommandEnvelope(
                commandId: commandID,
                cancellationId: cancellationID,
                expectedRevision: nil,
                command: command
            ))
            dispatcher.executePendingRequests(from: facade)
        }
    }

    func podcast(id: UUID) -> Podcast? {
        cachedSnapshot?.podcasts.first { $0.podcastId.uuid == id }?.swiftValue
    }

    func podcast(feedURL: URL) -> Podcast? {
        let key = feedURL.absoluteString.lowercased()
        return cachedSnapshot?.podcasts.first {
            $0.feedIdentity?.comparisonKey == key
        }?.swiftValue
    }

    func subscription(podcastID: UUID) -> PodcastSubscription? {
        cachedSnapshot?.subscriptions.first {
            $0.podcastId.uuid == podcastID
        }?.swiftValue
    }
}
