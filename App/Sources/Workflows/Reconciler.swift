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
            transcriptDesiredEpisodeIDs: transcriptDesiredIDs(),
            scheduledTasks: appStore.scheduledTasks,
            now: now()
        ))
        report.ensured = try jobStore.ensureJobs(desired)
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

    /// Repairs the small external-index selection gap after the authoritative
    /// artifact/job transaction. Generation rows are immutable and remain
    /// invisible until this selected pointer is restored.
    func repairVectorSelections() async throws {
        for artifact in try artifacts.all()
            where artifact.kind == .semanticIndex || artifact.kind == .metadataIndex {
            guard artifact.integrity == .available,
                  let encoded = artifact.origin,
                  let data = Data(base64Encoded: encoded),
                  let receipt = try? Self.decoder.decode(
                    VectorArtifactReceipt.self, from: data
                  ),
                  receipt.generation == artifact.outputVersion,
                  try await RAGService.shared.index.verifyArtifact(
                    episodeID: artifact.subjectID, receipt: receipt
                  ) else {
                try artifacts.markIntegrity(
                    kind: artifact.kind,
                    subjectID: artifact.subjectID,
                    integrity: .corrupt
                )
                continue
            }
            let selected = try await RAGService.shared.index.selectedReceipt(
                episodeID: artifact.subjectID,
                artifactKind: receipt.artifactKind
            )
            if selected != receipt {
                try await RAGService.shared.index.selectArtifact(
                    episodeID: artifact.subjectID, receipt: receipt
                )
            }
        }
    }

    private func transcriptDesiredIDs() -> Set<UUID> {
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
        return ids
    }

    private func verifyAndAdoptFilesystemArtifacts() throws -> Int {
        var adopted = 0
        for episode in appStore.state.episodes {
            let audioVersion = DesiredStatePlanner.audioVersion(episode)
            let existing = try artifacts.current(kind: .transcript, subjectID: episode.id)
            if let existing, let location = existing.location,
               let data = TranscriptStore.shared.verifiedData(
                at: URL(fileURLWithPath: location), episodeID: episode.id
               ), ArtifactRepository.hash(data) == existing.contentHash {
                if existing.inputVersion != audioVersion || existing.integrity != .available {
                    try artifacts.markIntegrity(
                        kind: .transcript, subjectID: episode.id, integrity: .stale
                    )
                } else {
                    _ = appStore.applyTranscriptEvent(.artifactAdopted(.init(
                        inputVersion: existing.inputVersion,
                        contentHash: existing.contentHash,
                        fileURL: URL(fileURLWithPath: location),
                        source: TranscriptState.Source(rawValue: existing.origin ?? "") ?? .other
                    )), episodeID: episode.id)
                }
            } else if let staged = TranscriptStore.shared.recoverableStagedOutput(
                episodeID: episode.id, inputVersion: audioVersion
            ) {
                let url = try TranscriptStore.shared.promoteStaged(
                    episodeID: episode.id,
                    leaseToken: staged.leaseToken,
                    contentHash: staged.contentHash
                )
                try adoptTranscript(
                    episode: episode, inputVersion: audioVersion,
                    hash: staged.contentHash, location: url.path, origin: "recovered-attempt"
                )
                adopted += 1
            } else if let data = TranscriptStore.shared.verifiedData(
                at: TranscriptStore.shared.fileURL(for: episode.id), episodeID: episode.id
            ) {
                let hash = ArtifactRepository.hash(data)
                let url = TranscriptStore.shared.contentFileURL(
                    for: episode.id, contentHash: hash
                )
                try FileManager.default.createDirectory(
                    at: url.deletingLastPathComponent(), withIntermediateDirectories: true
                )
                if !FileManager.default.fileExists(atPath: url.path) {
                    try data.write(to: url, options: .withoutOverwriting)
                }
                try adoptTranscript(
                    episode: episode, inputVersion: audioVersion,
                    hash: hash, location: url.path, origin: transcriptOrigin(episode)
                )
                adopted += 1
            } else if existing != nil {
                try artifacts.markIntegrity(
                    kind: .transcript, subjectID: episode.id, integrity: .corrupt
                )
            }

            adopted += try reconcileDownloadArtifact(
                episode: episode,
                inputVersion: audioVersion
            )
            adopted += try adoptInlinePublisherChapters(episode: episode)
            try restoreDerivedProjection(kind: .chapters, episodeID: episode.id)
            try restoreDerivedProjection(kind: .adSegments, episodeID: episode.id)
        }
        return adopted
    }

    private func reconcileDownloadArtifact(
        episode: Episode,
        inputVersion: String
    ) throws -> Int {
        let repository = EpisodeDownloadStore.shared
        let existing = try artifacts.current(kind: .downloadFile, subjectID: episode.id)
        if let existing {
            let url = existing.location.map(URL.init(fileURLWithPath:))
            if existing.inputVersion == inputVersion,
               existing.integrity == .available,
               let url,
               let data = try? Data(contentsOf: url, options: .mappedIfSafe),
               ArtifactRepository.hash(data) == existing.contentHash {
                _ = appStore.applyDownloadEvent(.artifactRecovered(.init(
                    inputVersion: inputVersion,
                    contentHash: existing.contentHash,
                    fileURL: url,
                    byteCount: Int64(data.count)
                )), episodeID: episode.id)
                return 0
            }
            let integrity: ArtifactIntegrity = existing.inputVersion == inputVersion
                ? .corrupt : .stale
            try artifacts.markIntegrity(
                kind: .downloadFile,
                subjectID: episode.id,
                integrity: integrity
            )
            _ = appStore.applyDownloadEvent(
                .artifactInvalidated(inputVersion: inputVersion),
                episodeID: episode.id
            )
        }

        if let staged = repository.recoverableStagedOutput(
            episodeID: episode.id,
            inputVersion: inputVersion
        ) {
            if let job = try jobStore.job(id: staged.jobID),
               job.state == .cancelled || job.state == .obsolete {
                repository.discard(staged)
                return 0
            }
            let selected = try repository.promote(staged, episode: episode)
            try artifacts.adopt(ArtifactRecord(
                kind: .downloadFile,
                subjectID: episode.id,
                inputVersion: inputVersion,
                outputVersion: staged.contentHash,
                contentHash: staged.contentHash,
                location: selected.path,
                origin: "recovered-attempt",
                schemaVersion: 1,
                integrity: .available,
                verifiedAt: now()
            ))
            _ = appStore.applyDownloadEvent(.artifactRecovered(.init(
                inputVersion: inputVersion,
                contentHash: staged.contentHash,
                fileURL: selected,
                byteCount: staged.byteCount
            )), episodeID: episode.id)
            return 1
        }

        // A stable local-file projection without artifact metadata is a
        // one-time adoption source (for locally generated episodes).
        if existing == nil,
           case .downloaded(let url, _) = episode.downloadState,
           let data = try? Data(contentsOf: url, options: .mappedIfSafe) {
            let hash = ArtifactRepository.hash(data)
            try artifacts.adopt(ArtifactRecord(
                kind: .downloadFile,
                subjectID: episode.id,
                inputVersion: inputVersion,
                outputVersion: hash,
                contentHash: hash,
                location: url.path,
                origin: "stable-projection",
                schemaVersion: 1,
                integrity: .available,
                verifiedAt: now()
            ))
            return 1
        }
        return 0
    }

    private func adoptInlinePublisherChapters(episode: Episode) throws -> Int {
        guard episode.chaptersURL == nil,
              let sourceVersion = DesiredStatePlanner.publisherChapterInputVersion(episode),
              let chapters = episode.chapters,
              !chapters.isEmpty else { return 0 }
        let current = try artifacts.current(kind: .chapters, subjectID: episode.id)
        let publisherOrigin = DesiredStatePlanner.publisherChapterOrigin(
            sourceVersion: sourceVersion,
            enriched: false
        )
        let enrichedOrigin = DesiredStatePlanner.publisherChapterOrigin(
            sourceVersion: sourceVersion,
            enriched: true
        )
        if current?.integrity == .available,
           current?.origin == publisherOrigin || current?.origin == enrichedOrigin {
            return 0
        }
        let stored = try DerivedArtifactStagingStore.shared.adoptPublisherChapters(
            chapters,
            episodeID: episode.id
        )
        try artifacts.adopt(ArtifactRecord(
            kind: .chapters,
            subjectID: episode.id,
            inputVersion: sourceVersion,
            outputVersion: stored.contentHash,
            contentHash: stored.contentHash,
            location: stored.url.path,
            origin: publisherOrigin,
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: now()
        ))
        return 1
    }

    private func restoreDerivedProjection(kind: ArtifactKind, episodeID: UUID) throws {
        guard let artifact = try artifacts.current(kind: kind, subjectID: episodeID),
              artifact.integrity == .available,
              let location = artifact.location else { return }
        let url = URL(fileURLWithPath: location)
        guard let data = try? Data(contentsOf: url),
              ArtifactRepository.hash(data) == artifact.contentHash else {
            try artifacts.markIntegrity(kind: kind, subjectID: episodeID, integrity: .corrupt)
            return
        }
        switch kind {
        case .chapters:
            guard let chapters = DerivedArtifactStagingStore.shared.loadChapters(at: url) else {
                try artifacts.markIntegrity(kind: kind, subjectID: episodeID, integrity: .corrupt)
                return
            }
            appStore.setEpisodeChapters(episodeID, chapters: chapters)
        case .adSegments:
            guard let ads = DerivedArtifactStagingStore.shared.loadAds(at: url) else {
                try artifacts.markIntegrity(kind: kind, subjectID: episodeID, integrity: .corrupt)
                return
            }
            appStore.setEpisodeAdSegments(episodeID, segments: ads)
        default:
            break
        }
    }

    private func adoptTranscript(
        episode: Episode,
        inputVersion: String,
        hash: String,
        location: String,
        origin: String
    ) throws {
        try artifacts.adopt(ArtifactRecord(
            kind: .transcript, subjectID: episode.id,
            inputVersion: inputVersion, outputVersion: hash,
            contentHash: hash, location: location, origin: origin,
            schemaVersion: 1, integrity: .available, verifiedAt: now()
        ))
        _ = appStore.applyTranscriptEvent(.artifactAdopted(.init(
            inputVersion: inputVersion,
            contentHash: hash,
            fileURL: URL(fileURLWithPath: location),
            source: TranscriptState.Source(rawValue: origin) ?? .other
        )), episodeID: episode.id)
    }

    private func transcriptOrigin(_ episode: Episode) -> String {
        if case .ready(let source) = episode.transcriptState { return source.rawValue }
        return "adopted"
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

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
