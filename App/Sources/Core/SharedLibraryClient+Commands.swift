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

    /// Dispatches a commit-immediately command and reads its outcome from the
    /// post-dispatch operation projection. Unlike `execute`, this never parks
    /// a continuation waiting for host work to finish: from contract version
    /// 53 the feed family succeeds once the intent is durably queued, and any
    /// fetch it triggers is background workflow state projected separately.
    func executeCommitted(_ command: ApplicationCommand) async throws -> OperationResult? {
        await subscriptionTask?.value
        await initialProjectionTask?.value
        let commandID = CommandId(uuid: UUID())
        dispatchCoreCommand(command, commandID: commandID)
        await coreCommandTail?.value
        let envelope = await coreSnapshot(ProjectionRequest(
            scope: .library,
            offset: 0,
            maxItems: 1
        ))
        guard case .library(let value) = envelope.projection,
              let operation = value.operations.last(where: { $0.commandId == commandID })
        else { throw SharedLibraryError.unavailable }
        switch operation.stage {
        case .failed, .cancelled, .unsupported:
            throw SharedLibraryError(operation.failure?.code)
        default:
            return operation.result
        }
    }

    func execute(_ command: ApplicationCommand) async throws -> OperationResult? {
        await subscriptionTask?.value
        await initialProjectionTask?.value
        let commandID = CommandId(uuid: UUID())
        let cancellationID = CancellationId(uuid: UUID())
        let result = try await withCheckedThrowingContinuation { continuation in
            waiters[commandID] = Waiter(continuation: continuation)
            dispatchCoreCommand(
                command,
                commandID: commandID,
                cancellationID: cancellationID
            )
        }
        await libraryProjectionTask?.value
        return result
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
