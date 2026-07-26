import Foundation

/// Native opportunity adapter for Rust-owned durable workflows.
///
/// The app may announce platform lifecycle and input changes, but it does not
/// lease, execute, or mutate durable workflow rows. Rust remains the single
/// workflow owner and asks native hosts for bounded platform capabilities.
@MainActor
final class WorkflowRuntime {
    static let shared = WorkflowRuntime()

    private weak var appStore: AppStateStore?
    private weak var client: WorkflowClient?
    private var wakeTask: Task<Void, Never>?
    private var wakeRequested = false
    private init() {}

    func attach(store: AppStateStore) {
        guard appStore !== store else { return }
        appStore = store
        if let client { store.sharedLibrary?.attach(workflowClient: client) }
    }

    func attach(client: WorkflowClient) {
        self.client = client
        appStore?.sharedLibrary?.attach(workflowClient: client)
    }

    func startAndReconcile() async {
        await reconcile()
    }

    func reconcileOpportunity() async {
        await reconcile()
    }

    func requestTranscript(episodeID: UUID, provider: STTProvider? = nil) {
        appStore?.sharedLibrary?.requestTranscript(episodeID: episodeID, provider: provider)
    }

    func perform(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) async -> WorkflowJobActionResult {
        switch projection.authority {
        case .sharedRustPublisherChapters:
            return await appStore?.sharedLibrary?.performPublisherChapterAction(
                action,
                on: projection
            ) ?? .failed
        case .sharedRustModelChapters:
            return await appStore?.sharedLibrary?.performModelChapterAction(
                action,
                on: projection
            ) ?? .failed
        case .sharedRustDownloads:
            return await appStore?.sharedLibrary?.performDownloadAction(
                action,
                on: projection
            ) ?? .failed
        case .sharedRustTranscripts:
            return await appStore?.sharedLibrary?.performTranscriptAction(
                action,
                on: projection
            ) ?? .failed
        case .sharedRustScheduledAgents:
            return await appStore?.sharedLibrary?.performScheduledAgentAction(
                action,
                on: projection
            ) ?? .failed
        case .swiftJobStore:
            // Decode-only legacy rows are never actionable product work.
            return .notAllowed
        }
    }

    func wake() {
        guard appStore != nil else { return }
        wakeRequested = true
        guard wakeTask == nil else { return }
        wakeTask = Task { @MainActor [weak self] in
            guard let self else { return }
            repeat {
                wakeRequested = false
                await reconcile()
            } while wakeRequested && !Task.isCancelled
            wakeTask = nil
            if wakeRequested && !Task.isCancelled { wake() }
        }
    }

    private func reconcile() async {
        guard let store = appStore else { return }
        let episodes = store.state.episodes
        let settings = store.state.settings
        store.sharedLibrary?.ensurePublisherChapters(
            episodeIDs: episodes.map(\.id)
        )
        let transcriptStartPolicies = store.state.subscriptions.reduce(
            into: [UUID: TranscriptStartPolicy]()
        ) { policies, subscription in
            policies[subscription.podcastID] = subscription.transcriptStartPolicy
        }
        let transcriptOpportunities = await Task.detached(priority: .utility) {
            SharedLibraryClient.transcriptWorkflowOpportunities(
                episodes: episodes,
                settings: settings,
                startPolicies: transcriptStartPolicies
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
    }
}
