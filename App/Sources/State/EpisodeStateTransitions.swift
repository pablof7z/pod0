import Foundation
import os.log

enum EpisodeTransitionResult: Equatable, Sendable {
    case applied
    case noOp
    case stale
    case rejected(String)
}

struct DownloadArtifactEvidence: Equatable, Sendable {
    let inputVersion: String
    let contentHash: String
    let fileURL: URL
    let byteCount: Int64
}

enum DownloadDomainEvent: Equatable, Sendable {
    case artifactCommitted(DownloadArtifactEvidence)
    case artifactRecovered(DownloadArtifactEvidence)
    case artifactInvalidated(inputVersion: String)
    case userRemoved
}

struct TranscriptArtifactEvidence: Equatable, Sendable {
    let inputVersion: String
    let contentHash: String
    let fileURL: URL
    let source: TranscriptState.Source
}

enum TranscriptDomainEvent: Equatable, Sendable {
    case artifactCommitted(TranscriptArtifactEvidence)
    case artifactAdopted(TranscriptArtifactEvidence)
    case artifactInvalidated(inputVersion: String)
}

extension AppStateStore {
    @discardableResult
    func applyDownloadEvent(
        _ event: DownloadDomainEvent,
        episodeID: UUID
    ) -> EpisodeTransitionResult {
        guard let index = state.episodes.firstIndex(where: { $0.id == episodeID }) else {
            return rejectEpisodeTransition("Episode does not exist")
        }
        let episode = state.episodes[index]
        let next: DownloadState
        switch event {
        case .artifactCommitted(let evidence), .artifactRecovered(let evidence):
            guard evidence.inputVersion == DesiredStatePlanner.audioVersion(episode) else {
                return .stale
            }
            guard let data = try? Data(contentsOf: evidence.fileURL, options: .mappedIfSafe),
                  Int64(data.count) == evidence.byteCount,
                  ArtifactRepository.hash(data) == evidence.contentHash else {
                return rejectEpisodeTransition(
                    "Download evidence is missing or failed integrity verification"
                )
            }
            next = .downloaded(
                localFileURL: evidence.fileURL,
                byteCount: evidence.byteCount
            )
        case .artifactInvalidated(let inputVersion):
            guard inputVersion == DesiredStatePlanner.audioVersion(episode) else { return .stale }
            next = .notDownloaded
        case .userRemoved:
            next = .notDownloaded
        }
        guard episode.downloadState != next else { return .noOp }
        var episodes = state.episodes
        episodes[index].downloadState = next
        performMutationBatch {
            mutateState { $0.episodes = episodes }
            invalidateEpisodeProjections()
        }
        return .applied
    }

    @discardableResult
    func applyTranscriptEvent(
        _ event: TranscriptDomainEvent,
        episodeID: UUID
    ) -> EpisodeTransitionResult {
        guard let index = state.episodes.firstIndex(where: { $0.id == episodeID }) else {
            return rejectEpisodeTransition("Episode does not exist")
        }
        let episode = state.episodes[index]
        let next: TranscriptState
        switch event {
        case .artifactCommitted(let evidence), .artifactAdopted(let evidence):
            guard evidence.inputVersion == DesiredStatePlanner.audioVersion(episode) else {
                return .stale
            }
            guard let data = TranscriptStore.shared.verifiedData(
                at: evidence.fileURL, episodeID: episodeID
            ), ArtifactRepository.hash(data) == evidence.contentHash else {
                return rejectEpisodeTransition(
                    "Transcript evidence is missing, unparseable, or corrupt"
                )
            }
            next = .ready(source: evidence.source)
        case .artifactInvalidated(let inputVersion):
            guard inputVersion == DesiredStatePlanner.audioVersion(episode) else { return .stale }
            next = .none
        }
        guard episode.transcriptState != next else { return .noOp }
        var episodes = state.episodes
        episodes[index].transcriptState = next
        performMutationBatch {
            mutateState { $0.episodes = episodes }
            invalidateEpisodeProjections()
        }
        if case .ready = next {
            recordProductSignal(.once(
                name: .transcriptReady,
                subjectID: episodeID,
                outcome: .ready,
                domainRevision: state.persistenceGeneration
            ))
        }
        return .applied
    }

    func rejectEpisodeTransition(_ message: String) -> EpisodeTransitionResult {
        Logger.app("EpisodeStateTransitions").error("Rejected domain event: \(message, privacy: .public)")
        return .rejected(message)
    }
}
