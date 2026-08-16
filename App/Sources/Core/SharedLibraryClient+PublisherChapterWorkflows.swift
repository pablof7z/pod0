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
    }

    func receiveChapterWorkflows(
        _ projection: ChapterWorkflowsProjection,
        revision: UInt64
    ) {
        guard revision >= lastChapterWorkflowRevision else { return }
        lastChapterWorkflowRevision = revision
        let publisherChanged = projection.publisher != cachedPublisherChapterWorkflows
        cachedPublisherChapterWorkflows = projection.publisher
        workflowClient?.refresh(immediately: true)
        if publisherChanged { WorkflowRuntime.shared.wake() }
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
