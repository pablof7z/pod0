import Foundation
import os.log

@MainActor
final class WorkflowRuntime {
    static let shared = WorkflowRuntime()
    private static let logger = Logger.app("WorkflowRuntime")

    private weak var appStore: AppStateStore?
    private(set) var jobStore: JobStore?
    private(set) var artifactRepository: ArtifactRepository?
    private var coordinator: WorkCoordinator?
    private lazy var persistenceObserver: NSObjectProtocol = NotificationCenter.default.addObserver(
        forName: .persistenceDidCommitWorkflowJobs,
        object: nil,
        queue: .main
    ) { [weak self] _ in
        MainActor.assumeIsolated { self?.wake() }
    }
    var podcastDepsProvider: @MainActor @Sendable () -> PodcastAgentToolDeps? = { nil }

    private init() {}

    func attach(store: AppStateStore) {
        _ = persistenceObserver
        guard appStore !== store else { return }
        appStore = store
        let databaseURL = store.persistence.episodeStore.fileURL
        let jobs = JobStore(fileURL: databaseURL)
        let artifacts = ArtifactRepository(fileURL: databaseURL)
        jobStore = jobs
        artifactRepository = artifacts

        let scheduled = ScheduledAgentRunJobExecutor(
            store: store,
            artifacts: artifacts
        ) { [weak self] in
            self?.podcastDepsProvider()
        }
        let executors: [WorkJobKind: any JobExecutor] = [
            .feedDiscovery: FeedDiscoveryJobExecutor(store: store, jobStore: jobs),
            .download: DownloadJobExecutor(store: store, jobStore: jobs),
            .transcriptIngest: TranscriptIngestJobExecutor(store: store, jobStore: jobs),
            .transcriptIndex: TranscriptIndexJobExecutor(),
            .publisherChapters: PublisherChaptersJobExecutor(),
            .chapterArtifacts: ChapterArtifactsJobExecutor(store: store),
            .metadataIndex: MetadataIndexJobExecutor(store: store),
            .autoDownload: AutoDownloadJobExecutor(store: store),
            .newEpisodeNotification: NewEpisodeNotificationJobExecutor(store: store),
            .scheduledAgentRun: scheduled,
        ]
        let verifier = WorkflowArtifactVerifier(appStore: store, artifacts: artifacts)
        let verifiers = Dictionary(
            uniqueKeysWithValues: executors.keys.map { ($0, verifier as any JobPostconditionVerifier) }
        )
        coordinator = WorkCoordinator(
            jobStore: jobs,
            executors: executors,
            verifiers: verifiers
        )
    }

    func startAndReconcile() async {
        guard let coordinator, let jobStore else { return }
        await EpisodeDownloadService.shared.reconcileBackgroundTransfers(jobStore: jobStore)
        await coordinator.start()
        await reconcile(signalOnly: true)
    }

    func reconcileAndDrain() async {
        guard let coordinator else { return }
        await coordinator.start()
        await reconcile(signalOnly: false)
    }

    func requestTranscript(episodeID: UUID, provider: STTProvider? = nil) {
        appStore?.setRequestedTranscriptProvider(episodeID, provider: provider ?? appStore?.state.settings.sttProvider)
        try? jobStore?.manuallyRetry(kind: .transcriptIngest, subjectID: episodeID)
        wake()
    }

    func requestDownload(
        episodeID: UUID,
        origin: DownloadIntentOrigin = .user
    ) {
        guard let episode = appStore?.episode(id: episodeID), let jobStore else { return }
        let inputVersion = DesiredStatePlanner.audioVersion(episode)
        let payload = DownloadJobPayload(
            origin: origin,
            enclosureURL: episode.enclosureURL,
            audioVersion: inputVersion
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let desired = DesiredJob(
            idempotencyKey: "download:\(episodeID):\(inputVersion):\(origin.rawValue)",
            kind: .download,
            subjectID: episodeID,
            inputVersion: inputVersion,
            occurrenceID: "download:\(episodeID):\(inputVersion):\(origin.rawValue)",
            payload: try? encoder.encode(payload),
            priority: origin.priority,
            resourceClass: .download,
            maxAttempts: 8
        )
        do {
            let inserted = try jobStore.ensureJob(desired)
            if !inserted {
                try jobStore.rearmJob(idempotencyKey: desired.idempotencyKey)
            }
            wake()
        } catch {
            Self.logger.error("Unable to persist download intent: \(error, privacy: .public)")
        }
    }

    func cancelDownload(episodeID: UUID) {
        do {
            try jobStore?.cancelActiveJobs(kind: .download, subjectID: episodeID)
            wake()
        } catch {
            Self.logger.error("Unable to cancel download intent: \(error, privacy: .public)")
        }
    }

    func latestJob(kind: WorkJobKind, subjectID: UUID) -> WorkJob? {
        guard let jobs = try? jobStore?.allJobs() else { return nil }
        return jobs.last { $0.kind == kind && $0.subjectID == subjectID }
    }

    func wake() {
        guard let coordinator else { return }
        Task { @MainActor [weak self] in
            guard let self else { return }
            await self.reconcile(signalOnly: true)
            await coordinator.signal()
        }
    }

    func dependencyChanged(for kind: WorkJobKind) {
        try? jobStore?.makeDependencyRetriesDue(kind: kind)
        wake()
    }

    func cancelActive() async {
        await coordinator?.cancelActive()
    }

    private func reconcile(signalOnly: Bool) async {
        guard let store = appStore, let jobStore, let artifactRepository, let coordinator else { return }
        do {
            let reconciler = Reconciler(
                appStore: store,
                jobStore: jobStore,
                artifacts: artifactRepository
            )
            _ = try reconciler.reconcile()
            try await reconciler.repairVectorSelections()
            if signalOnly { await coordinator.signal() }
            else { await coordinator.drainDueJobs() }
            repairCompletedScheduledOccurrences()
        } catch {
            Self.logger.error("Reconciliation failed: \(error, privacy: .public)")
        }
    }

    private func repairCompletedScheduledOccurrences() {
        guard let store = appStore, let jobStore,
              let jobs = try? jobStore.allJobs() else { return }
        store.advanceCompletedScheduledOccurrences(from: jobs)
    }
}
