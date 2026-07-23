import Foundation
import Pod0Core

extension Persistence {

    /// The App Group suite name, resolved from the target's build setting.
    static var appGroupIdentifier: String {
        Bundle.main.object(forInfoDictionaryKey: "AppGroupIdentifier") as? String
            ?? "group.com.podcastr.app"
    }

    /// Retained only for the one-shot legacy state migration.
    static var appGroupDefaults: UserDefaults {
        UserDefaults(suiteName: appGroupIdentifier) ?? .standard
    }

    /// Production state location inside the shared App Group container.
    static var appGroupStateFileURL: URL {
        let manager = FileManager.default
        let base: URL
        if let groupContainer = manager.containerURL(
            forSecurityApplicationGroupIdentifier: appGroupIdentifier
        ) {
            base = groupContainer.appendingPathComponent(
                "Library/Application Support",
                isDirectory: true
            )
        } else {
            base = (try? manager.url(
                for: .cachesDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            )) ?? URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
        }
        return base.appendingPathComponent("podcastr-state.v1.json", isDirectory: false)
    }

    static func episodeStoreURL(for stateFileURL: URL) -> URL {
        let baseName = stateFileURL.deletingPathExtension().lastPathComponent
        return stateFileURL
            .deletingLastPathComponent()
            .appendingPathComponent("\(baseName).episodes.sqlite", isDirectory: false)
    }

    var sharedCoreStoreURL: URL {
        fileURL.deletingPathExtension().appendingPathExtension("core.sqlite")
    }

    var sharedCoreSchemaBackupURL: URL {
        sharedCoreStoreURL.appendingPathExtension("schema-backup")
    }

    var nativeHostObservationOutboxURL: URL {
        sharedCoreStoreURL.appendingPathExtension("host-observations-v1.json")
    }

    /// Schema migrations retain version-specific rollback evidence so a
    /// later upgrade never mistakes an older valid backup for its own.
    func sharedCoreSchemaBackupURL(targetVersion: UInt32) -> URL {
        sharedCoreStoreURL.appendingPathExtension("schema-backup-v\(targetVersion)")
    }

    var legacyListeningBackupURL: URL {
        episodeStore.fileURL.appendingPathExtension("listening-backup")
    }

    var legacyNotesBackupURL: URL {
        episodeStore.fileURL.appendingPathExtension("notes-backup")
    }

    var legacyClipsBackupURL: URL {
        episodeStore.fileURL.appendingPathExtension("clips-backup")
    }

    var legacyTranscriptRootURL: URL {
        if let support = try? FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        ) {
            return support
                .appendingPathComponent("podcastr", isDirectory: true)
                .appendingPathComponent("transcripts", isDirectory: true)
        }
        return FileManager.default.temporaryDirectory
            .appendingPathComponent("podcastr-transcripts", isDirectory: true)
    }

    var legacyTranscriptBackupRootURL: URL {
        episodeStore.fileURL.appendingPathExtension("transcript-backups")
    }

    var legacyChapterArtifactRootURL: URL {
        if let support = try? FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        ) {
            return support
                .appendingPathComponent("podcastr", isDirectory: true)
                .appendingPathComponent("workflow-artifacts", isDirectory: true)
        }
        return FileManager.default.temporaryDirectory
            .appendingPathComponent("podcastr-workflow-artifacts", isDirectory: true)
    }

    var legacyChapterBackupRootURL: URL {
        episodeStore.fileURL.appendingPathExtension("chapter-backups")
    }

    var legacyModelChapterWorkflowBackupRootURL: URL {
        episodeStore.fileURL.appendingPathExtension("model-chapter-workflow-backups")
    }

    var legacyPublisherChapterWorkflowBackupRootURL: URL {
        episodeStore.fileURL.appendingPathExtension("publisher-chapter-workflow-backups")
    }

    var legacyTranscriptWorkflowBackupRootURL: URL {
        episodeStore.fileURL.appendingPathExtension("transcript-workflow-backups")
    }

    var legacyDownloadWorkflowBackupURL: URL {
        episodeStore.fileURL.appendingPathExtension("download-workflow-backup-v1.json")
    }

    var legacyScheduledAgentWorkflowBackupRootURL: URL {
        episodeStore.fileURL.appendingPathExtension("scheduled-agent-workflow-backups")
    }

    var legacyAgentHistoryBackupRootURL: URL {
        episodeStore.fileURL.appendingPathExtension("agent-history-backups")
    }

    var legacyAgentMemoryBackupRootURL: URL {
        episodeStore.fileURL.appendingPathExtension("agent-memory-backups")
    }

    var legacyRecallIndexURL: URL? {
        guard let support = try? FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        ) else { return nil }
        return support
            .appendingPathComponent("podcastr", isDirectory: true)
            .appendingPathComponent("vectors.sqlite", isDirectory: false)
    }

    func legacyClipsBackupURL(for plan: LegacyClipImportPlan) -> URL {
        episodeStore.fileURL.appendingPathExtension(
            "clips-backup-\(plan.sourceGeneration)-\(plan.sourceHash)"
        )
    }

    func removeSharedCoreArtifacts() {
        let core = sharedCoreStoreURL
        var urls = [
            core,
            URL(fileURLWithPath: core.path + "-wal"),
            URL(fileURLWithPath: core.path + "-shm"),
            nativeHostObservationOutboxURL,
            sharedCoreSchemaBackupURL,
            legacyListeningBackupURL,
            legacyNotesBackupURL,
            legacyClipsBackupURL,
            legacyTranscriptBackupRootURL,
            legacyChapterBackupRootURL,
            legacyModelChapterWorkflowBackupRootURL,
            legacyPublisherChapterWorkflowBackupRootURL,
            legacyTranscriptWorkflowBackupRootURL,
            legacyDownloadWorkflowBackupURL,
            legacyScheduledAgentWorkflowBackupRootURL,
            legacyAgentHistoryBackupRootURL,
            legacyAgentMemoryBackupRootURL
        ]
        urls.append(contentsOf: (1...32).map {
            sharedCoreSchemaBackupURL(targetVersion: UInt32($0))
        })
        let directory = episodeStore.fileURL.deletingLastPathComponent()
        let prefix = episodeStore.fileURL.lastPathComponent + ".clips-backup-"
        if let entries = try? FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: nil
        ) {
            urls.append(contentsOf: entries.filter { $0.lastPathComponent.hasPrefix(prefix) })
        }
        for url in urls {
            if FileManager.default.fileExists(atPath: url.path) {
                try? FileManager.default.removeItem(at: url)
            }
        }
    }
}
