import Foundation
import Pod0Core

extension Persistence {
    func userDataErasureLocations() throws -> UserDataErasureLocations {
        let manager = FileManager.default
        let base = fileURL.deletingLastPathComponent()
        let production = fileURL.standardizedFileURL
            == Self.applicationStateFileURL.standardizedFileURL
        let support = production ? try manager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        ) : base
        let documents = production ? try manager.url(
            for: .documentDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        ) : base
        let core = sharedCoreStoreURL
        let episode = episodeStore.fileURL
        let recall = URL(fileURLWithPath: core.path + ".recall-index.sqlite")
        let outbox = nativeHostObservationOutboxURL
        let leasedOutbox = outbox.deletingPathExtension().appendingPathExtension("leased.json")
        let downloadHost = production
            ? CoreDownloadNativeStore.erasureRootURL(fileManager: manager)
            : base.appendingPathComponent("core-download-host-v1", isDirectory: true)
        let generatedAudio = production
            ? try CoreAgentGeneratedAudioFileStore.erasureRootURL()
            : base.appendingPathComponent("agent-episodes", isDirectory: true)
        let costLedger = production
            ? CostLedger.erasureFileURL
            : base.appendingPathComponent("UsageLedger/ledger.json")
        let productSignals = production
            ? ProductSignalStore.erasureFileURL
            : base.appendingPathComponent("product-signals-v1.json")
        var targets = coreTargets(core: core, episode: episode, recall: recall)
        targets += artifactTargets(
            base: base,
            core: core,
            support: support,
            documents: documents,
            downloadHost: downloadHost,
            generatedAudio: generatedAudio,
            costLedger: costLedger,
            productSignals: productSignals,
            outbox: outbox,
            leasedOutbox: leasedOutbox
        )
        targets += migrationBackupTargets(in: episode.deletingLastPathComponent())
        targets += nativeActionTargets()
        let roots = Set([base, support, documents].map(\.standardizedFileURL.path)).sorted()
        return UserDataErasureLocations(
            recoveryRoot: base.path,
            allowedRoots: roots,
            targets: targets
        )
    }

    private func coreTargets(
        core: URL,
        episode: URL,
        recall: URL
    ) -> [UserDataErasureTargetLocation] {
        [
            target(.coreSqlite, core),
            target(.coreWal, suffixed(core, "-wal")),
            target(.coreShm, suffixed(core, "-shm")),
            target(.episodeSqlite, episode),
            target(.episodeWal, suffixed(episode, "-wal")),
            target(.episodeShm, suffixed(episode, "-shm")),
            target(.recallIndex, recall),
            target(.recallIndexWal, suffixed(recall, "-wal")),
            target(.recallIndexShm, suffixed(recall, "-shm")),
        ]
    }

    private func artifactTargets(
        base: URL,
        core: URL,
        support: URL,
        documents: URL,
        downloadHost: URL,
        generatedAudio: URL,
        costLedger: URL,
        productSignals: URL,
        outbox: URL,
        leasedOutbox: URL
    ) -> [UserDataErasureTargetLocation] {
        [
            target(.downloadedMediaRoot, URL(fileURLWithPath: core.path + ".downloads")),
            target(.stagedMediaRoot, downloadHost),
            covered(.transcriptArtifactRoot, by: .coreSqlite),
            target(.legacyTranscriptRoot, legacyTranscriptRootURL),
            covered(.chapterArtifactRoot, by: .coreSqlite),
            target(.applicationStateProjection, fileURL),
            target(.nativeObservationOutbox, outbox),
            target(.nativeObservationLease, leasedOutbox),
            target(.agentGeneratedAudioRoot, generatedAudio),
            target(.legacyChatHistoryRoot, documents.appendingPathComponent("chat_history")),
            covered(.legacyWorkflowStore, by: .episodeSqlite),
            target(.legacyWorkflowArtifactRoot, legacyChapterArtifactRootURL),
            target(.costLedger, costLedger),
            target(.productSignals, productSignals),
        ]
    }

    private func migrationBackupTargets(in directory: URL) -> [UserDataErasureTargetLocation] {
        let corePrefix = sharedCoreStoreURL.lastPathComponent + ".schema-backup"
        let episodePrefix = episodeStore.fileURL.lastPathComponent + "."
        let knownEpisodeFragments = ["backup", "rollback"]
        let entries = (try? FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: [.isRegularFileKey, .isDirectoryKey],
            options: []
        )) ?? []
        var exact = [
            sharedCoreSchemaBackupURL,
            legacyListeningBackupURL,
            legacyNotesBackupURL,
            legacyClipsBackupURL,
            legacyTranscriptBackupRootURL,
            legacyChapterBackupRootURL,
            legacyTranscriptWorkflowBackupRootURL,
            legacyDownloadWorkflowBackupURL,
            legacyFeedDiscoveryWorkflowBackupRootURL,
        ]
        exact += entries.filter { url in
            let name = url.lastPathComponent
            return name.hasPrefix(corePrefix)
                || (name.hasPrefix(episodePrefix)
                    && knownEpisodeFragments.contains(where: name.localizedCaseInsensitiveContains))
        }
        return Set(exact.map(\.standardizedFileURL.path)).sorted().map {
            UserDataErasureTargetLocation(
                kind: .migrationBackupRoot,
                location: $0,
                coveredBy: nil
            )
        }
    }

    private func nativeActionTargets() -> [UserDataErasureTargetLocation] {
        [
            .init(
                kind: .agentConversationPointer,
                location: "pod0.agent.lastConversationID.v1",
                coveredBy: nil
            ),
            .init(
                kind: .spotlightIndex,
                location: SpotlightIndexer.erasureIdentifier,
                coveredBy: nil
            ),
            .init(
                kind: .nowPlayingProjection,
                location: "group.com.podcastr.app/now-playing-snapshot.v1",
                coveredBy: nil
            ),
        ]
    }

    private func target(
        _ kind: UserDataErasureTargetKind,
        _ url: URL
    ) -> UserDataErasureTargetLocation {
        .init(kind: kind, location: url.standardizedFileURL.path, coveredBy: nil)
    }

    private func covered(
        _ kind: UserDataErasureTargetKind,
        by coveringKind: UserDataErasureTargetKind
    ) -> UserDataErasureTargetLocation {
        .init(kind: kind, location: "", coveredBy: coveringKind)
    }

    private func suffixed(_ url: URL, _ suffix: String) -> URL {
        URL(fileURLWithPath: url.path + suffix)
    }
}
