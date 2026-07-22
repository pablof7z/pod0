import Foundation

final class LegacyDownloadSessionRetirement: NSObject, URLSessionDelegate, @unchecked Sendable {
    static let shared = LegacyDownloadSessionRetirement()
    static let identifier = "io.f7z.podcast.downloads"

    private let lock = NSLock()
    private var completionHandlers: [() -> Void] = []
    private lazy var session: URLSession = {
        let configuration = URLSessionConfiguration.background(withIdentifier: Self.identifier)
        configuration.sessionSendsLaunchEvents = true
        configuration.waitsForConnectivity = false
        return URLSession(configuration: configuration, delegate: self, delegateQueue: nil)
    }()

    func captureAndCancel() -> (
        tasks: [LegacyDownloadWorkflowBackup.TaskEvidence],
        resumeData: [UUID: Data]
    ) {
        let tasks = allDownloadTasks()
        let evidence = tasks.map { task in
            let identity = Self.parseTaskDescription(task.taskDescription)
            return LegacyDownloadWorkflowBackup.TaskEvidence(
                taskIdentifier: task.taskIdentifier,
                jobID: identity?.jobID,
                episodeID: identity?.episodeID,
                inputVersion: identity?.inputVersion,
                originalURL: task.originalRequest?.url?.absoluteString,
                state: task.state.rawValue,
                receivedByteCount: task.countOfBytesReceived,
                expectedByteCount: task.countOfBytesExpectedToReceive
            )
        }.sorted { $0.taskIdentifier < $1.taskIdentifier }
        let group = DispatchGroup()
        let resumeData = LockedBox<[UUID: Data]>([:])
        for task in tasks {
            let episodeID = Self.parseTaskDescription(
                task.taskDescription
            )?.episodeID
            group.enter()
            task.cancel { data in
                if let episodeID, let data, !data.isEmpty {
                    resumeData.withValue { $0[episodeID] = data }
                }
                group.leave()
            }
        }
        _ = group.wait(timeout: .now() + 5)
        return (evidence, resumeData.snapshot())
    }

    func handleEvents(identifier: String, completion: @escaping () -> Void) {
        guard identifier == Self.identifier else {
            completion()
            return
        }
        lock.lock()
        completionHandlers.append(completion)
        lock.unlock()
        _ = session
    }

    func urlSessionDidFinishEvents(forBackgroundURLSession session: URLSession) {
        lock.lock()
        let handlers = completionHandlers
        completionHandlers.removeAll()
        lock.unlock()
        DispatchQueue.main.async { handlers.forEach { $0() } }
    }

    private func allDownloadTasks() -> [URLSessionDownloadTask] {
        let semaphore = DispatchSemaphore(value: 0)
        let result = LockedBox<[URLSessionDownloadTask]>([])
        session.getAllTasks { tasks in
            result.withValue { $0 = tasks.compactMap { $0 as? URLSessionDownloadTask } }
            semaphore.signal()
        }
        _ = semaphore.wait(timeout: .now() + 5)
        return result.snapshot()
    }

    private static func parseTaskDescription(
        _ value: String?
    ) -> (jobID: UUID, episodeID: UUID, inputVersion: String)? {
        guard let parts = value?.split(separator: ":"), parts.count == 6,
              parts[0] == "job", parts[2] == "episode", parts[4] == "input",
              let jobID = UUID(uuidString: String(parts[1])),
              let episodeID = UUID(uuidString: String(parts[3]))
        else { return nil }
        return (jobID, episodeID, String(parts[5]))
    }
}

private final class LockedBox<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: Value

    init(_ value: Value) { self.value = value }

    func withValue(_ mutation: (inout Value) -> Void) {
        lock.lock()
        mutation(&value)
        lock.unlock()
    }

    func snapshot() -> Value {
        lock.lock()
        defer { lock.unlock() }
        return value
    }
}
