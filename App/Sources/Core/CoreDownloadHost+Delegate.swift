import Foundation
import Pod0Core
import os.log

final class CoreDownloadCoordinator: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
    private let lock = NSLock()
    private weak var _host: CoreDownloadHost?
    private static let logger = Logger.app("CoreDownloadCoordinator")

    private var host: CoreDownloadHost? {
        lock.lock()
        defer { lock.unlock() }
        return _host
    }

    func bind(host: CoreDownloadHost) {
        lock.lock()
        _host = host
        lock.unlock()
    }

    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didWriteData bytesWritten: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        guard let identity = CoreDownloadTaskIdentity(encoded: downloadTask.taskDescription) else {
            return
        }
        Task { @MainActor [weak host] in
            host?.handleProgress(
                identity: identity,
                taskID: downloadTask.taskIdentifier,
                totalBytesWritten: totalBytesWritten,
                totalBytesExpected: totalBytesExpectedToWrite
            )
        }
    }

    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didFinishDownloadingTo location: URL
    ) {
        guard let identity = CoreDownloadTaskIdentity(encoded: downloadTask.taskDescription) else {
            return
        }
        let status = (downloadTask.response as? HTTPURLResponse)?.statusCode
        guard status.map({ (200 ..< 300).contains($0) }) ?? true else {
            let code = Self.failureCode(httpStatus: status)
            let detail = status.map { "HTTP response \($0)" } ?? "Invalid HTTP response"
            Task { @MainActor [weak host] in
                host?.handleFailure(
                    identity: identity,
                    taskID: downloadTask.taskIdentifier,
                    code: code,
                    safeDetail: detail
                )
            }
            return
        }

        let interim: URL
        do {
            let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
                "pod0-core-download-handoff",
                isDirectory: true
            )
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
            interim = directory.appendingPathComponent(
                "\(identity.stableAttemptKey)-\(UUID().uuidString)",
                isDirectory: false
            )
            try FileManager.default.moveItem(at: location, to: interim)
        } catch {
            Self.logger.error("download handoff failed: \(error, privacy: .public)")
            Task { @MainActor [weak host] in
                host?.handleFailure(
                    identity: identity,
                    taskID: downloadTask.taskIdentifier,
                    code: .platformFailure,
                    safeDetail: "Native download staging failed"
                )
            }
            return
        }
        Task { @MainActor [weak host] in
            host?.handleDownloaded(
                identity: identity,
                taskID: downloadTask.taskIdentifier,
                interim: interim
            )
        }
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: Error?
    ) {
        guard let error,
              let identity = CoreDownloadTaskIdentity(encoded: task.taskDescription)
        else { return }
        let nsError = error as NSError
        let resumeData = nsError.userInfo[NSURLSessionDownloadTaskResumeData] as? Data
        let status = (task.response as? HTTPURLResponse)?.statusCode
        Task { @MainActor [weak host] in
            host?.handleCompletionError(
                identity: identity,
                taskID: task.taskIdentifier,
                error: nsError,
                httpStatus: status,
                resumeData: resumeData
            )
        }
    }

    func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        Task { @MainActor [weak host] in
            host?.handleBackgroundEventsFinished(for: session)
        }
    }

    static func failureCode(httpStatus: Int?) -> HostFailureCode {
        guard let httpStatus else { return .platformFailure }
        return switch httpStatus {
        case 401: .unauthorized
        case 403: .permissionDenied
        case 408: .timedOut
        case 413: .responseTooLarge
        case 500 ... 599: .providerUnavailable
        default: .invalidResponse
        }
    }

    static func failureCode(_ error: NSError) -> HostFailureCode {
        guard error.domain == NSURLErrorDomain else { return .platformFailure }
        return switch error.code {
        case NSURLErrorNotConnectedToInternet: .offline
        case NSURLErrorTimedOut: .timedOut
        case NSURLErrorUserAuthenticationRequired: .unauthorized
        case NSURLErrorNoPermissionsToReadFile: .permissionDenied
        case NSURLErrorBadServerResponse, NSURLErrorCannotParseResponse: .invalidResponse
        default: .platformFailure
        }
    }
}

extension CoreDownloadHost {
    func handleProgress(
        identity: CoreDownloadTaskIdentity,
        taskID: Int,
        totalBytesWritten: Int64,
        totalBytesExpected: Int64
    ) {
        guard identitiesByTask[taskID] == identity else { return }
        let expected = totalBytesExpected > 0 ? UInt64(totalBytesExpected) : nil
        let fraction: Double
        if let expected {
            fraction = min(
                1,
                max(0, Double(totalBytesWritten) / Double(expected))
            )
        } else {
            fraction = 0
        }
        let timestamp = Date()
        let previous = lastPublishedProgress[identity.episodeID] ?? 0
        let lastDate = lastPublishedAt[identity.episodeID] ?? .distantPast
        guard fraction == 1
                || fraction - previous >= 0.05
                || timestamp.timeIntervalSince(lastDate) >= 0.2
        else { return }
        progress[identity.episodeID] = fraction
        expectedBytes[identity.episodeID] = expected
        lastPublishedProgress[identity.episodeID] = fraction
        lastPublishedAt[identity.episodeID] = timestamp
    }

    func handleDownloaded(
        identity: CoreDownloadTaskIdentity,
        taskID: Int,
        interim: URL
    ) {
        do {
            let (staged, byteCount) = try nativeStore.stage(interim, for: identity.attemptID)
            clearTask(identity: identity, taskID: taskID)
            emit(
                requestID: identity.requestID,
                sequence: 2,
                observation: .downloadStaged(
                    episodeId: identity.episodeID,
                    intentId: identity.intentID,
                    attemptId: identity.attemptID,
                    stagedFilePath: staged.path,
                    byteCount: byteCount
                ),
                identity: identity
            )
        } catch {
            try? FileManager.default.removeItem(at: interim)
            handleFailure(
                identity: identity,
                taskID: taskID,
                code: .platformFailure,
                safeDetail: "Native download staging failed"
            )
        }
    }

    func handleCompletionError(
        identity: CoreDownloadTaskIdentity,
        taskID: Int,
        error: NSError,
        httpStatus: Int?,
        resumeData: Data?
    ) {
        if cancelledTaskIDs.remove(taskID) != nil { return }
        nativeStore.saveResumeData(resumeData, for: identity.attemptID)
        let code = httpStatus.map(CoreDownloadCoordinator.failureCode(httpStatus:))
            ?? CoreDownloadCoordinator.failureCode(error)
        handleFailure(
            identity: identity,
            taskID: taskID,
            code: code,
            safeDetail: httpStatus.map { "HTTP response \($0)" }
                ?? "Native transfer failed (\(error.domain):\(error.code))"
        )
    }

    func handleFailure(
        identity: CoreDownloadTaskIdentity,
        taskID: Int,
        code: HostFailureCode,
        safeDetail: String
    ) {
        clearTask(identity: identity, taskID: taskID)
        emit(
            requestID: identity.requestID,
            sequence: 2,
            observation: .failed(code: code, safeDetail: String(safeDetail.prefix(256))),
            identity: identity
        )
    }

    private func clearTask(identity: CoreDownloadTaskIdentity, taskID: Int) {
        identitiesByTask[taskID] = nil
        tasksByRequest[identity.requestID] = nil
        clearProgress(for: identity.episodeID)
    }
}
