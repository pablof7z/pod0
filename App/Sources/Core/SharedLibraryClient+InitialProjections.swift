import Foundation
import Pod0Core

struct SharedInitialProjections: @unchecked Sendable {
    let library: SharedLibrarySnapshot
    let notes: SharedNoteSnapshot
    let memories: SharedMemorySnapshot
    let clips: SharedClipSnapshot
    let scheduledAgents: ScheduledAgentProjection
    let notificationSettings: NewEpisodeNotificationSettingsProjection?
    let recallConfiguration: RecallConfiguration?
}

extension SharedLibraryClient {
    func refreshInitialProjections() {
        let facade = facade
        let activeEpisodeIDs = chapterScopes.retainedEpisodeIDs
        let chapterReader = authoritativeChapterReader
        initialProjectionTask?.cancel()
        initialProjectionTask = Task { @MainActor [weak self] in
            let projections = await Task.detached(priority: .utility) {
                Self.loadInitialProjections(
                    facade: facade,
                    activeEpisodeIDs: activeEpisodeIDs,
                    chapterReader: chapterReader
                )
            }.value
            guard !Task.isCancelled, let self, let store else { return }
            applyInitialProjections(projections, to: store)
        }
    }

    func hydrateSynchronouslyForTesting() {
        initialProjectionTask?.cancel()
        initialProjectionTask = nil
        guard let store else { return }
        let projections = Self.loadInitialProjections(
            facade: facade,
            activeEpisodeIDs: chapterScopes.retainedEpisodeIDs,
            chapterReader: authoritativeChapterReader
        )
        applyInitialProjections(projections, to: store, force: true)
    }

    nonisolated private static func loadInitialProjections(
        facade: Pod0Facade,
        activeEpisodeIDs: Set<UUID>,
        chapterReader: SharedChapterReader
    ) -> SharedInitialProjections {
        SharedInitialProjections(
            library: loadAllPages(
                facade: facade,
                activeEpisodeIDs: activeEpisodeIDs,
                chapterReader: chapterReader
            ),
            notes: loadNotePages(facade: facade, scope: .all),
            memories: loadMemoryPages(facade: facade, scope: .all),
            clips: loadClipPages(facade: facade, scope: .active),
            scheduledAgents: loadScheduledAgentPages(facade: facade, fallback: nil),
            notificationSettings: loadNewEpisodeNotificationSettings(facade: facade),
            recallConfiguration: loadRecallConfiguration(facade: facade)
        )
    }

    private func applyInitialProjections(
        _ projections: SharedInitialProjections,
        to store: AppStateStore,
        force: Bool = false
    ) {
        if force || lastLibraryRevision == 0 {
            cachedSnapshot = projections.library
            chapterSnapshots = projections.library.chaptersByEpisodeID
            store.applySharedLibrary(projections.library)
        }
        if force || lastNotesRevision == 0 {
            cachedNotes = projections.notes
            store.applySharedNotes(projections.notes)
        }
        if force || lastMemoriesRevision == 0 {
            cachedMemories = projections.memories
            store.applySharedMemories(projections.memories)
        }
        if force || lastClipsRevision == 0 {
            cachedClips = projections.clips
            store.applySharedClips(projections.clips)
        }
        if force || lastScheduledAgentRevision == 0 {
            cachedScheduledAgent = projections.scheduledAgents
            publishScheduledAgents(to: store)
        }
        if (force || cachedNewEpisodeNotificationSettings == nil),
           let settings = projections.notificationSettings {
            cachedNewEpisodeNotificationSettings = settings
            publishNewEpisodeNotificationSettings(to: store)
        }
        if (force || cachedRecallConfiguration == nil),
           let configuration = projections.recallConfiguration {
            cachedRecallConfiguration = configuration
            publishRecallConfiguration(to: store)
        }
    }
}
