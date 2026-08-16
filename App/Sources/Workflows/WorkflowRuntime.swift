import Foundation
import Pod0Core

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
        await reconcile(reason: .launch)
    }

    func reconcileOpportunity() async {
        await reconcile(reason: .libraryChanged)
    }

    func requestTranscript(episodeID: UUID, provider: STTProvider? = nil) {
        appStore?.sharedLibrary?.requestTranscript(episodeID: episodeID, provider: provider)
    }

    func perform(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) async -> WorkflowJobActionResult {
        guard let token = projection.token(for: action),
              let shared = appStore?.sharedLibrary else { return .notAllowed }
        return shared.executeWorkflowAction(token).swiftValue(for: action)
    }

    func wake() {
        announceCapabilityChange(reason: .libraryChanged)
    }

    func announceCredentialAvailabilityChanged() {
        announceCapabilityChange(reason: .credentialChanged)
    }

    private func announceCapabilityChange(reason: WorkflowOpportunityReason) {
        Task { @MainActor [weak self] in await self?.reconcile(reason: reason) }
    }

    private func reconcile(reason: WorkflowOpportunityReason) async {
        guard let store = appStore, let shared = store.sharedLibrary else { return }
        let settings = store.state.settings
        let configuration = WorkflowConfigurationInput(
            transcriptProvider: settings.sttProvider.coreValue,
            elevenLabsModel: settings.elevenLabsSTTModel,
            assemblyAiModel: settings.assemblyAISTTModel,
            openRouterModel: settings.openRouterWhisperModel,
            autoPublisherTranscripts: settings.autoIngestPublisherTranscripts,
            autoProviderTranscripts: settings.autoFallbackToScribe,
            chapterModel: settings.chapterCompilationModel
        )
        let current: WorkflowConfiguration?
        do {
            current = try shared.workflowConfiguration()
        } catch {
            return
        }
        if current == nil {
            do {
                _ = try await shared.executeCommitted(.importLegacyWorkflowConfiguration(
                    configuration: configuration,
                    sourceGeneration: ContentDigest(word0: 41, word1: 1, word2: 0, word3: 0)
                ))
            } catch {
                return
            }
        }
        guard let authoritative = try? shared.workflowConfiguration() else { return }
        if authoritative.value != configuration {
            do {
                _ = try await shared.executeCommitted(.setWorkflowConfiguration(
                    expectedConfigurationRevision: authoritative.revision,
                    configuration: configuration
                ))
            } catch {
                return
            }
        }
        let capabilities = WorkflowCapabilitySnapshotInput(
            credentials: TranscriptCredentialCapabilities(
                elevenLabs: ElevenLabsCredentialStore.hasAPIKey(),
                assemblyAi: AssemblyAICredentialStore.hasAPIKey(),
                openRouter: OpenRouterCredentialStore.hasAPIKey(),
                appleSpeech: true
            ),
            localAudio: store.state.episodes.compactMap { episode in
                guard let url = episode.downloadState.localFileURL,
                      FileManager.default.fileExists(atPath: url.path) else { return nil }
                return LocalAudioCapability(
                    episodeId: EpisodeId(uuid: episode.id),
                    localAudioUrl: url.absoluteString
                )
            }
        )
        let observedAt = UnixTimestampMilliseconds(date: Date())
        guard let snapshot = makeWorkflowCapabilitySnapshot(
            input: capabilities,
            observedAt: observedAt
        ) else { return }
        _ = try? await shared.executeCommitted(.observeWorkflowCapabilities(
            capabilities: capabilities
        ))
        _ = try? await shared.executeCommitted(.reconcileWorkflowOpportunity(
            opportunity: WorkflowOpportunity(
                reason: reason,
                observedAt: observedAt,
                capabilitySnapshotId: snapshot.snapshotId
            )
        ))
    }
}

private extension WorkflowActionDispatchResult {
    func swiftValue(for action: WorkflowJobAction) -> WorkflowJobActionResult {
        switch self {
        case .accepted: .accepted(action)
        case .stale: .stale
        case .notAllowed, .invalidToken: .notAllowed
        case .notFound: .notFound
        case .storageUnavailable: .failed
        }
    }
}
