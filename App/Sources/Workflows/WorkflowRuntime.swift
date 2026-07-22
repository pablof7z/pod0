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
    private weak var client: WorkflowClient?
    private lazy var persistenceObserver: NSObjectProtocol = NotificationCenter.default.addObserver(
        forName: .persistenceDidCommitWorkflowJobs,
        object: nil,
        queue: .main
    ) { [weak self] _ in
        MainActor.assumeIsolated {
            self?.client?.refresh()
            self?.wake()
        }
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
        client?.attach(jobStore: jobs)
        if let client { store.sharedLibrary?.attach(workflowClient: client) }

        let executors: [WorkJobKind: any JobExecutor] = [
            .feedDiscovery: FeedDiscoveryJobExecutor(store: store, jobStore: jobs),
            .metadataIndex: MetadataIndexJobExecutor(store: store),
            .newEpisodeNotification: NewEpisodeNotificationJobExecutor(store: store),
        ]
        let verifier = WorkflowArtifactVerifier(artifacts: artifacts)
        let verifiers = Dictionary(
            uniqueKeysWithValues: executors.keys.map { ($0, verifier as any JobPostconditionVerifier) }
        )
        coordinator = WorkCoordinator(
            jobStore: jobs,
            executors: executors,
            verifiers: verifiers
        )
    }

    func attach(client: WorkflowClient) {
        _ = persistenceObserver
        self.client = client
        if let jobStore { client.attach(jobStore: jobStore) }
        appStore?.sharedLibrary?.attach(workflowClient: client)
    }

    func startAndReconcile() async {
        guard let coordinator else { return }
        await coordinator.start()
        await reconcile(signalOnly: true)
    }

    func reconcileAndDrain() async {
        guard let coordinator else { return }
        await coordinator.start()
        await reconcile(signalOnly: false)
    }

    func requestTranscript(episodeID: UUID, provider: STTProvider? = nil) {
        appStore?.sharedLibrary?.requestTranscript(episodeID: episodeID, provider: provider)
    }

    func perform(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) -> WorkflowJobActionResult {
        if projection.authority == .sharedRustPublisherChapters {
            return appStore?.sharedLibrary?.performPublisherChapterAction(
                action,
                on: projection
            ) ?? .failed
        }
        if projection.authority == .sharedRustModelChapters {
            return appStore?.sharedLibrary?.performModelChapterAction(
                action,
                on: projection
            ) ?? .failed
        }
        if projection.authority == .sharedRustDownloads {
            return appStore?.sharedLibrary?.performDownloadAction(
                action,
                on: projection
            ) ?? .failed
        }
        if projection.authority == .sharedRustTranscripts {
            return appStore?.sharedLibrary?.performTranscriptAction(
                action,
                on: projection
            ) ?? .failed
        }
        if projection.authority == .sharedRustScheduledAgents {
            return appStore?.sharedLibrary?.performScheduledAgentAction(
                action,
                on: projection
            ) ?? .failed
        }
        guard let jobStore else { return .failed }
        do {
            let result = try jobStore.perform(
                action,
                jobID: projection.id,
                expectedUpdatedAt: projection.updatedAt
            )
            if case .accepted = result { wake() }
            return result
        } catch {
            Self.logger.error("Unable to perform workflow action: \(error, privacy: .public)")
            return .failed
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

    func cancelActive() async {
        await coordinator?.cancelActive()
    }

    private func reconcile(signalOnly: Bool) async {
        guard let store = appStore, let jobStore, let coordinator else { return }
        do {
            store.sharedLibrary?.ensurePublisherChapters(
                episodeIDs: store.state.episodes.map(\.id)
            )
            store.sharedLibrary?.ensureTranscriptWorkflows(
                episodes: store.state.episodes,
                settings: store.state.settings
            )
            let transcriptSnapshots = store.sharedLibrary?.transcriptWorkflowSnapshots(
                episodeIDs: store.state.episodes.map(\.id)
            ) ?? []
            store.sharedLibrary?.ensureModelChapters(
                transcripts: transcriptSnapshots,
                configuredModel: store.state.settings.chapterCompilationModel
            )
            store.sharedLibrary?.reconcileScheduledAgents()
            let reconciler = Reconciler(appStore: store, jobStore: jobStore)
            _ = try reconciler.reconcile()
            if signalOnly { await coordinator.signal() }
            else { await coordinator.drainDueJobs() }
        } catch {
            Self.logger.error("Reconciliation failed: \(error, privacy: .public)")
        }
    }

}
