import AVFoundation
import Foundation
import os.log

extension AgentTTSComposer {
    /// Resolves the raw native download capability without polling. The
    /// durable workflow still owns admission, retry, recovery, and selection.
    func resolveEpisodeAudio(
        episodeID: EpisodeID,
        timeout: TimeInterval = 300
    ) async throws -> URL {
        guard let uuid = UUID(uuidString: episodeID) else {
            throw AgentTTSError.snippetEpisodeNotFound(episodeID: episodeID)
        }

        let alreadyReady: URL? = await MainActor.run {
            guard let store, let episode = store.episode(id: uuid) else { return nil }
            if case .downloaded = episode.downloadState {
                let localURL = EpisodeDownloadStore.shared.localFileURL(for: episode)
                if FileManager.default.fileExists(atPath: localURL.path) {
                    return localURL
                }
            }
            let service = EpisodeDownloadService.shared
            service.attach(appStore: store)
            service.download(episodeID: uuid)
            return nil
        }
        if let alreadyReady { return alreadyReady }

        let episodeExists = await MainActor.run { store?.episode(id: uuid) != nil }
        guard episodeExists else {
            throw AgentTTSError.snippetEpisodeNotFound(episodeID: episodeID)
        }

        Self.logger.info(
            "AgentTTSComposer: awaiting download of snippet episode \(episodeID, privacy: .public)"
        )
        let observerID = UUID()
        do {
            return try await withThrowingTaskGroup(of: URL.self) { group in
                group.addTask {
                    try await EpisodeDownloadService.shared.observeDownloadCompletion(
                        episodeID: uuid,
                        observerID: observerID
                    )
                }
                group.addTask {
                    try await Task.sleep(for: .seconds(timeout))
                    throw AgentTTSError.snippetDownloadTimeout(episodeID: episodeID)
                }
                defer { group.cancelAll() }
                guard let result = try await group.next() else {
                    throw AgentTTSError.snippetDownloadTimeout(episodeID: episodeID)
                }
                return result
            }
        } catch let error as AgentTTSError {
            throw error
        } catch is CancellationError {
            throw CancellationError()
        } catch {
            throw AgentTTSError.snippetDownloadFailed(
                episodeID: episodeID,
                message: error.localizedDescription
            )
        }
    }

    /// Loads a real duration so failed tracks cannot corrupt later chapter
    /// offsets in the raw native composition.
    func audioDuration(of url: URL) async throws -> TimeInterval {
        let asset = AVURLAsset(url: url)
        do {
            let duration = try await asset.load(.duration)
            let seconds = CMTimeGetSeconds(duration)
            guard seconds > 0 else { throw AudioDurationError.zeroDuration(url) }
            return seconds
        } catch let error as AudioDurationError {
            throw error
        } catch {
            throw AudioDurationError.assetLoadFailed(url, underlying: error)
        }
    }
}

enum AudioDurationError: Error {
    case zeroDuration(URL)
    case assetLoadFailed(URL, underlying: Error)
}
