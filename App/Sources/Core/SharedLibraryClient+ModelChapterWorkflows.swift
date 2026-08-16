import Foundation
import Pod0Core

extension SharedLibraryClient {
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
