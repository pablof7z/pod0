import Foundation
import Pod0Core

struct TranscriptWorkflowOpportunity: @unchecked Sendable {
    let episodeID: UUID
    let configuration: TranscriptWorkflowConfiguration
    let version: String
}

extension SharedLibraryClient {
    /// Announces current platform capability facts; Rust alone decides whether
    /// generation or evidence work is admitted.
    func ensureTranscriptWorkflows(_ opportunities: [TranscriptWorkflowOpportunity]) {
        var announced = false
        for opportunity in opportunities {
            guard announcedTranscriptWorkflowVersions[opportunity.episodeID]
                    != opportunity.version else { continue }
            announcedTranscriptWorkflowVersions[opportunity.episodeID] = opportunity.version
            dispatchCoreCommand(
                .ensureTranscriptWorkflow(
                    episodeId: EpisodeId(uuid: opportunity.episodeID),
                    origin: .automatic,
                    configuration: opportunity.configuration
                )
            )
            announced = true
        }
        guard announced else { return }
        workflowClient?.refresh(immediately: true)
    }

    /// `startPolicies` is keyed by podcast ID and snapshotted on the main actor
    /// by the caller, so this stays free of `store` and can run off-actor.
    nonisolated static func transcriptWorkflowOpportunities(
        episodes: [Episode],
        settings: Settings,
        startPolicies: [UUID: TranscriptStartPolicy]
    ) -> [TranscriptWorkflowOpportunity] {
        episodes.map { episode in
            let configuration = NativeTranscriptWorkflowConfiguration.make(
                episode: episode,
                settings: settings
            )
            return TranscriptWorkflowOpportunity(
                episodeID: episode.id,
                configuration: configuration,
                version: transcriptOpportunityVersion(
                    episode,
                    configuration: configuration,
                    startPolicy: startPolicies[episode.podcastID] ?? .automatic
                )
            )
        }
    }

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

    func performTranscriptAction(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) async -> WorkflowJobActionResult {
        let request = ProjectionRequest(
            scope: .transcriptWorkflows(
                episodeId: EpisodeId(uuid: projection.subjectID)
            ),
            offset: 0,
            maxItems: 1
        )
        let currentEnvelope = await coreSnapshot(request)
        guard projection.authority == .sharedRustTranscripts,
              let expected = projection.coreWorkflowRevision,
              let current = Self.transcriptWorkflow(in: currentEnvelope),
              current.workflowRevision.value == expected else { return .stale }
        let command: ApplicationCommand
        switch action {
        case .cancel where current.allowedActions.canCancel:
            command = .cancelTranscriptWorkflow(
                episodeId: current.episodeId,
                expectedWorkflowRevision: current.workflowRevision
            )
        case .retry where current.allowedActions.canRetry:
            guard let store, let episode = store.episode(id: projection.subjectID) else {
                return .notFound
            }
            command = .retryTranscriptWorkflow(
                episodeId: current.episodeId,
                expectedWorkflowRevision: current.workflowRevision,
                configuration: NativeTranscriptWorkflowConfiguration.make(
                    episode: episode,
                    settings: store.state.settings,
                    provider: current.provider,
                    model: current.model
                )
            )
        default:
            return current.stage == .succeeded ? .alreadyComplete : .notAllowed
        }
        let result = await executeWorkflowAction(command, action: action)
        if case .accepted = result { workflowClient?.refresh(immediately: true) }
        return result
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

private extension SharedLibraryClient {
    nonisolated static func transcriptWorkflow(
        in envelope: ProjectionEnvelope
    ) -> TranscriptWorkflowProjection? {
        guard case .transcriptWorkflows(let projection) = envelope.projection,
              projection.failure == nil else { return nil }
        return projection.workflows.first
    }

    nonisolated static func transcriptOpportunityVersion(
        _ episode: Episode,
        configuration: TranscriptWorkflowConfiguration,
        startPolicy: TranscriptStartPolicy
    ) -> String {
        ArtifactRepository.version(parts: [
            DesiredStatePlanner.audioVersion(episode),
            String(describing: configuration.provider), configuration.model,
            configuration.localAudioUrl ?? "", String(configuration.credentialAvailable),
            String(configuration.autoPublisherEnabled), String(configuration.autoProviderEnabled),
            startPolicy.rawValue,
            episode.publisherTranscriptURL?.absoluteString ?? "",
            episode.publisherTranscriptType?.rawValue ?? "",
        ])
    }
}
