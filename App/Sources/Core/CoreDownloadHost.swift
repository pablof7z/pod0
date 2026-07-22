import Foundation
import Observation
import Pod0Core
import os.log

@MainActor
@Observable
final class CoreDownloadHost: CoreDownloadHosting {
    static let shared = CoreDownloadHost()
    static let backgroundSessionIdentifier = "io.f7z.podcast.core-downloads"

    typealias Delivery = CoreDownloadHosting.Delivery

    let logger = Logger.app("CoreDownloadHost")
    let session: URLSession
    let sessionIdentifier: String?
    let coordinator: CoreDownloadCoordinator
    let nativeStore: CoreDownloadNativeStore

    var progress: [EpisodeId: Double] = [:]
    var expectedBytes: [EpisodeId: UInt64] = [:]

    var coreStoreURL: URL?
    var deliveries: [HostRequestId: Delivery] = [:]
    var identitiesByRequest: [HostRequestId: CoreDownloadTaskIdentity] = [:]
    var identitiesByTask: [Int: CoreDownloadTaskIdentity] = [:]
    var tasksByRequest: [HostRequestId: URLSessionDownloadTask] = [:]
    var cancelledTaskIDs: Set<Int> = []
    var lastPublishedProgress: [EpisodeId: Double] = [:]
    var lastPublishedAt: [EpisodeId: Date] = [:]
    var orphanObservationSink: OrphanDelivery?
    var pendingOrphanObservations: [CoreDownloadOrphanObservation] = []
    private var backgroundCompletionHandlers: [String: () -> Void] = [:]

    init(
        configuration: URLSessionConfiguration? = nil,
        nativeRootURL: URL? = nil,
        fileManager: FileManager = .default
    ) {
        nativeStore = CoreDownloadNativeStore(rootURL: nativeRootURL, fileManager: fileManager)
        let coordinator = CoreDownloadCoordinator()
        self.coordinator = coordinator
        let selectedConfiguration: URLSessionConfiguration
        if let configuration {
            selectedConfiguration = configuration
        } else {
            let background = URLSessionConfiguration.background(
                withIdentifier: Self.backgroundSessionIdentifier
            )
            background.isDiscretionary = false
            background.sessionSendsLaunchEvents = true
            background.allowsCellularAccess = true
            background.waitsForConnectivity = true
            selectedConfiguration = background
        }
        sessionIdentifier = selectedConfiguration.identifier
        session = URLSession(
            configuration: selectedConfiguration,
            delegate: coordinator,
            delegateQueue: nil
        )
        coordinator.bind(host: self)
    }

    func configure(coreStoreURL: URL) {
        self.coreStoreURL = coreStoreURL.standardizedFileURL
    }

    func installOrphanObservationSink(_ sink: @escaping OrphanDelivery) {
        orphanObservationSink = sink
        let pending = pendingOrphanObservations
        pendingOrphanObservations.removeAll(keepingCapacity: true)
        for observation in pending {
            sink(observation)
        }
    }

    func execute(_ envelope: HostRequestEnvelope, delivery: @escaping Delivery) {
        guard deliveries[envelope.requestId] == nil else { return }
        deliveries[envelope.requestId] = delivery
        switch envelope.request {
        case .startEpisodeDownload:
            start(envelope)
        case .cancelEpisodeDownload:
            cancel(envelope)
        case .removeEpisodeDownloadArtifact:
            remove(envelope)
        default:
            deliveries[envelope.requestId] = nil
            delivery(
                1,
                .failed(code: .invalidResponse, safeDetail: "Unexpected download host request")
            )
        }
    }

    func cancel(requestID: HostRequestId, cancellationID: CancellationId) {
        guard let identity = identitiesByRequest[requestID],
              identity.cancellationID == cancellationID
        else { return }
        deliveries[requestID] = nil
        identitiesByRequest[requestID] = nil
        guard let task = tasksByRequest.removeValue(forKey: requestID) else { return }
        cancelledTaskIDs.insert(task.taskIdentifier)
        identitiesByTask[task.taskIdentifier] = nil
        Task { [nativeStore, attemptID = identity.attemptID] in
            let resumeData = await task.cancelByProducingResumeData()
            nativeStore.saveResumeData(resumeData, for: attemptID)
        }
        clearProgress(for: identity.episodeID)
    }

    func retire(
        requestID: HostRequestId,
        observation: HostObservation,
        receipt: HostObservationReceipt
    ) {
        let terminal = switch receipt {
        case .persisted(_, let terminal): terminal
        case .rejected: true
        case .acceptedTransient, .retainAndRetry: false
        }
        guard terminal else { return }
        let identity = identitiesByRequest.removeValue(forKey: requestID)
        deliveries[requestID] = nil
        if let task = tasksByRequest.removeValue(forKey: requestID) {
            identitiesByTask[task.taskIdentifier] = nil
            if case .rejected = receipt {
                cancelledTaskIDs.insert(task.taskIdentifier)
                task.cancel()
            }
        }
        let attemptID = identity?.attemptID ?? observation.downloadAttemptID
        let episodeID = identity?.episodeID ?? observation.downloadEpisodeID
        switch observation {
        case .downloadStaged, .downloadCancelled:
            if let attemptID { nativeStore.removeNativeFiles(for: attemptID) }
        case .failed:
            break
        default:
            if case .rejected = receipt, let attemptID {
                nativeStore.removeNativeFiles(for: attemptID)
            }
        }
        if let episodeID { clearProgress(for: episodeID) }
    }

    func shutdown() {
        orphanObservationSink = nil
        deliveries.removeAll()
        identitiesByRequest.removeAll()
        tasksByRequest.removeAll()
    }

    func handleEventsForBackgroundURLSession(
        identifier: String,
        completionHandler: @escaping () -> Void
    ) {
        guard let sessionIdentifier, identifier == sessionIdentifier else {
            completionHandler()
            return
        }
        backgroundCompletionHandlers[identifier] = completionHandler
    }

    func handleBackgroundEventsFinished(for session: URLSession) {
        guard session.configuration.identifier == sessionIdentifier,
              let sessionIdentifier,
              let completion = backgroundCompletionHandlers.removeValue(
                forKey: sessionIdentifier
              )
        else { return }
        completion()
    }

    func emit(
        requestID: HostRequestId,
        sequence: UInt64,
        observation: HostObservation,
        identity: CoreDownloadTaskIdentity? = nil
    ) {
        if let delivery = deliveries[requestID] {
            delivery(sequence, observation)
            return
        }
        guard let identity else { return }
        let orphan = CoreDownloadOrphanObservation(
            identity: identity,
            sequenceNumber: sequence,
            observation: observation
        )
        if let orphanObservationSink {
            orphanObservationSink(orphan)
        } else {
            pendingOrphanObservations.append(orphan)
        }
    }

    func clearProgress(for episodeID: EpisodeId) {
        progress[episodeID] = nil
        expectedBytes[episodeID] = nil
        lastPublishedProgress[episodeID] = nil
        lastPublishedAt[episodeID] = nil
    }
}

extension HostObservation {
    var isDownloadResult: Bool {
        switch self {
        case .downloadAccepted, .downloadStaged, .downloadCancelled,
             .downloadArtifactRemoved:
            true
        default:
            false
        }
    }

    var downloadAttemptID: DownloadAttemptId? {
        switch self {
        case .downloadAccepted(_, _, let attemptID, _, _),
             .downloadStaged(_, _, let attemptID, _, _),
             .downloadCancelled(_, _, let attemptID):
            attemptID
        default:
            nil
        }
    }

    var downloadEpisodeID: EpisodeId? {
        switch self {
        case .downloadAccepted(let episodeID, _, _, _, _),
             .downloadStaged(let episodeID, _, _, _, _),
             .downloadCancelled(let episodeID, _, _),
             .downloadArtifactRemoved(let episodeID, _):
            episodeID
        default:
            nil
        }
    }
}
