import Foundation
import Pod0Core

extension SharedLibraryClient {
    func requestTranscript(episodeID: UUID, provider: STTProvider?) {
        guard let store, let episode = store.episode(id: episodeID) else { return }
        let configuration = NativeTranscriptWorkflowConfiguration.make(
            episode: episode,
            settings: store.state.settings,
            provider: provider
        )
        dispatchCoreCommand(
            .ensureTranscriptWorkflow(
                episodeId: EpisodeId(uuid: episodeID),
                origin: .user,
                configuration: configuration
            )
        )
        workflowClient?.refresh(immediately: true)
    }

    func receiveTranscriptWorkflows(revision: UInt64) {
        guard revision >= lastTranscriptWorkflowRevision else { return }
        lastTranscriptWorkflowRevision = revision
        workflowClient?.refresh(immediately: true)
    }

    nonisolated static func transcriptWorkflows(
        facade: Pod0Facade,
        query: WorkflowProjectionQuery
    ) -> [TranscriptWorkflowProjection] {
        let kinds: Set<WorkflowProjectionKind> = [.transcriptIngest, .transcriptIndex]
        let direct = !kinds.isDisjoint(with: query.kinds)
        let global = !kinds.isDisjoint(with: query.attentionKinds)
            || !kinds.isDisjoint(with: query.recentKinds)
        guard direct || global else { return [] }
        var byEpisode: [EpisodeId: TranscriptWorkflowProjection] = [:]
        if global {
            var offset: UInt32 = 0
            while byEpisode.count < query.limit {
                let envelope = facade.snapshot(request: ProjectionRequest(
                    scope: .transcriptWorkflows(episodeId: nil),
                    offset: offset,
                    maxItems: 200
                ))
                guard case .transcriptWorkflows(let page) = envelope.projection,
                      page.failure == nil else { break }
                for workflow in page.workflows { byEpisode[workflow.episodeId] = workflow }
                guard page.hasMore, offset <= UInt32.max - 200 else { break }
                offset += 200
            }
        }
        if direct {
            for episodeID in query.subjectIDs.prefix(200) {
                let envelope = facade.snapshot(request: ProjectionRequest(
                    scope: .transcriptWorkflows(episodeId: EpisodeId(uuid: episodeID)),
                    offset: 0,
                    maxItems: 1
                ))
                guard case .transcriptWorkflows(let page) = envelope.projection,
                      page.failure == nil, let workflow = page.workflows.first else { continue }
                byEpisode[workflow.episodeId] = workflow
            }
        }
        return Array(byEpisode.values.prefix(query.limit))
    }
}
