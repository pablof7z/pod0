import Foundation
import Pod0Core

extension SharedLibraryClient {
    func requestDownload(
        episodeID: UUID,
        origin: Pod0Core.DownloadIntentOrigin = .user
    ) {
        dispatchDownload(.requestEpisodeDownload(
            episodeId: EpisodeId(uuid: episodeID),
            origin: origin
        ))
    }

    func reportAutomaticDownloadCandidates(podcastID: UUID, episodeIDs: [UUID]) {
        dispatchDownload(.reportAutomaticDownloadCandidates(
            podcastId: PodcastId(uuid: podcastID),
            episodeIds: episodeIDs.map(EpisodeId.init(uuid:))
        ))
    }

    func cancelDownload(episodeID: UUID) {
        guard let revision = cachedDownloadWorkflows[episodeID]?.workflowRevision else { return }
        dispatchDownload(.cancelEpisodeDownload(
            episodeId: EpisodeId(uuid: episodeID),
            expectedWorkflowRevision: revision
        ))
    }

    func removeDownload(episodeID: UUID) {
        guard let revision = cachedDownloadWorkflows[episodeID]?.workflowRevision else { return }
        dispatchDownload(.removeEpisodeDownload(
            episodeId: EpisodeId(uuid: episodeID),
            expectedWorkflowRevision: revision
        ))
    }

    func retryDownload(episodeID: UUID) {
        requestDownload(
            episodeID: episodeID,
            origin: cachedDownloadWorkflows[episodeID]?.origin ?? .user
        )
    }

    func downloadWorkflow(episodeID: UUID) -> DownloadWorkflowProjection? {
        cachedDownloadWorkflows[episodeID]
    }

    func downloadState(for status: DownloadArtifactStatus) -> DownloadState {
        guard case .available(let reference, let byteCount) = status,
              let url = downloadNativeStore.artifactURL(
                coreStoreURL: coreStoreURL,
                artifactKey: reference.opaqueKey,
                expectedByteCount: byteCount
              ),
              byteCount <= UInt64(Int64.max)
        else { return .notDownloaded }
        return .downloaded(localFileURL: url, byteCount: Int64(byteCount))
    }

    func downloadProgress(episodeID: UUID) -> Double? {
        CoreDownloadHost.shared.progress[EpisodeId(uuid: episodeID)]
    }

    func downloadExpectedBytes(episodeID: UUID) -> Int64? {
        CoreDownloadHost.shared.expectedBytes[EpisodeId(uuid: episodeID)].flatMap {
            $0 <= UInt64(Int64.max) ? Int64($0) : nil
        }
    }

    func receiveDownloads(revision: UInt64) {
        guard revision >= lastDownloadsRevision else { return }
        lastDownloadsRevision = revision
        var offset: UInt32 = 0
        var workflows: [UUID: DownloadWorkflowProjection] = [:]
        while true {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .downloads(episodeId: nil),
                offset: offset,
                maxItems: 200
            ))
            guard case .downloads(let page) = envelope.projection else { break }
            for workflow in page.workflows {
                if let id = workflow.episodeId.uuid { workflows[id] = workflow }
            }
            guard page.hasMore, offset <= UInt32.max - 200 else { break }
            offset += 200
        }
        cachedDownloadWorkflows = workflows
        store?.applySharedLibrary(loadAllPages())
        workflowClient?.refresh(immediately: true)
        dispatcher.executePendingRequests(from: facade)
    }

    func performDownloadAction(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) -> WorkflowJobActionResult {
        guard projection.authority == .sharedRustDownloads,
              projection.allowedActions.contains(action),
              cachedDownloadWorkflows[projection.subjectID]?.workflowRevision.value
                == projection.coreWorkflowRevision
        else { return .stale }
        switch action {
        case .retry:
            retryDownload(episodeID: projection.subjectID)
        case .cancel:
            cancelDownload(episodeID: projection.subjectID)
        }
        return .accepted(action)
    }

    nonisolated static func downloadWorkflows(
        facade: Pod0Facade,
        query: WorkflowProjectionQuery
    ) -> [DownloadWorkflowProjection] {
        let direct: Set<UUID> = query.kinds.contains(.download)
            ? Set(query.subjectIDs)
            : []
        let wantsAttention = query.attentionKinds.contains(.download)
        let wantsRecent = query.recentKinds.contains(.download)
        guard !direct.isEmpty || wantsAttention || wantsRecent else { return [] }
        var offset: UInt32 = 0
        var result: [DownloadWorkflowProjection] = []
        while result.count < query.limit {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .downloads(episodeId: nil),
                offset: offset,
                maxItems: 200
            ))
            guard case .downloads(let page) = envelope.projection else { break }
            result.append(contentsOf: page.workflows.filter { workflow in
                guard let episodeID = workflow.episodeId.uuid else { return false }
                if direct.contains(episodeID) { return true }
                if wantsRecent { return true }
                return wantsAttention && workflow.stage.needsAttention
            })
            guard page.hasMore, offset <= UInt32.max - 200 else { break }
            offset += 200
        }
        return Array(result.prefix(query.limit))
    }

    func observeDownloadEnvironment(
        network: DownloadNetworkStatus,
        availableCapacityBytes: Int64?
    ) {
        let mappedNetwork: DownloadNetworkState = switch network {
        case .unknown: .unknown
        case .unavailable: .unavailable
        case .wifi: .wifi
        case .other: .other
        }
        let capacity = availableCapacityBytes.flatMap { value in
            value >= 0 ? UInt64(value) : nil
        }
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(uuid: UUID()),
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: .observeDownloadEnvironment(
                observation: DownloadEnvironmentObservation(
                    network: mappedNetwork,
                    availableCapacityBytes: capacity
                )
            )
        ))
        dispatcher.executePendingRequests(from: facade)
    }

    private func dispatchDownload(_ command: ApplicationCommand) {
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(uuid: UUID()),
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: command
        ))
        dispatcher.executePendingRequests(from: facade)
    }
}

private extension DownloadWorkflowStage {
    var needsAttention: Bool {
        switch self {
        case .waitingForEnvironment, .requested, .hostAccepted, .transferring,
             .staged, .retryScheduled, .removing, .failed:
            true
        case .cancelled, .succeeded, .unsupported:
            false
        }
    }
}
