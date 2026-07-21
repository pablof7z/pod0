import Foundation
import Pod0Core

extension SharedLibraryClient {
    func performModelChapterAction(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) -> WorkflowJobActionResult {
        guard projection.authority == .sharedRustModelChapters,
              let expectedRevision = projection.coreWorkflowRevision,
              let current = modelChapterWorkflow(episodeID: projection.subjectID)
        else { return .notFound }
        guard current.workflowRevision.value == expectedRevision else { return .stale }

        let command: ApplicationCommand
        switch action {
        case .retry where current.allowedActions.canRetry:
            command = .retryModelChapters(
                episodeId: current.episodeId,
                configuredModel: current.configuredModel,
                expectedWorkflowRevision: current.workflowRevision
            )
        case .cancel where current.allowedActions.canCancel:
            command = .cancelModelChapters(
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
        let updated = modelChapterWorkflow(episodeID: projection.subjectID)
        guard let updated, updated.workflowRevision.value > expectedRevision else { return .stale }
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
        return .accepted(action)
    }

    nonisolated func modelChapterWorkflowSnapshots(
        episodeIDs: some Sequence<UUID>
    ) -> [ModelChapterWorkflowProjection] {
        episodeIDs.compactMap(modelChapterWorkflow(episodeID:))
    }

    nonisolated fileprivate func modelChapterWorkflow(
        episodeID: UUID
    ) -> ModelChapterWorkflowProjection? {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .chapterWorkflows(episodeId: EpisodeId(uuid: episodeID)),
            offset: 0,
            maxItems: 2
        ))
        guard case .chapterWorkflows(let projection) = envelope.projection,
              projection.failure == nil else { return nil }
        return projection.model.first
    }

    nonisolated static func modelChapterWorkflows(
        facade: Pod0Facade,
        query: WorkflowProjectionQuery
    ) -> [ModelChapterWorkflowProjection] {
        let subjectRequested = query.kinds.contains(.chapterArtifacts)
        let globalRequested = query.attentionKinds.contains(.chapterArtifacts)
            || query.recentKinds.contains(.chapterArtifacts)
        guard subjectRequested || globalRequested else { return [] }

        var byEpisode: [EpisodeId: ModelChapterWorkflowProjection] = [:]
        if globalRequested {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .chapterWorkflows(episodeId: nil),
                offset: 0,
                maxItems: 200
            ))
            if case .chapterWorkflows(let projection) = envelope.projection,
               projection.failure == nil {
                for workflow in projection.model { byEpisode[workflow.episodeId] = workflow }
            }
        }
        if subjectRequested {
            for episodeID in query.subjectIDs.prefix(200) {
                let coreID = EpisodeId(uuid: episodeID)
                let envelope = facade.snapshot(request: ProjectionRequest(
                    scope: .chapterWorkflows(episodeId: coreID),
                    offset: 0,
                    maxItems: 2
                ))
                guard case .chapterWorkflows(let projection) = envelope.projection,
                      projection.failure == nil,
                      let workflow = projection.model.first else { continue }
                byEpisode[coreID] = workflow
            }
        }
        return Array(byEpisode.values)
    }
}
