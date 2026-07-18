import Foundation
import Observation
import os.log

/// Temporary Swift application adapter for the durable workflow system.
/// Native views declare bounded interests, render projections, and dispatch
/// typed intents without opening workflow storage or the runtime singleton.
@MainActor
@Observable
final class WorkflowClient {
    typealias Loader = @Sendable (WorkflowProjectionQuery) async throws -> [WorkflowJobProjection]

    nonisolated private static let logger = Logger.app("WorkflowClient")
    private(set) var revision: UInt64 = 0
    private var jobsByKey: [WorkflowJobKey: WorkflowJobProjection] = [:]

    @ObservationIgnored private var registrations: [UUID: WorkflowProjectionRequest] = [:]
    @ObservationIgnored private var loader: Loader?
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

    func latest(kind: WorkJobKind, subjectID: UUID) -> WorkflowJobProjection? {
        jobsByKey[WorkflowJobKey(kind: kind, subjectID: subjectID)]
    }

    func jobs(kind: WorkJobKind) -> [WorkflowJobProjection] {
        jobsByKey.values
            .filter { $0.kind == kind }
            .sorted {
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
        guard let loader, let query = mergedQuery() else {
            replaceJobs([], generation: requestedGeneration)
            return
        }
        let delay = immediately ? 0 : coalescingDelayNanoseconds
        loadTask = Task { @MainActor [weak self] in
            do {
                if delay > 0 { try await Task.sleep(nanoseconds: delay) }
                let jobs = try await loader(query)
                guard !Task.isCancelled else { return }
                self?.replaceJobs(jobs, generation: requestedGeneration)
            } catch is CancellationError {
                return
            } catch {
                Self.logger.error("Unable to refresh workflow projection: \(error, privacy: .public)")
            }
        }
    }

    // MARK: - Typed workflow intents

    func configurePodcastDependencies(
        _ provider: @escaping @MainActor @Sendable () -> PodcastAgentToolDeps?
    ) {
        WorkflowRuntime.shared.podcastDepsProvider = provider
    }

    func startAndReconcile() async {
        WorkflowRuntime.shared.attach(client: self)
        await WorkflowRuntime.shared.startAndReconcile()
    }

    func reconcileAndDrain() async {
        await WorkflowRuntime.shared.reconcileAndDrain()
    }

    func wake() {
        WorkflowRuntime.shared.wake()
    }

    func requestTranscript(episodeID: UUID, provider: STTProvider? = nil) {
        WorkflowRuntime.shared.requestTranscript(episodeID: episodeID, provider: provider)
    }

    func dismissDownloadFailure(episodeID: UUID) {
        WorkflowRuntime.shared.dismissDownloadFailure(episodeID: episodeID)
    }

    private func receiveChange(at changedPath: String?) {
        guard let databaseURL,
              changedPath == databaseURL.standardizedFileURL.path else { return }
        refresh()
    }

    private func mergedQuery() -> WorkflowProjectionQuery? {
        var subjects: Set<UUID> = []
        var kinds: Set<WorkJobKind> = []
        var attentionKinds: Set<WorkJobKind> = []
        for request in registrations.values where !request.isEmpty {
            subjects.formUnion(request.subjectIDs)
            kinds.formUnion(request.kinds)
            attentionKinds.formUnion(request.attentionKinds)
        }
        guard (!subjects.isEmpty && !kinds.isEmpty) || !attentionKinds.isEmpty else { return nil }
        return WorkflowProjectionQuery(
            subjectIDs: subjects.sorted { $0.uuidString < $1.uuidString },
            kinds: kinds.sorted { $0.rawValue < $1.rawValue },
            attentionKinds: attentionKinds.sorted { $0.rawValue < $1.rawValue },
            limit: 1_000
        )
    }

    private func replaceJobs(_ jobs: [WorkflowJobProjection], generation: UInt64) {
        guard generation == self.generation else { return }
        let replacement = Dictionary(uniqueKeysWithValues: jobs.map { ($0.key, $0) })
        guard replacement != jobsByKey else { return }
        jobsByKey = replacement
        revision &+= 1
    }
}
