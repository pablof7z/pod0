import Foundation
import Observation
import Pod0Core
import os.log

/// Temporary Swift application adapter for the durable workflow system.
/// Native views declare bounded interests, render projections, and dispatch
/// typed intents without opening workflow storage or the runtime singleton.
@MainActor
@Observable
final class WorkflowClient {
    typealias Loader = @Sendable (WorkflowProjectionQuery) async throws -> [WorkflowJobProjection]
    typealias PublisherLoader = @Sendable (WorkflowProjectionQuery) async
        -> [PublisherChapterWorkflowProjection]
    typealias ModelChapterLoader = @Sendable (WorkflowProjectionQuery) async
        -> [ModelChapterWorkflowProjection]
    typealias DownloadLoader = @Sendable (WorkflowProjectionQuery) async
        -> [DownloadWorkflowProjection]
    typealias TranscriptLoader = @Sendable (WorkflowProjectionQuery) async
        -> [TranscriptWorkflowProjection]

    nonisolated private static let logger = Logger.app("WorkflowClient")
    private(set) var revision: UInt64 = 0
    private var jobsByID: [UUID: WorkflowJobProjection] = [:]
    private var swiftJobsByID: [UUID: WorkflowJobProjection] = [:]
    private var corePublisherJobsByID: [UUID: WorkflowJobProjection] = [:]
    private var coreModelChapterJobsByID: [UUID: WorkflowJobProjection] = [:]
    private var coreDownloadJobsByID: [UUID: WorkflowJobProjection] = [:]
    private var coreTranscriptJobsByID: [UUID: WorkflowJobProjection] = [:]
    private var latestByKey: [WorkflowJobKey: WorkflowJobProjection] = [:]

    @ObservationIgnored private var registrations: [UUID: WorkflowProjectionRequest] = [:]
    @ObservationIgnored private var loader: Loader?
    @ObservationIgnored private var publisherLoader: PublisherLoader?
    @ObservationIgnored private var modelChapterLoader: ModelChapterLoader?
    @ObservationIgnored private var downloadLoader: DownloadLoader?
    @ObservationIgnored private var transcriptLoader: TranscriptLoader?
    @ObservationIgnored private var databaseURL: URL?
    @ObservationIgnored private var loadTask: Task<Void, Never>?
    @ObservationIgnored private var generation: UInt64 = 0
    @ObservationIgnored private var changeObserver: NSObjectProtocol?
    @ObservationIgnored private let coalescingDelayNanoseconds: UInt64

    init(
        loader: Loader? = nil,
        coalescingDelayNanoseconds: UInt64 = 40_000_000
    ) {
        self.loader = loader
        self.coalescingDelayNanoseconds = coalescingDelayNanoseconds
        changeObserver = NotificationCenter.default.addObserver(
            forName: .workflowJobStoreDidChange,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            let changedPath = (notification.object as? NSString).map(String.init)
            MainActor.assumeIsolated { self?.receiveChange(at: changedPath) }
        }
    }

    func attach(jobStore: JobStore) {
        guard databaseURL?.standardizedFileURL != jobStore.fileURL.standardizedFileURL else {
            refresh()
            return
        }
        databaseURL = jobStore.fileURL
        loader = { query in
            try await Task.detached(priority: .userInitiated) {
                try jobStore.projections(for: query)
            }.value
        }
        refresh()
    }

    func attachPublisherChapterCore(loader: @escaping PublisherLoader) {
        publisherLoader = loader
        refresh()
    }

    func detachPublisherChapterCore() {
        publisherLoader = nil
        refresh(immediately: true)
    }

    func attachModelChapterCore(loader: @escaping ModelChapterLoader) {
        modelChapterLoader = loader
        refresh()
    }

    func detachModelChapterCore() {
        modelChapterLoader = nil
        refresh(immediately: true)
    }

    func attachDownloadCore(loader: @escaping DownloadLoader) {
        downloadLoader = loader
        refresh()
    }

    func detachDownloadCore() {
        downloadLoader = nil
        refresh(immediately: true)
    }

    func attachTranscriptCore(loader: @escaping TranscriptLoader) {
        transcriptLoader = loader
        refresh()
    }

    func detachTranscriptCore() {
        transcriptLoader = nil
        refresh(immediately: true)
    }

    func latest(kind: WorkflowProjectionKind, subjectID: UUID) -> WorkflowJobProjection? {
        latestByKey[WorkflowJobKey(kind: kind, subjectID: subjectID)]
    }

    func jobs(kind: WorkflowProjectionKind) -> [WorkflowJobProjection] {
        jobsByID.values
            .filter { $0.kind == kind }
            .sorted {
                if $0.updatedAt != $1.updatedAt { return $0.updatedAt > $1.updatedAt }
                return $0.id.uuidString > $1.id.uuidString
            }
    }

    func allJobs() -> [WorkflowJobProjection] {
        jobsByID.values.sorted {
            if $0.updatedAt != $1.updatedAt { return $0.updatedAt > $1.updatedAt }
            return $0.id.uuidString > $1.id.uuidString
        }
    }

    @discardableResult
    func register(_ request: WorkflowProjectionRequest) -> UUID {
        let token = UUID()
        registrations[token] = request
        refresh()
        return token
    }

    func updateRegistration(_ token: UUID, request: WorkflowProjectionRequest) {
        guard registrations[token] != request else { return }
        registrations[token] = request
        refresh()
    }

    func unregister(_ token: UUID) {
        guard registrations.removeValue(forKey: token) != nil else { return }
        refresh()
    }

    func refresh(immediately: Bool = false) {
        generation &+= 1
        let requestedGeneration = generation
        loadTask?.cancel()
        guard let query = mergedQuery() else {
            replaceJobs(
                [],
                publisherWorkflows: [],
                modelChapterWorkflows: [],
                generation: requestedGeneration
            )
            return
        }
        let loader = loader
        let publisherLoader = publisherLoader
        let modelChapterLoader = modelChapterLoader
        let downloadLoader = downloadLoader
        let transcriptLoader = transcriptLoader
        guard loader != nil || publisherLoader != nil || modelChapterLoader != nil
                || downloadLoader != nil || transcriptLoader != nil else {
            replaceJobs(
                [],
                publisherWorkflows: [],
                modelChapterWorkflows: [],
                generation: requestedGeneration
            )
            return
        }
        let delay = immediately ? 0 : coalescingDelayNanoseconds
        loadTask = Task { @MainActor [weak self] in
            do {
                if delay > 0 { try await Task.sleep(nanoseconds: delay) }
                let jobs = try await loader?(query) ?? []
                let publisherWorkflows = await publisherLoader?(query) ?? []
                let modelChapterWorkflows = await modelChapterLoader?(query) ?? []
                let downloadWorkflows = await downloadLoader?(query) ?? []
                let transcriptWorkflows = await transcriptLoader?(query) ?? []
                guard !Task.isCancelled else { return }
                self?.replaceJobs(
                    jobs,
                    publisherWorkflows: publisherWorkflows,
                    modelChapterWorkflows: modelChapterWorkflows,
                    downloadWorkflows: downloadWorkflows,
                    transcriptWorkflows: transcriptWorkflows,
                    generation: requestedGeneration
                )
            } catch is CancellationError {
                return
            } catch {
                Self.logger.error("Unable to refresh workflow projection: \(error, privacy: .public)")
            }
        }
    }

    private func receiveChange(at changedPath: String?) {
        guard let databaseURL,
              changedPath == databaseURL.standardizedFileURL.path else { return }
        refresh()
    }

    private func mergedQuery() -> WorkflowProjectionQuery? {
        var subjects: Set<UUID> = []
        var kinds: Set<WorkflowProjectionKind> = []
        var attentionKinds: Set<WorkflowProjectionKind> = []
        var recentKinds: Set<WorkflowProjectionKind> = []
        for request in registrations.values where !request.isEmpty {
            subjects.formUnion(request.subjectIDs)
            kinds.formUnion(request.kinds)
            attentionKinds.formUnion(request.attentionKinds)
            recentKinds.formUnion(request.recentKinds)
        }
        guard (!subjects.isEmpty && !kinds.isEmpty)
                || !attentionKinds.isEmpty || !recentKinds.isEmpty else { return nil }
        return WorkflowProjectionQuery(
            subjectIDs: subjects.sorted { $0.uuidString < $1.uuidString },
            kinds: kinds.sorted { $0.rawValue < $1.rawValue },
            attentionKinds: attentionKinds.sorted { $0.rawValue < $1.rawValue },
            recentKinds: recentKinds.sorted { $0.rawValue < $1.rawValue },
            limit: 1_000
        )
    }

    private func replaceJobs(
        _ jobs: [WorkflowJobProjection],
        publisherWorkflows: [PublisherChapterWorkflowProjection],
        modelChapterWorkflows: [ModelChapterWorkflowProjection],
        downloadWorkflows: [DownloadWorkflowProjection] = [],
        transcriptWorkflows: [TranscriptWorkflowProjection] = [],
        generation: UInt64
    ) {
        guard generation == self.generation else { return }
        swiftJobsByID = Dictionary(uniqueKeysWithValues: jobs.map { ($0.id, $0) })
        corePublisherJobsByID = Dictionary(uniqueKeysWithValues: publisherWorkflows.map {
            let projection = WorkflowJobProjection(publisherChapterWorkflow: $0)
            return (projection.id, projection)
        })
        coreModelChapterJobsByID = Dictionary(uniqueKeysWithValues: modelChapterWorkflows.map {
            let projection = WorkflowJobProjection(modelChapterWorkflow: $0)
            return (projection.id, projection)
        })
        coreDownloadJobsByID = Dictionary(uniqueKeysWithValues: downloadWorkflows.map {
            let projection = WorkflowJobProjection(downloadWorkflow: $0)
            return (projection.id, projection)
        })
        coreTranscriptJobsByID = Dictionary(uniqueKeysWithValues: transcriptWorkflows.map {
            let projection = WorkflowJobProjection(transcriptWorkflow: $0)
            return (projection.id, projection)
        })
        mergeJobs()
    }

    private func mergeJobs() {
        let chapterJobs = corePublisherJobsByID.merging(coreModelChapterJobsByID) { _, model in model }
        let coreJobs = chapterJobs.merging(coreDownloadJobsByID) { _, download in download }
            .merging(coreTranscriptJobsByID) { _, transcript in transcript }
        let replacement = swiftJobsByID.merging(coreJobs) { _, core in core }
        guard replacement != jobsByID else { return }
        jobsByID = replacement
        var latest: [WorkflowJobKey: WorkflowJobProjection] = [:]
        for job in replacement.values.sorted(by: { $0.updatedAt > $1.updatedAt })
            where latest[job.key] == nil {
            latest[job.key] = job
        }
        latestByKey = latest
        revision &+= 1
    }
}
