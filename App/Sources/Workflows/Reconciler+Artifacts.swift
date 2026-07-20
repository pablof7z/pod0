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

}
