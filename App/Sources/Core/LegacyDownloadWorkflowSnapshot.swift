import Foundation
import Pod0Core

struct LegacyDownloadWorkflowSnapshot {
    let sourceGeneration: UInt64
    let candidates: [LegacyDownloadCutoverCandidate]
    let resumeDataByEpisodeID: [UUID: Data]
    let backup: LegacyDownloadWorkflowBackup

    static func capture(
        state: AppState,
        jobStore: JobStore,
        artifactRepository: ArtifactRepository,
        tasks: [LegacyDownloadWorkflowBackup.TaskEvidence],
        producedResumeData: [UUID: Data],
        downloadStore: LegacyDownloadSourceStore = .shared
    ) throws -> Self {
        let jobs = try jobStore.allJobs()
            .filter { $0.kind == .download || $0.kind == .autoDownload }
            .sorted { $0.id.uuidString < $1.id.uuidString }
        let artifacts = try artifactRepository.all()
            .filter { $0.kind == .downloadFile || $0.kind == .autoDownloadDecision }
            .sorted { lhs, rhs in
                if lhs.subjectID != rhs.subjectID {
                    return lhs.subjectID.uuidString < rhs.subjectID.uuidString
                }
                return lhs.outputVersion < rhs.outputVersion
            }
        let jobsByEpisode = Dictionary(grouping: jobs, by: \WorkJob.subjectID)
        let tasksByEpisode = Dictionary(grouping: tasks.compactMap { task in
            task.episodeID.map { ($0, task) }
        }, by: \.0)
        var candidates: [LegacyDownloadCutoverCandidate] = []
        var evidence: [LegacyDownloadWorkflowBackup.CandidateEvidence] = []
        var resumeData: [UUID: Data] = [:]

        for episode in state.episodes.sorted(by: { $0.id.uuidString < $1.id.uuidString }) {
            let episodeJobs = jobsByEpisode[episode.id] ?? []
            let selected = selectedSource(
                episode: episode,
                jobs: episodeJobs,
                tasks: tasksByEpisode[episode.id]?.map(\.1) ?? [],
                producedResumeData: producedResumeData[episode.id],
                downloadStore: downloadStore
            )
            guard let selected else { continue }
            candidates.append(LegacyDownloadCutoverCandidate(
                episodeId: EpisodeId(uuid: episode.id),
                origin: selected.origin.coreValue,
                disposition: selected.disposition
            ))
            let resume = selected.resumeData
            if let resume { resumeData[episode.id] = resume }
            evidence.append(.init(
                episodeID: episode.id,
                origin: selected.origin,
                disposition: selected.sourcePath == nil ? .restart : .available,
                sourcePath: selected.sourcePath,
                byteCount: selected.byteCount,
                resumeByteCount: resume?.count,
                resumeDigest: LegacyDownloadWorkflowBackup.digest(resume)
            ))
        }
        let generation = try LegacyDownloadWorkflowBackup.generation(
            persistenceGeneration: state.persistenceGeneration,
            jobs: jobs,
            artifacts: artifacts,
            tasks: tasks,
            candidates: evidence
        )
        return Self(
            sourceGeneration: generation,
            candidates: candidates,
            resumeDataByEpisodeID: resumeData,
            backup: LegacyDownloadWorkflowBackup(
                formatVersion: 1,
                sourceGeneration: generation,
                persistenceGeneration: state.persistenceGeneration,
                jobs: jobs,
                artifacts: artifacts,
                tasks: tasks,
                candidates: evidence
            )
        )
    }
}

private extension LegacyDownloadWorkflowSnapshot {
    struct SelectedSource {
        let origin: LegacyDownloadIntentOrigin
        let disposition: LegacyDownloadCutoverDisposition
        let sourcePath: String?
        let byteCount: Int64?
        let resumeData: Data?
    }

    static func selectedSource(
        episode: Episode,
        jobs: [WorkJob],
        tasks: [LegacyDownloadWorkflowBackup.TaskEvidence],
        producedResumeData: Data?,
        downloadStore: LegacyDownloadSourceStore
    ) -> SelectedSource? {
        let currentVersion = DesiredStatePlanner.audioVersion(episode)
        let currentJobs = jobs
            .filter { $0.inputVersion == currentVersion }
            .sorted { $0.updatedAt > $1.updatedAt }
        if case .downloaded(let url, let byteCount) = episode.downloadState {
            return SelectedSource(
                origin: origin(currentJobs.first),
                disposition: .available(sourcePath: url.path, byteCount: UInt64(max(0, byteCount))),
                sourcePath: url.path,
                byteCount: byteCount,
                resumeData: nil
            )
        }
        for job in currentJobs {
            if let staged = downloadStore.recoverableStagedOutput(
                episodeID: episode.id,
                inputVersion: job.inputVersion
            ) {
                return SelectedSource(
                    origin: origin(job),
                    disposition: .available(
                        sourcePath: staged.fileURL.path,
                        byteCount: UInt64(max(0, staged.byteCount))
                    ),
                    sourcePath: staged.fileURL.path,
                    byteCount: staged.byteCount,
                    resumeData: nil
                )
            }
        }
        let active = currentJobs.first(where: { $0.state.isActive })
        guard active != nil || !tasks.isEmpty else { return nil }
        let resume = producedResumeData ?? downloadStore.loadResumeData(episodeID: episode.id)
        return SelectedSource(
            origin: origin(active),
            disposition: .restart(resumeAvailable: resume?.isEmpty == false),
            sourcePath: nil,
            byteCount: nil,
            resumeData: resume
        )
    }

    static func origin(_ job: WorkJob?) -> LegacyDownloadIntentOrigin {
        guard let data = job?.payload,
              let payload = try? JSONDecoder().decode(LegacyDownloadJobPayload.self, from: data)
        else { return .user }
        return payload.origin
    }
}

extension LegacyDownloadIntentOrigin {
    var coreValue: Pod0Core.DownloadIntentOrigin {
        switch self {
        case .user: .user
        case .playback: .playback
        case .autoDownload: .automatic
        }
    }
}
