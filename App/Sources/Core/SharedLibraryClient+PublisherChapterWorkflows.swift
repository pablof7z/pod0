import Foundation
import Pod0Core

extension SharedLibraryClient {
    func attach(workflowClient: WorkflowClient) {
        self.workflowClient = workflowClient
        let facade = facade
        workflowClient.attachPublisherChapterCore { query in
            await Task.detached(priority: .userInitiated) {
                Self.publisherChapterWorkflows(facade: facade, query: query)
            }.value
        }
    }

    /// Announces a native execution opportunity only. Rust derives whether a
    /// publisher workflow is needed from its authoritative episode metadata.
    func ensurePublisherChapters(episodeIDs: some Sequence<UUID>) {
        let episodeIDs = Set(episodeIDs)
            .subtracting(announcedPublisherChapterEpisodeIDs)
        announcedPublisherChapterEpisodeIDs.formUnion(episodeIDs)
        announcePublisherChapterOpportunity(episodeIDs: episodeIDs)
    }

    func announcePublisherSourceChanges(
        previous: SharedLibrarySnapshot?,
        current: SharedLibrarySnapshot
    ) {
        let previousEpisodes: [EpisodeRecord] = if let previous {
            previous.episodes
        } else {
            []
        }
        let previousSources: [UUID: String] = Dictionary(uniqueKeysWithValues:
            previousEpisodes.compactMap { record in
                record.episodeId.uuid.map {
                    ($0, record.feedMetadata.chaptersUrl ?? "")
                }
            }
        )
        let changed = current.episodes.compactMap { record -> UUID? in
            guard let id = record.episodeId.uuid,
                  previousSources[id] != (record.feedMetadata.chaptersUrl ?? "")
            else { return nil }
            return id
        }
        announcedPublisherChapterEpisodeIDs.formUnion(changed)
        announcePublisherChapterOpportunity(episodeIDs: changed)
    }

    private func announcePublisherChapterOpportunity(
        episodeIDs: some Sequence<UUID>
    ) {
        for episodeID in episodeIDs {
            facade.dispatch(command: CommandEnvelope(
                commandId: CommandId(uuid: UUID()),
                cancellationId: CancellationId(uuid: UUID()),
                expectedRevision: nil,
                command: .ensurePublisherChapters(episodeId: EpisodeId(uuid: episodeID))
            ))
        }
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
    }

    func performPublisherChapterAction(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) -> WorkflowJobActionResult {
        guard projection.authority == .sharedRustPublisherChapters,
              let expectedRevision = projection.coreWorkflowRevision,
              let current = publisherChapterWorkflow(episodeID: projection.subjectID)
        else { return .notFound }
        guard current.workflowRevision.value == expectedRevision else { return .stale }

        let command: ApplicationCommand
        switch action {
        case .retry where current.canRetry:
            command = .retryPublisherChapters(
                episodeId: current.episodeId,
                expectedWorkflowRevision: current.workflowRevision
            )
        case .cancel where current.canCancel:
            command = .cancelPublisherChapters(
                episodeId: current.episodeId,
                expectedWorkflowRevision: current.workflowRevision
            )
        default:
            return current.stage == .succeeded ? .alreadyComplete : .notAllowed
        }

        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(uuid: UUID()),
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: command
        ))
        let updated = publisherChapterWorkflow(episodeID: projection.subjectID)
        guard let updated, updated.workflowRevision.value > expectedRevision else { return .stale }
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
        return .accepted(action)
    }

    nonisolated func publisherChapterWorkflowSnapshots(
        episodeIDs: some Sequence<UUID>
    ) -> [PublisherChapterWorkflowProjection] {
        episodeIDs.compactMap(publisherChapterWorkflow(episodeID:))
    }

    func receivePublisherChapterWorkflows(revision: UInt64) {
        guard revision >= lastChapterWorkflowRevision else { return }
        lastChapterWorkflowRevision = revision
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
    }

    nonisolated private func publisherChapterWorkflow(
        episodeID: UUID
    ) -> PublisherChapterWorkflowProjection? {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .chapterWorkflows(episodeId: EpisodeId(uuid: episodeID)),
            offset: 0,
            maxItems: 1
        ))
        guard case .chapterWorkflows(let projection) = envelope.projection,
              projection.failure == nil else { return nil }
        return projection.publisher.first
    }

    nonisolated private static func publisherChapterWorkflows(
        facade: Pod0Facade,
        query: WorkflowProjectionQuery
    ) -> [PublisherChapterWorkflowProjection] {
        let subjectRequested = query.kinds.contains(.publisherChapters)
        let globalRequested = query.attentionKinds.contains(.publisherChapters)
            || query.recentKinds.contains(.publisherChapters)
        guard subjectRequested || globalRequested else { return [] }

        var byEpisode: [EpisodeId: PublisherChapterWorkflowProjection] = [:]
        if globalRequested {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .chapterWorkflows(episodeId: nil),
                offset: 0,
                maxItems: 200
            ))
            if case .chapterWorkflows(let projection) = envelope.projection,
               projection.failure == nil {
                for workflow in projection.publisher {
                    byEpisode[workflow.episodeId] = workflow
                }
            }
        }
        if subjectRequested {
            for episodeID in query.subjectIDs.prefix(200) {
                let coreID = EpisodeId(uuid: episodeID)
                let envelope = facade.snapshot(request: ProjectionRequest(
                    scope: .chapterWorkflows(episodeId: coreID),
                    offset: 0,
                    maxItems: 1
                ))
                guard case .chapterWorkflows(let projection) = envelope.projection,
                      projection.failure == nil,
                      let workflow = projection.publisher.first else { continue }
                byEpisode[coreID] = workflow
            }
        }
        return Array(byEpisode.values)
    }
}
