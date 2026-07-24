import Foundation
import Pod0Core

struct SharedClipSnapshot: @unchecked Sendable {
    let collectionRevision: StateRevision
    let clips: [Clip]
    let operations: [OperationProjection]
}

extension SharedLibraryClient {
    func receiveClips(revision: UInt64) {
        guard revision >= lastClipsRevision else { return }
        lastClipsRevision = revision
        let facade = facade
        clipProjectionTask?.cancel()
        clipProjectionTask = Task { @MainActor [weak self] in
            let snapshot = await Task.detached(priority: .utility) {
                Self.loadClipPages(facade: facade, scope: .active)
            }.value
            guard !Task.isCancelled, let self, revision == lastClipsRevision else { return }
            cachedClips = snapshot
            store?.applySharedClips(snapshot)
            resolveWaiters(snapshot.operations)
        }
    }

    func clip(id: UUID) -> Clip? {
        cachedClips?.clips.first { $0.id == id && !$0.deleted }
    }

    func clips(forEpisode episodeID: UUID) -> [Clip] {
        cachedClips?.clips.filter { $0.episodeID == episodeID && !$0.deleted } ?? []
    }

    func allClips() -> [Clip] {
        cachedClips?.clips.filter { !$0.deleted } ?? []
    }

    func createClip(_ clip: Clip) async throws -> Clip {
        guard let start = clip.coreStartMilliseconds,
              let end = clip.coreEndMilliseconds,
              start < end
        else { throw SharedClipMappingError.invalidBounds }
        let result = try await execute(.createClip(
            clipId: ClipId(uuid: clip.id),
            episodeId: EpisodeId(uuid: clip.episodeID),
            podcastId: PodcastId(uuid: clip.subscriptionID),
            startMilliseconds: start,
            endMilliseconds: end,
            caption: clip.caption,
            speakerId: try clip.coreSpeakerID(),
            frozenTranscriptText: clip.transcriptText,
            source: clip.source.coreValue
        ))
        let snapshot = await refreshClipSnapshot()
        guard case .clipCreated(
            let clipID,
            let clipRevision,
            let collectionRevision
        ) = result,
              let id = clipID.uuid,
              snapshot.collectionRevision == collectionRevision,
              let projected = snapshot.clips.first(where: { $0.id == id }),
              projected.revision == clipRevision.value
        else { throw SharedLibraryError.unavailable }
        return projected
    }

    func updateClip(_ clip: Clip) async throws {
        guard let start = clip.coreStartMilliseconds,
              let end = clip.coreEndMilliseconds,
              start < end
        else { throw SharedClipMappingError.invalidBounds }
        let result = try await execute(.updateClip(
            clipId: ClipId(uuid: clip.id),
            expectedClipRevision: ClipRevision(value: clip.revision),
            startMilliseconds: start,
            endMilliseconds: end,
            caption: clip.caption,
            speakerId: try clip.coreSpeakerID(preservingLegacyLabel: true),
            frozenTranscriptText: clip.transcriptText
        ))
        _ = await refreshClipSnapshot()
        try await verifyClipUpdate(result, id: clip.id, deleted: false)
    }

    func setClipDeleted(_ clip: Clip, deleted: Bool) async throws {
        let result = try await execute(.setClipDeleted(
            clipId: ClipId(uuid: clip.id),
            expectedClipRevision: ClipRevision(value: clip.revision),
            deleted: deleted
        ))
        _ = await refreshClipSnapshot()
        try await verifyClipUpdate(result, id: clip.id, deleted: deleted)
    }

    func clearClips() async throws {
        let revision = await clipCollectionRevision()
        let result = try await execute(.clearClips(expectedCollectionRevision: revision))
        _ = await refreshClipSnapshot()
        guard case .clipsCleared(let collectionRevision) = result,
              cachedClips?.collectionRevision == collectionRevision
        else { throw SharedLibraryError.unavailable }
    }

    nonisolated static func loadClipPages(
        facade: Pod0Facade,
        scope: ClipProjectionScope
    ) -> SharedClipSnapshot {
        var offset: UInt32 = 0
        var collectionRevision = StateRevision(value: 1)
        var clips: [Clip] = []
        var operations: [OperationProjection] = []
        while true {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .clips(scope: scope),
                offset: offset,
                maxItems: 200
            ))
            guard case .clips(let page) = envelope.projection else { break }
            collectionRevision = page.collectionRevision
            clips.append(contentsOf: page.clips.compactMap(\.swiftValue))
            if operations.isEmpty { operations = page.operations }
            guard page.hasMore, offset <= UInt32.max - 200 else { break }
            offset += 200
        }
        return SharedClipSnapshot(
            collectionRevision: collectionRevision,
            clips: clips,
            operations: operations
        )
    }

    private func clipCollectionRevision() async -> StateRevision {
        if let revision = cachedClips?.collectionRevision { return revision }
        let facade = facade
        let snapshot = await Task.detached(priority: .utility) {
            Self.loadClipPages(facade: facade, scope: .active)
        }.value
        cachedClips = snapshot
        store?.applySharedClips(snapshot)
        return snapshot.collectionRevision
    }

    private func refreshClipSnapshot() async -> SharedClipSnapshot {
        let facade = facade
        let snapshot = await Task.detached(priority: .utility) {
            Self.loadClipPages(facade: facade, scope: .active)
        }.value
        cachedClips = snapshot
        store?.applySharedClips(snapshot)
        return snapshot
    }

    private func verifyClipUpdate(
        _ result: OperationResult?,
        id: UUID,
        deleted: Bool
    ) async throws {
        let facade = facade
        guard case .clipUpdated(
            let clipID,
            let clipRevision,
            let collectionRevision
        ) = result,
              clipID.uuid == id,
              cachedClips?.collectionRevision == collectionRevision
        else { throw SharedLibraryError.unavailable }
        let projected = await Task.detached(priority: .utility) {
            Self.loadClipPages(facade: facade, scope: .clip(clipId: clipID)).clips.first
        }.value
        guard let projected,
              projected.revision == clipRevision.value,
              projected.deleted == deleted
        else { throw SharedLibraryError.unavailable }
    }
}
