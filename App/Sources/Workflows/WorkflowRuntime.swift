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
    private var wakeTask: Task<Void, Never>?
    private var wakeRequested = false
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
            .metadataIndex: MetadataIndexJobExecutor(store: store),
        ]
        coordinator = WorkCoordinator(
            jobStore: jobs,
            executors: executors
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
        guard coordinator != nil else { return }
        wakeRequested = true
        guard wakeTask == nil else { return }
        wakeTask = Task { @MainActor [weak self] in
            guard let self else { return }
            repeat {
                wakeRequested = false
                await reconcile(signalOnly: true)
            } while wakeRequested && !Task.isCancelled
            wakeTask = nil
            if wakeRequested && !Task.isCancelled { wake() }
        }
    }

    func cancelActive() async {
        await coordinator?.cancelActive()
    }

    private func reconcile(signalOnly: Bool) async {
        guard let store = appStore, let coordinator else { return }
        let episodes = store.state.episodes
        let settings = store.state.settings
        store.sharedLibrary?.ensurePublisherChapters(
            episodeIDs: episodes.map(\.id)
        )
        let transcriptOpportunities = await Task.detached(priority: .utility) {
            SharedLibraryClient.transcriptWorkflowOpportunities(
                episodes: episodes,
                settings: settings
            )
        }.value
        store.sharedLibrary?.ensureTranscriptWorkflows(transcriptOpportunities)
        let transcriptSnapshots: [TranscriptWorkflowSnapshot]
        if let sharedLibrary = store.sharedLibrary {
            let facade = sharedLibrary.facade
            let episodeIDs = episodes.compactMap { episode -> UUID? in
                guard case .ready = episode.transcriptState else { return nil }
                return episode.id
            }
            transcriptSnapshots = await Task.detached(priority: .utility) {
                SharedLibraryClient.transcriptWorkflowSnapshots(
                    facade: facade,
                    episodeIDs: episodeIDs
                )
            }.value
        } else {
            transcriptSnapshots = []
        }
        store.sharedLibrary?.ensureModelChapters(
            transcripts: transcriptSnapshots,
            configuredModel: settings.chapterCompilationModel
        )
        store.sharedLibrary?.reconcileScheduledAgents()
        if signalOnly { await coordinator.signal() }
        else { await coordinator.drainDueJobs() }
    }

}
