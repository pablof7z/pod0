import Foundation

extension Reconciler {
    func verifyAndAdoptFilesystemArtifacts() throws -> Int {
        var adopted = 0
        for episode in appStore.state.episodes {
            let audioVersion = DesiredStatePlanner.audioVersion(episode)
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

}
