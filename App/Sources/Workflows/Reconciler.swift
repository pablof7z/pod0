import Foundation

struct ReconciliationReport: Equatable, Sendable {
    var ensured = 0
    var adoptedArtifacts = 0
    var invalidatedArtifacts = 0
    var obsoletedJobs = 0
}

@MainActor
struct Reconciler {
    let appStore: AppStateStore
    let jobStore: JobStore
    let artifacts: ArtifactRepository
    var now: () -> Date = Date.init

    @discardableResult
    func reconcile() throws -> ReconciliationReport {
        var report = ReconciliationReport()
        report.adoptedArtifacts += try verifyAndAdoptFilesystemArtifacts()
        let desired = DesiredStatePlanner().plan(.init(
            episodes: appStore.state.episodes,
            settings: appStore.state.settings,
            artifacts: try artifacts.all(),
            transcripts: transcriptSnapshots(),
            transcriptDesiredEpisodeIDs: try transcriptDesiredIDs(),
            scheduledTasks: appStore.scheduledTasks,
            now: now()
        ))
        report.ensured = try jobStore.ensureJobs(desired)
        report.ensured += try jobStore.rearmSucceededRepairs(desired, now: now())
        let jobsByKey = Dictionary(
            uniqueKeysWithValues: try jobStore.allJobs().map { ($0.idempotencyKey, $0) }
        )
        for desiredJob in desired {
            guard let existing = jobsByKey[desiredJob.idempotencyKey],
                  existing.state == .blocked,
                  prerequisitesAreAvailable(for: existing) else { continue }
            try jobStore.unblock(idempotencyKey: desiredJob.idempotencyKey, now: now())
        }

        report.obsoletedJobs += try obsoleteDisabledAutomaticDownloads()
        report.obsoletedJobs += try obsoleteDisabledNotifications()
        report.obsoletedJobs += try obsoleteStaleDownloadInputs()

        let desiredKeys = Set(desired.map(\.idempotencyKey))
        let before = try jobStore.allJobs().filter { $0.state.isActive && $0.occurrenceID == nil }.count
        try jobStore.obsoleteActiveJobs(notIn: desiredKeys)
        let after = try jobStore.allJobs().filter { $0.state.isActive && $0.occurrenceID == nil }.count
        report.obsoletedJobs += max(0, before - after)
        return report
    }

    private func obsoleteDisabledAutomaticDownloads() throws -> Int {
        var count = 0
        let jobs = try jobStore.allJobs()
        for job in jobs where job.state.isActive {
            let isAutomatic: Bool
            switch job.kind {
            case .autoDownload:
                isAutomatic = true
            case .download:
                isAutomatic = (try? Self.decoder.decode(
                    DownloadJobPayload.self,
                    from: job.payload ?? Data()
                ).origin) == .autoDownload
            default:
                isAutomatic = false
            }
            guard isAutomatic,
                  let episode = appStore.episode(id: job.subjectID) else { continue }
            if case .off = appStore.effectiveAutoDownload(forPodcast: episode.podcastID).mode {
                try jobStore.updateActiveTerminal(id: job.id, state: .obsolete)
                let hasOtherIntent = jobs.contains {
                    $0.id != job.id && $0.kind == .download && $0.state.isActive
                        && $0.subjectID == job.subjectID
                        && $0.inputVersion == job.inputVersion
                        && (try? Self.decoder.decode(
                            DownloadJobPayload.self,
                            from: $0.payload ?? Data()
                        ).origin) != .autoDownload
                }
                if !hasOtherIntent {
                    EpisodeDownloadService.shared.cancelAdmittedTransfer(
                        jobID: job.id,
                        episodeID: job.subjectID
                    )
                }
                count += 1
            }
        }
        return count
    }

    private func obsoleteStaleDownloadInputs() throws -> Int {
        var count = 0
        for job in try jobStore.allJobs()
            where job.kind == .download && job.state.isActive {
            guard let episode = appStore.episode(id: job.subjectID),
                  DesiredStatePlanner.audioVersion(episode) != job.inputVersion else {
                continue
            }
            try jobStore.updateActiveTerminal(id: job.id, state: .obsolete)
            EpisodeDownloadService.shared.cancelAdmittedTransfer(
                jobID: job.id,
                episodeID: job.subjectID
            )
            count += 1
        }
        return count
    }

    /// Notification occurrences are authoritative history, but an undelivered
    /// occurrence must still honor the current global choice. Once disabled,
    /// terminally obsolete pending delivery so a later re-enable cannot surface
    /// a stale alert.
    private func obsoleteDisabledNotifications() throws -> Int {
        guard !appStore.state.settings.notifyOnNewEpisodes else { return 0 }
        var count = 0
        for job in try jobStore.allJobs()
            where job.kind == .newEpisodeNotification && job.state.isActive {
            try jobStore.updateActiveTerminal(id: job.id, state: .obsolete)
            count += 1
        }
        return count
    }

    /// Verifies that the workflow's derived receipt still names the exact
    /// generation selected by the authoritative Rust evidence store.
    func verifySharedEvidenceSelections() async throws {
        for artifact in try artifacts.all()
            where artifact.kind == .semanticIndex {
            guard artifact.integrity == .available,
                  let encoded = artifact.origin,
                  let data = Data(base64Encoded: encoded),
                  let receipt = try? Self.decoder.decode(
                    SharedEvidenceReceipt.self, from: data
                  ),
                  receipt.generationID == artifact.outputVersion,
                  receipt.episodeID == artifact.subjectID,
                  appStore.sharedLibrary?.verifyEvidenceReceipt(receipt) == true else {
                try artifacts.markIntegrity(
                    kind: artifact.kind,
                    subjectID: artifact.subjectID,
                    integrity: .corrupt
                )
                continue
            }
        }
    }

    private func transcriptDesiredIDs() throws -> Set<UUID> {
        let episodes = appStore.state.episodes
        var ids = Set(episodes.compactMap { episode -> UUID? in
            if case .downloaded = episode.downloadState { return episode.id }
            return episode.requestedTranscriptProvider == nil ? nil : episode.id
        })
        ids.formUnion(TranscriptIngestService.autoIngestCandidates(
            among: episodes,
            settings: appStore.state.settings,
            elevenLabsKey: TranscriptIngestService.shared.resolvedElevenLabsKey(),
            openRouterKey: TranscriptIngestService.shared.resolvedOpenRouterKey(),
            assemblyAIKey: TranscriptIngestService.shared.resolvedAssemblyAIKey()
        ).map(\.id))
        ids.formUnion(transcriptSnapshots().map(\.episodeID))
        return ids
    }

    private func transcriptSnapshots() -> [TranscriptWorkflowSnapshot] {
        appStore.sharedLibrary?.transcriptWorkflowSnapshots(
            episodeIDs: appStore.state.episodes.map(\.id)
        ) ?? []
    }

    private func prerequisitesAreAvailable(for job: WorkJob) -> Bool {
        switch job.kind {
        case .transcriptIngest:
            guard let payload = try? Self.decoder.decode(
                TranscriptJobPayload.self, from: job.payload ?? Data()
            ) else { return false }
            switch payload.provider {
            case .elevenLabsScribe:
                return TranscriptIngestService.shared.resolvedElevenLabsKey() != nil
            case .openRouterWhisper:
                return TranscriptIngestService.shared.resolvedOpenRouterKey() != nil
            case .assemblyAI:
                return TranscriptIngestService.shared.resolvedAssemblyAIKey() != nil
            case .appleNative:
                guard let episode = appStore.episode(id: job.subjectID) else { return false }
                return EpisodeDownloadStore.shared.exists(for: episode)
            }
        case .metadataIndex, .transcriptIndex:
            let model = LLMModelReference(storedID: appStore.state.settings.embeddingsModel)
            return LLMProviderCredentialResolver.hasAPIKey(for: model.provider)
        case .chapterArtifacts:
            let model = LLMModelReference(storedID: appStore.state.settings.chapterCompilationModel)
            return LLMProviderCredentialResolver.hasAPIKey(for: model.provider)
        case .publisherChapters:
            return true
        case .scheduledAgentRun:
            guard let payload = try? Self.decoder.decode(
                ScheduledRunPayload.self, from: job.payload ?? Data()
            ) else { return false }
            return LLMProviderCredentialResolver.hasAPIKey(
                for: LLMModelReference(storedID: payload.modelID).provider
            )
        case .feedDiscovery, .download, .autoDownload, .newEpisodeNotification:
            return true
        }
    }

    static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
