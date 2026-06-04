import Foundation
import os.log

enum LocalModelState: Equatable, Sendable {
    case notDownloaded
    case downloading(progress: Double)
    case downloaded
    case active
}

@MainActor
@Observable
final class LocalModelDownloadManager: NSObject, URLSessionDownloadDelegate {
    /// The single instance the app uses. A background `URLSession` permits only
    /// one live session per identifier, and the session strongly retains its
    /// delegate until invalidated — so constructing a fresh manager per view
    /// appearance both violated that rule and leaked the old manager, leaving
    /// the visible UI bound to a manager that no longer received the download
    /// callbacks ("Cancel" with no progress). Everything goes through `.shared`.
    static let shared = LocalModelDownloadManager()

    /// Background-session identifier. Shared with `AppDelegate` so OS relaunch
    /// events for this session are routed to this manager rather than to the
    /// episode download capability.
    static let sessionIdentifier = "com.podcastr.local-model-downloads"

    private(set) var states: [String: LocalModelState] = [:]
    private var session: URLSession?
    private var activeDownloads: [String: URL] = [:]
    private let fileManager = FileManager.default
    private let modelsDirectoryURL: URL

    /// OS handoff completion handler, stored when iOS relaunches the app to
    /// deliver this session's background events. Drained in
    /// `urlSessionDidFinishEvents` once all queued events are processed.
    private var backgroundCompletionHandler: (() -> Void)?

    /// Private: the one background session per identifier lives behind
    /// `.shared`. Constructing a second instance would spin up a duplicate
    /// session for the same identifier and reintroduce the leak this type was
    /// fixed to avoid — the compiler now forbids it.
    private override init() {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        modelsDirectoryURL = appSupport.appendingPathComponent("LocalModels", isDirectory: true)
        super.init()

        do {
            try fileManager.createDirectory(at: modelsDirectoryURL, withIntermediateDirectories: true)
        } catch {
            os_log("Failed to create LocalModels directory: %{public}@", log: .default, type: .error, error.localizedDescription)
        }

        let config = URLSessionConfiguration.background(withIdentifier: Self.sessionIdentifier)
        config.sessionSendsLaunchEvents = true
        self.session = URLSession(configuration: config, delegate: self, delegateQueue: OperationQueue.main)

        recomputeStatesFromDisk(activeModelID: nil)
    }

    func recomputeStatesFromDisk(activeModelID: String?) {
        var newStates: [String: LocalModelState] = [:]

        for spec in LocalModelCatalog.all {
            // Preserve an in-flight download: this method rebuilds every state
            // from disk and runs on each view appearance, so without this guard
            // navigating away and back mid-download would reset the row to
            // .notDownloaded ("Download") until the next progress callback.
            if case .downloading = states[spec.id] {
                newStates[spec.id] = states[spec.id]
                continue
            }

            let fileURL = modelFileURL(for: spec.id)
            let fileExists = fileManager.fileExists(atPath: fileURL.path)

            if activeModelID == spec.id && fileExists {
                newStates[spec.id] = .active
            } else if fileExists {
                newStates[spec.id] = .downloaded
            } else {
                newStates[spec.id] = .notDownloaded
            }
        }

        states = newStates
    }

    func modelFileURL(for modelID: String) -> URL {
        modelsDirectoryURL.appendingPathComponent("\(modelID).litertlm")
    }

    func download(spec: LocalModelSpec) {
        guard let session = session else { return }

        states[spec.id] = .downloading(progress: 0.0)

        var request = URLRequest(url: spec.downloadURL)
        request.timeoutInterval = 3600 // 1 hour timeout

        let downloadTask = session.downloadTask(with: request)
        activeDownloads[spec.id] = modelFileURL(for: spec.id)
        downloadTask.resume()
    }

    func cancel(_ modelID: String) {
        guard let session = session else { return }
        if let spec = LocalModelCatalog.all.first(where: { $0.id == modelID }) {
            session.getTasksWithCompletionHandler { dataTasks, uploadTasks, downloadTasks in
                downloadTasks.forEach { task in
                    if let url = task.originalRequest?.url, url == spec.downloadURL {
                        task.cancel()
                    }
                }
            }
        }
        states[modelID] = .notDownloaded
        activeDownloads.removeValue(forKey: modelID)
    }

    func delete(_ modelID: String) {
        let fileURL = modelFileURL(for: modelID)
        try? fileManager.removeItem(at: fileURL)
        states[modelID] = .notDownloaded
        activeDownloads.removeValue(forKey: modelID)
    }

    func state(for modelID: String) -> LocalModelState {
        return states[modelID] ?? .notDownloaded
    }

    /// Whether the model's weights are present on disk. Drives which local
    /// models appear as selectable entries in the per-role model selector —
    /// only downloaded models are offered, so "download to make available" is
    /// honest. Reads the filesystem directly rather than the cached `states`
    /// map so it is correct before any `recomputeStatesFromDisk` runs.
    func isDownloaded(_ modelID: String) -> Bool {
        fileManager.fileExists(atPath: modelFileURL(for: modelID).path)
    }

    /// Catalog specs whose weights are downloaded and ready to use.
    func downloadedSpecs() -> [LocalModelSpec] {
        LocalModelCatalog.all.filter { isDownloaded($0.id) }
    }

    // MARK: - Background-session OS handoff

    /// Wired from `AppDelegate.application(_:handleEventsForBackgroundURLSession:)`.
    /// Holds the OS completion handler until this session signals — via
    /// `urlSessionDidFinishEvents` — that every queued event (including the
    /// final `didFinishDownloadingTo` that moves the file into place) has been
    /// delivered. Calling it before then would let iOS re-suspend the app while
    /// the multi-GB transfer is still being finalized.
    func handleEventsForBackgroundURLSession(completionHandler: @escaping () -> Void) {
        backgroundCompletionHandler = completionHandler
    }

    // MARK: - URLSessionDownloadDelegate

    nonisolated func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didFinishDownloadingTo temporaryURL: URL
    ) {
        // Match by download URL — identical to the sibling didWriteData /
        // didCompleteWithError methods. The previous basename comparison
        // (local "gemma4-e2b" vs remote "gemma-4-E2B-it") could never be equal,
        // so this guard always failed: the temp file was never moved, the model
        // never persisted, and the row was stuck at .downloading(1.0) until a
        // relaunch reset it to .notDownloaded. Using the catalog URL also works
        // when iOS re-delivers a finished background task after a cold launch,
        // when `activeDownloads` is empty (the `?? modelFileURL` fallback below).
        guard let originalURL = downloadTask.originalRequest?.url,
              let modelID = LocalModelCatalog.modelID(forDownloadURL: originalURL) else { return }

        MainActor.assumeIsolated {
            let destinationURL = activeDownloads[modelID] ?? modelFileURL(for: modelID)

            do {
                try fileManager.removeItem(at: destinationURL)
            } catch {
                // File might not exist yet
            }

            do {
                try fileManager.moveItem(at: temporaryURL, to: destinationURL)
                states[modelID] = .downloaded
                activeDownloads.removeValue(forKey: modelID)
            } catch {
                os_log("Failed to move downloaded model: %{public}@", log: .default, type: .error, error.localizedDescription)
                states[modelID] = .notDownloaded
            }
        }
    }

    nonisolated func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didWriteData bytesWritten: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        guard let originalURL = downloadTask.originalRequest?.url,
              let modelID = LocalModelCatalog.modelID(forDownloadURL: originalURL) else { return }

        let progress = totalBytesExpectedToWrite > 0 ? Double(totalBytesWritten) / Double(totalBytesExpectedToWrite) : 0.0
        MainActor.assumeIsolated {
            states[modelID] = .downloading(progress: progress)
        }
    }

    nonisolated func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: Error?
    ) {
        guard let originalURL = task.originalRequest?.url,
              let modelID = LocalModelCatalog.modelID(forDownloadURL: originalURL) else { return }

        MainActor.assumeIsolated {
            if let error = error {
                os_log("Download failed for model %{public}@: %{public}@", log: .default, type: .error, modelID, error.localizedDescription)
                states[modelID] = .notDownloaded
            }
            activeDownloads.removeValue(forKey: modelID)
        }
    }

    /// All queued background events for this session have been delivered. Drain
    /// the stored OS completion handler so iOS knows the app finished processing
    /// and can suspend cleanly.
    nonisolated func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        MainActor.assumeIsolated {
            let handler = backgroundCompletionHandler
            backgroundCompletionHandler = nil
            handler?()
        }
    }
}
