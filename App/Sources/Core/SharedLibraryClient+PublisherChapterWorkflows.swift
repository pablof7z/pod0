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
        workflowClient.attachModelChapterCore { query in
            await Task.detached(priority: .userInitiated) {
                Self.modelChapterWorkflows(facade: facade, query: query)
            }.value
        }
        workflowClient.attachDownloadCore { query in
            await Task.detached(priority: .userInitiated) {
                Self.downloadWorkflows(facade: facade, query: query)
            }.value
        }
        workflowClient.attachTranscriptCore { query in
            await Task.detached(priority: .userInitiated) {
                Self.transcriptWorkflows(facade: facade, query: query)
            }.value
        }
        workflowClient.attachScheduledAgentCore(cachedScheduledAgent?.workflows ?? [])
    }

    /// Announces a native execution opportunity only. Rust derives whether a
    /// publisher workflow is needed from its authoritative episode metadata.
    func ensurePublisherChapters(episodeIDs: some Sequence<UUID>) {
        guard let cachedSnapshot else { return }
        let eligibleEpisodeIDs = PublisherChapterOpportunityPlanner.requestedEpisodeIDs(
            requested: episodeIDs,
            current: cachedSnapshot,
            excluding: announcedPublisherChapterEpisodeIDs
        )
        announcedPublisherChapterEpisodeIDs.formUnion(eligibleEpisodeIDs)
        announcePublisherChapterOpportunity(episodeIDs: eligibleEpisodeIDs)
    }

    func announcePublisherSourceChanges(
        previous: SharedLibrarySnapshot?,
        current: SharedLibrarySnapshot
    ) {
        let changedEpisodeIDs = PublisherChapterOpportunityPlanner.changedEpisodeIDs(
            previous: previous,
            current: current
        )
        announcedPublisherChapterEpisodeIDs.formUnion(changedEpisodeIDs)
        announcePublisherChapterOpportunity(episodeIDs: changedEpisodeIDs)
    }

    private func announcePublisherChapterOpportunity(
        episodeIDs: some Sequence<UUID>
    ) {
        let episodeIDs = Array(episodeIDs)
        guard !episodeIDs.isEmpty else { return }
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

    func receiveChapterWorkflows(
        _ projection: ChapterWorkflowsProjection,
        revision: UInt64
    ) {
        guard revision >= lastChapterWorkflowRevision else { return }
        lastChapterWorkflowRevision = revision
        let publisherChanged = projection.publisher != cachedPublisherChapterWorkflows
        cachedPublisherChapterWorkflows = projection.publisher
        if publisherChanged { announcedModelChapterVersions.removeAll() }
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
        if publisherChanged { WorkflowRuntime.shared.wake() }
    }

    nonisolated private func publisherChapterWorkflow(
        episodeID: UUID
    ) -> PublisherChapterWorkflowProjection? {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .chapterWorkflows(episodeId: EpisodeId(uuid: episodeID)),
            offset: 0,
            maxItems: 2
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

/// Stateless coalescing over the latest typed Rust projection. This decides
/// only whether native should announce an execution opportunity; Rust still
/// owns workflow admission, replacement, retirement, retry, and persistence.
enum PublisherChapterOpportunityPlanner {
    static func requestedEpisodeIDs(
        requested: some Sequence<UUID>,
        current: SharedLibrarySnapshot,
        excluding announced: Set<UUID>
    ) -> [UUID] {
        let requested = Set(requested).subtracting(announced)
        guard !requested.isEmpty else { return [] }
        return current.episodes.compactMap { record in
            guard let id = record.episodeId.uuid,
                  requested.contains(id),
                  normalizedSource(record.feedMetadata.chaptersUrl) != nil
            else { return nil }
            return id
        }
    }

    static func changedEpisodeIDs(
        previous: SharedLibrarySnapshot?,
        current: SharedLibrarySnapshot
    ) -> [UUID] {
        let previousSources = previous.map(sourceMap)
        return current.episodes.compactMap { record in
            guard let id = record.episodeId.uuid else { return nil }
            let currentSource = normalizedSource(record.feedMetadata.chaptersUrl)
            if previousSources == nil { return currentSource == nil ? nil : id }
            return previousSources?[id] == currentSource ? nil : id
        }
    }

    private static func sourceMap(_ snapshot: SharedLibrarySnapshot) -> [UUID: String] {
        Dictionary(uniqueKeysWithValues: snapshot.episodes.compactMap { record in
            guard let id = record.episodeId.uuid,
                  let source = normalizedSource(record.feedMetadata.chaptersUrl)
            else { return nil }
            return (id, source)
        })
    }

    private static func normalizedSource(_ source: String?) -> String? {
        guard let source else { return nil }
        let trimmed = source.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}
