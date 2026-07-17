import Foundation

enum DownloadNetworkStatus: Sendable, Equatable {
    case unknown
    case unavailable
    case wifi
    case other
}

enum DownloadAdmissionDecision: Sendable, Equatable {
    case admit
    case obsolete
    case wait(reason: String)
}

struct DownloadAdmissionPolicy: Sendable {
    /// Leave enough capacity for SQLite/WAL commits, URLSession staging, and
    /// normal application operation even when the enclosure size is unknown.
    static let minimumFreeCapacity: Int64 = 256 * 1_024 * 1_024

    func evaluate(
        origin: DownloadIntentOrigin,
        automaticPolicy: AutoDownloadPolicy,
        network: DownloadNetworkStatus,
        availableStorageCapacity: Int64?
    ) -> DownloadAdmissionDecision {
        if origin == .autoDownload {
            if case .off = automaticPolicy.mode { return .obsolete }
            if automaticPolicy.wifiOnly, network != .wifi {
                return .wait(reason: "Automatic download is waiting for Wi-Fi.")
            }
        }
        if network == .unavailable {
            return .wait(reason: "Download is waiting for a network connection.")
        }
        if let availableStorageCapacity,
           availableStorageCapacity < Self.minimumFreeCapacity {
            return .wait(reason: "Download is waiting for more free storage.")
        }
        return .admit
    }
}

extension EpisodeDownloadService {
    var isOnWiFi: Bool { pathState.status == .wifi }
    var networkStatus: DownloadNetworkStatus { pathState.status }

    var availableStorageCapacity: Int64? {
        try? EpisodeDownloadStore.shared.rootURL.resourceValues(
            forKeys: [.volumeAvailableCapacityForImportantUsageKey]
        ).volumeAvailableCapacityForImportantUsage
    }

    /// Cancels only the URLSession transfer owned by `jobID`. This is used
    /// when automatic policy or input version invalidates that one intent;
    /// unrelated user/playback intents are left intact.
    @discardableResult
    func cancelAdmittedTransfer(jobID: UUID, episodeID: UUID) -> Bool {
        guard let task = episodeIDToTask[episodeID],
              taskIDToJobID[task.taskIdentifier] == jobID else { return false }
        let episode = appStore?.episode(id: episodeID)
        task.cancel { resumeData in
            guard let resumeData, let episode else { return }
            EpisodeDownloadStore.shared.writeResumeData(resumeData, for: episode)
        }
        episodeIDToTask[episodeID] = nil
        taskIDToEpisodeID[task.taskIdentifier] = nil
        taskIDToJobID[task.taskIdentifier] = nil
        taskIDToInputVersion[task.taskIdentifier] = nil
        clearProgress(for: episodeID)
        finishWaiter(
            episodeID: episodeID,
            waiterID: jobID,
            result: .failure(JobFailure(
                classification: .cancelled,
                message: "Download intent became obsolete."
            ))
        )
        return true
    }
}

final class PathState: @unchecked Sendable {
    private let lock = NSLock()
    private var _status: DownloadNetworkStatus = .unknown

    var status: DownloadNetworkStatus {
        lock.lock(); defer { lock.unlock() }
        return _status
    }

    func set(_ status: DownloadNetworkStatus) {
        lock.lock(); defer { lock.unlock() }
        _status = status
    }
}
