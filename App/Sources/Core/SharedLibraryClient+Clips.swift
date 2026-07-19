import Foundation
import Pod0Core

struct SharedClipSnapshot {
    let collectionRevision: StateRevision
    let clips: [Clip]
    let operations: [OperationProjection]
}

extension SharedLibraryClient {
    func receiveClips(revision: UInt64) {
        guard revision >= lastClipsRevision else { return }
        lastClipsRevision = revision
        let snapshot = loadClipPages(scope: .active)
        cachedClips = snapshot
        store?.applySharedClips(snapshot)
        resolveWaiters(snapshot.operations)
    }

    func clip(id: UUID) -> Clip? {
        loadClipPages(scope: .clip(clipId: ClipId(uuid: id)))
            .clips
            .first { !$0.deleted }
    }

    func clips(forEpisode episodeID: UUID) -> [Clip] {
        loadClipPages(scope: .episode(episodeId: EpisodeId(uuid: episodeID))).clips
    }

    func allClips() -> [Clip] {
        loadClipPages(scope: .active).clips
    }

    func createClip(_ clip: Clip) throws -> Clip {
        guard let start = clip.coreStartMilliseconds,
              let end = clip.coreEndMilliseconds,
              start < end
        else { throw SharedClipMappingError.invalidBounds }
        let result = try executeClipCommand(.createClip(
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
        guard case .clipCreated(
            let clipID,
            let clipRevision,
            let collectionRevision
        ) = result,
              let id = clipID.uuid,
              let snapshot = cachedClips,
              snapshot.collectionRevision == collectionRevision,
              let projected = snapshot.clips.first(where: { $0.id == id }),
              projected.revision == clipRevision.value
        else { throw SharedLibraryError.unavailable }
        return projected
    }

    func updateClip(_ clip: Clip) throws {
        guard let start = clip.coreStartMilliseconds,
              let end = clip.coreEndMilliseconds,
              start < end
        else { throw SharedClipMappingError.invalidBounds }
        let result = try executeClipCommand(.updateClip(
            clipId: ClipId(uuid: clip.id),
            expectedClipRevision: ClipRevision(value: clip.revision),
            startMilliseconds: start,
            endMilliseconds: end,
            caption: clip.caption,
            speakerId: try clip.coreSpeakerID(preservingLegacyLabel: true),
            frozenTranscriptText: clip.transcriptText
        ))
        try verifyClipUpdate(result, id: clip.id, deleted: false)
    }

    func setClipDeleted(_ clip: Clip, deleted: Bool) throws {
        let result = try executeClipCommand(.setClipDeleted(
            clipId: ClipId(uuid: clip.id),
            expectedClipRevision: ClipRevision(value: clip.revision),
            deleted: deleted
        ))
        try verifyClipUpdate(result, id: clip.id, deleted: deleted)
    }

    func clearClips() throws {
        let revision = cachedClips?.collectionRevision
            ?? loadClipPages(scope: .active).collectionRevision
        let result = try executeClipCommand(.clearClips(expectedCollectionRevision: revision))
        guard case .clipsCleared(let collectionRevision) = result,
              cachedClips?.collectionRevision == collectionRevision
        else { throw SharedLibraryError.unavailable }
    }

    func loadClipPages(scope: ClipProjectionScope) -> SharedClipSnapshot {
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

    private func executeClipCommand(_ command: ApplicationCommand) throws -> OperationResult? {
        let commandID = CommandId(uuid: UUID())
        facade.dispatch(command: CommandEnvelope(
            commandId: commandID,
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: command
        ))
        let snapshot = loadClipPages(scope: .active)
        cachedClips = snapshot
        store?.applySharedClips(snapshot)
        guard let operation = snapshot.operations.first(where: { $0.commandId == commandID })
        else { throw SharedLibraryError.unavailable }
        switch operation.stage {
        case .succeeded:
            return operation.result
        case .failed, .cancelled, .unsupported:
            throw SharedLibraryError(operation.failure?.code)
        case .accepted, .running, .blocked:
            throw SharedLibraryError.unavailable
        }
    }

    private func verifyClipUpdate(
        _ result: OperationResult?,
        id: UUID,
        deleted: Bool
    ) throws {
        guard case .clipUpdated(
            let clipID,
            let clipRevision,
            let collectionRevision
        ) = result,
              clipID.uuid == id,
              cachedClips?.collectionRevision == collectionRevision,
              let projected = loadClipPages(scope: .clip(clipId: clipID)).clips.first,
              projected.revision == clipRevision.value,
              projected.deleted == deleted
        else { throw SharedLibraryError.unavailable }
    }
}
