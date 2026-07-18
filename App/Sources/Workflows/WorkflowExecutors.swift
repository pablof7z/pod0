import Foundation

@MainActor
final class FeedDiscoveryJobExecutor: JobExecutor {
    private let store: AppStateStore
    private let jobStore: JobStore

    init(store: AppStateStore, jobStore: JobStore) {
        self.store = store
        self.jobStore = jobStore
    }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        let payload = try decode(FeedDiscoveryPayload.self, from: context.job)
        let available = payload.episodes.filter { input in
            guard let episode = store.episode(id: input.episodeID) else { return false }
            return DesiredStatePlanner.audioVersion(episode) == input.inputVersion
        }
        let sorted = available.sorted {
            if $0.pubDate != $1.pubDate { return $0.pubDate > $1.pubDate }
            return $0.episodeID.uuidString < $1.episodeID.uuidString
        }

        if payload.autoDownloadPolicy != nil {
            let current = store.effectiveAutoDownload(forPodcast: payload.podcastID)
            let selected: [FeedDiscoveryPayload.EpisodeInput]
            switch current.mode {
            case .off: selected = []
            case .latestN(let count): selected = Array(sorted.prefix(max(0, count)))
            case .allNew: selected = sorted
            }
            for input in selected {
                let occurrence = "autodownload:\(payload.occurrenceID):\(input.episodeID.uuidString)"
                let child = AutoDownloadJobPayload(
                    discoveryOccurrenceID: payload.occurrenceID,
                    policyVersion: payload.policyVersion
                )
                _ = try jobStore.ensureJob(DesiredJob(
                    idempotencyKey: occurrence,
                    kind: .autoDownload,
                    subjectID: input.episodeID,
                    inputVersion: input.inputVersion,
                    occurrenceID: occurrence,
                    payload: try workflowEncoder.encode(child),
                    priority: 20,
                    resourceClass: .planning
                ))
            }
        }

        if payload.notificationsEnabled,
           store.state.settings.notifyOnNewEpisodes,
           store.subscription(podcastID: payload.podcastID)?.notificationsEnabled == true {
            for input in sorted.prefix(NotificationService.maxNewEpisodeNotificationsPerRefresh) {
                let occurrence = "notification:\(payload.occurrenceID):\(input.episodeID.uuidString)"
                let child = NotificationJobPayload(
                    discoveredAt: payload.discoveredAt,
                    podcastID: payload.podcastID,
                    episodeTitle: input.title
                )
                _ = try jobStore.ensureJob(DesiredJob(
                    idempotencyKey: occurrence,
                    kind: .newEpisodeNotification,
                    subjectID: input.episodeID,
                    inputVersion: input.inputVersion,
                    occurrenceID: occurrence,
                    payload: try workflowEncoder.encode(child),
                    priority: 30,
                    resourceClass: .notification,
                    maxAttempts: 4
                ))
            }
        }
        return .succeeded(outputVersion: payload.occurrenceID)
    }
}

@MainActor
final class DownloadJobExecutor: JobExecutor {
    private let store: AppStateStore
    private let jobStore: JobStore

    init(store: AppStateStore, jobStore: JobStore) {
        self.store = store
        self.jobStore = jobStore
    }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        guard let episode = store.episode(id: context.job.subjectID) else { return .obsolete }
        let payload = try decode(DownloadJobPayload.self, from: context.job)
        guard payload.audioVersion == context.job.inputVersion,
              payload.enclosureURL == episode.enclosureURL,
              DesiredStatePlanner.audioVersion(episode) == context.job.inputVersion else {
            return .obsolete
        }
        let admission = DownloadAdmissionPolicy().evaluate(
            origin: payload.origin,
            automaticPolicy: store.effectiveAutoDownload(forPodcast: episode.podcastID),
            network: EpisodeDownloadService.shared.networkStatus,
            availableStorageCapacity: EpisodeDownloadService.shared.availableStorageCapacity
        )
        switch admission {
        case .obsolete:
            return .obsolete
        case .wait(let reason):
            return .retry(
                notBefore: Date().addingTimeInterval(5 * 60),
                error: JobFailure(classification: .missingDependency, message: reason)
            )
        case .admit:
            break
        }
        let output = try await EpisodeDownloadService.shared.startAdmittedDownload(
            context: context,
            jobStore: jobStore
        )
        return .succeeded(outputVersion: output)
    }
}

@MainActor
final class TranscriptIngestJobExecutor: JobExecutor {
    private let store: AppStateStore
    private let jobStore: JobStore

    init(store: AppStateStore, jobStore: JobStore) {
        self.store = store
        self.jobStore = jobStore
    }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        let job = context.job
        guard store.episode(id: job.subjectID) != nil else { return .obsolete }
        let payload = try decode(TranscriptJobPayload.self, from: job)
        guard hasCredential(payload.provider) else {
            return .blocked(reason: JobFailure(
                classification: .missingCredential,
                message: "No credential is configured for \(payload.provider.displayName)."
            ))
        }
        let output = try await TranscriptIngestService.shared.executeJob(
            context: context,
            payload: payload,
            jobStore: jobStore
        )
        return .succeeded(outputVersion: output)
    }

    private func hasCredential(_ provider: STTProvider) -> Bool {
        switch provider {
        case .elevenLabsScribe: TranscriptIngestService.shared.resolvedElevenLabsKey() != nil
        case .openRouterWhisper: TranscriptIngestService.shared.resolvedOpenRouterKey() != nil
        case .assemblyAI: TranscriptIngestService.shared.resolvedAssemblyAIKey() != nil
        case .appleNative: true
        }
    }
}

@MainActor
final class TranscriptIndexJobExecutor: JobExecutor {
    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        let receipt = try await TranscriptIngestService.shared.indexTranscript(
            episodeID: context.job.subjectID,
            generation: context.job.inputVersion
        )
        let data = try workflowEncoder.encode(receipt)
        return .succeeded(outputVersion: data.base64EncodedString())
    }
}

final class PublisherChaptersJobExecutor: JobExecutor {
    private let client: ChaptersClient

    init(client: ChaptersClient = ChaptersClient()) {
        self.client = client
    }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        let payload = try decode(PublisherChaptersJobPayload.self, from: context.job)
        guard payload.sourceVersion == context.job.inputVersion else {
            return .failedPermanent(JobFailure(
                classification: .invalidInput,
                message: "Publisher chapter payload version does not match its job."
            ))
        }
        do {
            let chapters = try await client.fetch(url: payload.url)
            let output = ChapterCompilationOutput(
                chapters: chapters,
                ads: [],
                chapterOrigin: .publisher
            )
            let manifest = try DerivedArtifactStagingStore.shared.stageChapters(
                output,
                episodeID: context.job.subjectID,
                inputVersion: context.job.inputVersion,
                leaseToken: context.leaseToken
            )
            return .succeeded(outputVersion: manifest)
        } catch let error as ChaptersClient.FetchError {
            switch error {
            case .http(let status) where status == 404 || status == 410:
                return .failedPermanent(JobFailure(
                    classification: .invalidInput,
                    message: "Publisher chapter document is unavailable (HTTP \(status))."
                ))
            default:
                throw JobFailure(classification: .transient, message: String(describing: error))
            }
        }
    }
}

@MainActor
final class ChapterArtifactsJobExecutor: JobExecutor {
    private let store: AppStateStore
    init(store: AppStateStore) { self.store = store }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        guard store.episode(id: context.job.subjectID) != nil else { return .obsolete }
        do {
            let output = try await AIChapterCompiler.shared.compile(
                episodeID: context.job.subjectID,
                store: store
            )
            let manifestHash = try DerivedArtifactStagingStore.shared.stageChapters(
                output,
                episodeID: context.job.subjectID,
                inputVersion: context.job.inputVersion,
                leaseToken: context.leaseToken
            )
            return .succeeded(outputVersion: manifestHash)
        } catch { throw JobFailure.classify(error) }
    }
}

@MainActor
final class MetadataIndexJobExecutor: JobExecutor {
    private let store: AppStateStore
    init(store: AppStateStore) { self.store = store }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        guard store.episode(id: context.job.subjectID) != nil else { return .obsolete }
        let receipt = try await EpisodeMetadataIndexer.shared.indexEpisode(
            id: context.job.subjectID,
            appStore: store,
            generation: context.job.inputVersion
        )
        return .succeeded(outputVersion: try workflowEncoder.encode(receipt).base64EncodedString())
    }
}

@MainActor
final class AutoDownloadJobExecutor: JobExecutor {
    private let store: AppStateStore
    private let persistDownload: @MainActor @Sendable (
        UUID,
        DownloadIntentOrigin
    ) throws -> WorkJob

    init(
        store: AppStateStore,
        persistDownload: @escaping @MainActor @Sendable (
            UUID,
            DownloadIntentOrigin
        ) throws -> WorkJob = { episodeID, origin in
            try WorkflowRuntime.shared.persistDownloadIntent(
                episodeID: episodeID,
                origin: origin
            )
        }
    ) {
        self.store = store
        self.persistDownload = persistDownload
    }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        guard let episode = store.episode(id: context.job.subjectID) else { return .obsolete }
        EpisodeDownloadService.shared.attach(appStore: store)
        let policy = store.effectiveAutoDownload(forPodcast: episode.podcastID)
        if case .off = policy.mode { return .obsolete }
        if policy.wifiOnly, !EpisodeDownloadService.shared.isOnWiFi {
            return .retry(
                notBefore: Date().addingTimeInterval(15 * 60),
                error: JobFailure(
                    classification: .missingDependency,
                    message: "Automatic download is waiting for Wi-Fi."
                )
            )
        }
        _ = try persistDownload(episode.id, .autoDownload)
        return .succeeded(outputVersion: context.job.inputVersion)
    }
}

@MainActor
final class NewEpisodeNotificationJobExecutor: JobExecutor {
    private let store: AppStateStore
    init(store: AppStateStore) { self.store = store }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        let payload = try decode(NotificationJobPayload.self, from: context.job)
        guard Date().timeIntervalSince(payload.discoveredAt) <= 24 * 60 * 60 else { return .obsolete }
        guard let episode = store.episode(id: context.job.subjectID),
              let podcast = store.podcast(id: episode.podcastID) else { return .obsolete }
        guard store.state.settings.notifyOnNewEpisodes else { return .obsolete }
        guard store.subscription(podcastID: episode.podcastID)?.notificationsEnabled == true else {
            return .obsolete
        }
        guard await NotificationService.notifyNewEpisodes(
            [episode], podcast: podcast, occurrenceID: context.job.occurrenceID
        ) else {
            return .obsolete
        }
        return .succeeded(outputVersion: context.job.occurrenceID)
    }
}

@MainActor
final class ScheduledAgentRunJobExecutor: JobExecutor {
    private let store: AppStateStore
    private let artifacts: ArtifactRepository
    private let history: ChatHistoryStore
    private let deps: @MainActor @Sendable () -> PodcastAgentToolDeps?

    init(
        store: AppStateStore,
        artifacts: ArtifactRepository,
        history: ChatHistoryStore = .shared,
        deps: @escaping @MainActor @Sendable () -> PodcastAgentToolDeps?
    ) {
        self.store = store
        self.artifacts = artifacts
        self.history = history
        self.deps = deps
    }

    func run(_ context: JobAttemptContext) async throws -> JobOutcome {
        let payload = try decode(ScheduledRunPayload.self, from: context.job)
        guard store.scheduledTasks.contains(where: { $0.id == payload.taskID }) else { return .obsolete }
        if try artifacts.current(kind: .scheduledOutput, subjectID: payload.taskID)?.outputVersion
            == context.job.occurrenceID {
            return .succeeded(outputVersion: context.job.occurrenceID)
        }
        if let occurrenceID = context.job.occurrenceID,
           history.conversation(occurrenceID: occurrenceID)?.hasCompletedScheduledOutput == true {
            return .succeeded(outputVersion: occurrenceID)
        }
        let reference = LLMModelReference(storedID: payload.modelID)
        guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
            return .blocked(reason: JobFailure(
                classification: .missingCredential,
                message: "No agent API key is configured."
            ))
        }
        let session = AgentChatSession(
            store: store,
            podcastDeps: deps(),
            history: history,
            resumeWindow: 0,
            drainPendingContext: false,
            scheduledOccurrenceID: context.job.occurrenceID
        )
        session.isScheduledTask = true
        if context.job.occurrenceID.flatMap({ history.conversation(occurrenceID: $0) }) != nil {
            await session.resumeScheduledRun(fallbackPrompt: payload.prompt)
        } else {
            await session.send(payload.prompt, source: .scheduledTask)
        }
        if case .failed(let failure) = session.phase {
            throw JobFailure(classification: failure.code.jobErrorClass,
                message: failure.diagnosticSummary
            )
        }
        guard let occurrenceID = context.job.occurrenceID,
              history.conversation(occurrenceID: occurrenceID)?.hasCompletedScheduledOutput == true else {
            throw JobFailure(
                classification: .transient,
                message: "Scheduled agent run did not produce a completed assistant response."
            )
        }
        return .succeeded(outputVersion: context.job.occurrenceID)
    }
}

private let workflowDecoder: JSONDecoder = {
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .iso8601
    return decoder
}()

private let workflowEncoder: JSONEncoder = {
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    encoder.outputFormatting = [.sortedKeys]
    return encoder
}()

private func decode<T: Decodable>(_ type: T.Type, from job: WorkJob) throws -> T {
    guard let payload = job.payload else {
        throw JobFailure(classification: .invalidInput, message: "Missing versioned job payload.")
    }
    do { return try workflowDecoder.decode(type, from: payload) }
    catch { throw JobFailure(classification: .invalidInput, message: error.localizedDescription) }
}
