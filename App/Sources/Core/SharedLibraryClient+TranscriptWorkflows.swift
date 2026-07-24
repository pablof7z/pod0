import Foundation
import Pod0Core

extension SharedLibraryClient {
    /// Announces current platform capability facts; Rust alone decides whether
    /// generation or evidence work is admitted.
    func ensureTranscriptWorkflows(episodes: some Sequence<Episode>, settings: Settings) {
        var announced = false
        for episode in episodes {
            let configuration = NativeTranscriptWorkflowConfiguration.make(
                episode: episode,
                settings: settings
            )
            let startPolicy = store?.subscription(
                podcastID: episode.podcastID
            )?.transcriptStartPolicy ?? .automatic
            let version = transcriptOpportunityVersion(
                episode,
                configuration: configuration,
                startPolicy: startPolicy
            )
            guard announcedTranscriptWorkflowVersions[episode.id] != version else { continue }
            announcedTranscriptWorkflowVersions[episode.id] = version
            dispatchTranscript(.ensureTranscriptWorkflow(
                episodeId: EpisodeId(uuid: episode.id),
                origin: .automatic,
                configuration: configuration
            ))
            announced = true
        }
        guard announced else { return }
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
    }

    func requestTranscript(episodeID: UUID, provider: STTProvider?) {
        guard let store, let episode = store.episode(id: episodeID) else { return }
        let configuration = NativeTranscriptWorkflowConfiguration.make(
            episode: episode,
            settings: store.state.settings,
            provider: provider
        )
        dispatchTranscript(.ensureTranscriptWorkflow(
            episodeId: EpisodeId(uuid: episodeID),
            origin: .user,
            configuration: configuration
        ))
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
    }

    func performTranscriptAction(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) -> WorkflowJobActionResult {
        guard projection.authority == .sharedRustTranscripts,
              let expected = projection.coreWorkflowRevision,
              let current = transcriptWorkflow(episodeID: projection.subjectID),
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
        dispatchTranscript(command)
        guard transcriptWorkflow(episodeID: projection.subjectID)?.workflowRevision.value != expected
        else { return .stale }
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
        return .accepted(action)
    }

    func receiveTranscriptWorkflows(revision: UInt64) {
        guard revision >= lastTranscriptWorkflowRevision else { return }
        lastTranscriptWorkflowRevision = revision
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
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
    func transcriptWorkflow(episodeID: UUID) -> TranscriptWorkflowProjection? {
        Self.transcriptWorkflows(
            facade: facade,
            query: WorkflowProjectionQuery(
                subjectIDs: [episodeID],
                kinds: [.transcriptIngest, .transcriptIndex],
                attentionKinds: [],
                recentKinds: [],
                limit: 1
            )
        ).first
    }

    func transcriptOpportunityVersion(
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

    func dispatchTranscript(_ command: ApplicationCommand) {
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(uuid: UUID()),
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: command
        ))
    }
}
