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
            if let localURL = episode.downloadState.localFileURL,
               FileManager.default.fileExists(atPath: localURL.path) {
                return localURL
            }
            if episode.enclosureURL.isFileURL,
               FileManager.default.fileExists(atPath: episode.enclosureURL.path) {
                return episode.enclosureURL
            }
            store.sharedLibrary?.requestDownload(episodeID: uuid)
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
        do {
            let deadline = ContinuousClock.now.advanced(by: .seconds(timeout))
            while ContinuousClock.now < deadline {
                try Task.checkCancellation()
                if let result = await MainActor.run(body: { () -> URL? in
                    guard let episode = store?.episode(id: uuid),
                          let url = episode.downloadState.localFileURL,
                          FileManager.default.fileExists(atPath: url.path)
                    else { return nil }
                    return url
                }) {
                    return result
                }
                if await MainActor.run(body: {
                    store?.sharedLibrary?.downloadWorkflow(episodeID: uuid)?.stage == .failed
                }) {
                    throw AgentTTSError.snippetDownloadFailed(
                        episodeID: episodeID,
                        message: "Shared download workflow failed"
                    )
                }
                try await Task.sleep(for: .milliseconds(250))
            }
            throw AgentTTSError.snippetDownloadTimeout(episodeID: episodeID)
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
