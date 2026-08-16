import Foundation
import Observation
import Pod0Core

/// Temporary native adapter for bounded Rust workflow projections.
/// Durable workflow policy remains in Rust; #210 removes this exception once
/// reconciliation and action routing are typed shared-core contracts.
@MainActor
@Observable
final class WorkflowClient {
    typealias PublisherLoader = @Sendable (WorkflowProjectionQuery) async
        -> [PublisherChapterWorkflowProjection]
    typealias ModelChapterLoader = @Sendable (WorkflowProjectionQuery) async
        -> [ModelChapterWorkflowProjection]
    typealias DownloadLoader = @Sendable (WorkflowProjectionQuery) async
        -> [DownloadWorkflowProjection]
    typealias TranscriptLoader = @Sendable (WorkflowProjectionQuery) async
        -> [TranscriptWorkflowProjection]

    private(set) var revision: UInt64 = 0
    private var jobsByID: [UUID: WorkflowJobProjection] = [:]
    private var corePublisherJobsByID: [UUID: WorkflowJobProjection] = [:]
    private var coreModelChapterJobsByID: [UUID: WorkflowJobProjection] = [:]
    private var coreDownloadJobsByID: [UUID: WorkflowJobProjection] = [:]
    private var coreTranscriptJobsByID: [UUID: WorkflowJobProjection] = [:]
    private var latestByKey: [WorkflowJobKey: WorkflowJobProjection] = [:]

    @ObservationIgnored private var registrations: [UUID: WorkflowProjectionRequest] = [:]
    @ObservationIgnored private var publisherLoader: PublisherLoader?
    @ObservationIgnored private var modelChapterLoader: ModelChapterLoader?
    @ObservationIgnored private var downloadLoader: DownloadLoader?
    @ObservationIgnored private var transcriptLoader: TranscriptLoader?
    @ObservationIgnored private var loadTask: Task<Void, Never>?
    @ObservationIgnored private var generation: UInt64 = 0
    @ObservationIgnored private let coalescingDelayNanoseconds: UInt64

    init(coalescingDelayNanoseconds: UInt64 = 40_000_000) {
        self.coalescingDelayNanoseconds = coalescingDelayNanoseconds
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
            .sorted(by: Self.newestFirst)
    }

    func allJobs() -> [WorkflowJobProjection] {
        jobsByID.values.sorted(by: Self.newestFirst)
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
        guard let query = mergedQuery(), hasLoader else {
            replaceJobs(
                publisher: [], modelChapters: [], downloads: [], transcripts: [],
                generation: requestedGeneration
            )
            return
        }
        let publisherLoader = publisherLoader
        let modelChapterLoader = modelChapterLoader
        let downloadLoader = downloadLoader
        let transcriptLoader = transcriptLoader
        let delay = immediately ? 0 : coalescingDelayNanoseconds
        loadTask = Task { @MainActor [weak self] in
            if delay > 0 {
                do { try await Task.sleep(nanoseconds: delay) } catch { return }
            }
            let publisher = await publisherLoader?(query) ?? []
            let modelChapters = await modelChapterLoader?(query) ?? []
            let downloads = await downloadLoader?(query) ?? []
            let transcripts = await transcriptLoader?(query) ?? []
            guard !Task.isCancelled else { return }
            self?.replaceJobs(
                publisher: publisher.map(WorkflowJobProjection.init),
                modelChapters: modelChapters.map(WorkflowJobProjection.init),
                downloads: downloads.map(WorkflowJobProjection.init),
                transcripts: transcripts.map(WorkflowJobProjection.init),
                generation: requestedGeneration
            )
        }
    }

    private var hasLoader: Bool {
        publisherLoader != nil || modelChapterLoader != nil
            || downloadLoader != nil || transcriptLoader != nil
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

    private static func newestFirst(
        _ lhs: WorkflowJobProjection,
        _ rhs: WorkflowJobProjection
    ) -> Bool {
        if lhs.updatedAt != rhs.updatedAt { return lhs.updatedAt > rhs.updatedAt }
        return lhs.id.uuidString > rhs.id.uuidString
    }

    func replaceJobs(
        publisher: [WorkflowJobProjection],
        modelChapters: [WorkflowJobProjection],
        downloads: [WorkflowJobProjection],
        transcripts: [WorkflowJobProjection],
        generation: UInt64
    ) {
        guard generation == self.generation else { return }
        corePublisherJobsByID = Dictionary(uniqueKeysWithValues: publisher.map { ($0.id, $0) })
        coreModelChapterJobsByID = Dictionary(
            uniqueKeysWithValues: modelChapters.map { ($0.id, $0) }
        )
        coreDownloadJobsByID = Dictionary(uniqueKeysWithValues: downloads.map { ($0.id, $0) })
        coreTranscriptJobsByID = Dictionary(
            uniqueKeysWithValues: transcripts.map { ($0.id, $0) }
        )
        mergeJobs()
    }

    func mergeJobs() {
        let chapterJobs = corePublisherJobsByID.merging(coreModelChapterJobsByID) { _, rhs in rhs }
        let replacement = chapterJobs
            .merging(coreDownloadJobsByID) { _, rhs in rhs }
            .merging(coreTranscriptJobsByID) { _, rhs in rhs }
        guard replacement != jobsByID else { return }
        jobsByID = replacement
        var latest: [WorkflowJobKey: WorkflowJobProjection] = [:]
        for job in replacement.values.sorted(by: Self.newestFirst) where latest[job.key] == nil {
            latest[job.key] = job
        }
        latestByKey = latest
        revision &+= 1
    }
}
