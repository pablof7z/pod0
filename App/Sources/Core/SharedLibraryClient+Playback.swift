import Foundation
import Pod0Core

extension SharedLibraryClient {
    func attachPlayback(_ playback: PlaybackState, store: AppStateStore) {
        playbackState = playback
        playback.attachSharedCore(self)
        if !playbackHostAttached {
            deferredPlaybackHost.attach(CorePlaybackHost(
                engine: playback.engine,
                resolveEpisode: { [weak store] id in store?.episode(id: id) }
            ))
            playbackHostAttached = true
        }
        if let cachedPlayback {
            playback.applySharedPlayback(
                cachedPlayback,
                stateRevision: cachedPlaybackRevision
            ) { [weak store] id in
                store?.episode(id: id)
            }
        }
        dispatchPlayback(.restore)
    }

    func dispatchPlayback(_ command: PlaybackCommand) {
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(uuid: UUID()),
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: .playback(command: command)
        ))
        dispatcher.executePendingRequests(from: facade)
    }

    func receivePlayback(_ projection: PlaybackProjection, revision: UInt64) {
        guard revision >= lastPlaybackRevision else { return }
        lastPlaybackRevision = revision
        cachedPlayback = projection
        cachedPlaybackRevision = revision
        let projectedEpisodeID = projection.current?.episodeId.uuid
        if playbackChapterEpisodeID != projectedEpisodeID {
            if let playbackChapterEpisodeID {
                releaseChapterProjection(episodeID: playbackChapterEpisodeID)
            }
            playbackChapterEpisodeID = projectedEpisodeID
            if let projectedEpisodeID {
                retainChapterProjection(episodeID: projectedEpisodeID)
            }
        }
        if let playbackState {
            playbackState.applySharedPlayback(
                projection,
                stateRevision: revision
            ) { [weak store] id in
                store?.episode(id: id)
            }
        }
        dispatcher.executePendingRequests(from: facade)
    }
}
