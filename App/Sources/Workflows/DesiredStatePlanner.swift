import Foundation

struct DesiredStatePlanner: Sendable {
    struct Input: Sendable {
        let episodes: [Episode]
        let settings: Settings
        let artifacts: [ArtifactRecord]
        let transcripts: [TranscriptWorkflowSnapshot]
        let transcriptDesiredEpisodeIDs: Set<UUID>
        let embeddingSpaceID: String?
        let scheduledTasks: [AgentScheduledTask]
        let now: Date

        init(
            episodes: [Episode],
            settings: Settings,
            artifacts: [ArtifactRecord],
            transcripts: [TranscriptWorkflowSnapshot],
            transcriptDesiredEpisodeIDs: Set<UUID>,
            embeddingSpaceID: String? = nil,
            scheduledTasks: [AgentScheduledTask],
            now: Date
        ) {
            self.episodes = episodes
            self.settings = settings
            self.artifacts = artifacts
            self.transcripts = transcripts
            self.transcriptDesiredEpisodeIDs = transcriptDesiredEpisodeIDs
            self.embeddingSpaceID = embeddingSpaceID
            self.scheduledTasks = scheduledTasks
            self.now = now
        }
    }

    func plan(_ input: Input) -> [DesiredJob] {
        let artifacts = Dictionary(
            input.artifacts.map { (ArtifactKey(kind: $0.kind, subjectID: $0.subjectID), $0) },
            uniquingKeysWith: { _, newest in newest }
        )
        let transcripts = Dictionary(
            uniqueKeysWithValues: input.transcripts.map { ($0.episodeID, $0) }
        )
        var jobs: [DesiredJob] = []
        for episode in input.episodes {
            let audioVersion = Self.audioVersion(episode)
            let transcript = transcripts[episode.id]
            if input.transcriptDesiredEpisodeIDs.contains(episode.id),
               transcript?.sourceRevision != audioVersion {
                let provider = episode.requestedTranscriptProvider ?? input.settings.sttProvider
                let payload = TranscriptJobPayload(
                    provider: provider,
                    modelID: Self.transcriptProviderVersion(provider, settings: input.settings),
                    audioURL: episode.enclosureURL,
                    audioVersion: audioVersion,
                    userInitiated: episode.requestedTranscriptProvider != nil
                )
                let providerVersion = Self.transcriptProviderVersion(provider, settings: input.settings)
                jobs.append(DesiredJob(
                    idempotencyKey: "transcribe:\(episode.id):\(audioVersion):\(provider.rawValue):\(providerVersion)",
                    kind: .transcriptIngest,
                    subjectID: episode.id,
                    inputVersion: audioVersion,
                    payload: try? Self.encoder.encode(payload),
                    priority: episode.requestedTranscriptProvider == nil ? 40 : 100,
                    resourceClass: provider == .appleNative ? .onDeviceSTT : .remoteSTT
                ))
            }

            guard let transcript,
                  transcript.sourceRevision == audioVersion,
                  let embeddingSpaceID = input.embeddingSpaceID else { continue }
            let indexVersion = Self.transcriptIndexInputVersion(
                transcript,
                embeddingSpaceID: embeddingSpaceID
            )
            let semantic = artifacts[ArtifactKey(kind: .semanticIndex, subjectID: episode.id)]
            if !Self.isCurrent(semantic, inputVersion: indexVersion) {
                jobs.append(DesiredJob(
                    idempotencyKey: "index:\(episode.id):\(indexVersion)",
                    kind: .transcriptIndex,
                    subjectID: episode.id,
                    inputVersion: indexVersion,
                    priority: 50,
                    resourceClass: .embedding
                ))
            }

        }

        for task in input.scheduledTasks where task.nextRunAt <= input.now {
            let occurrence = Self.scheduledOccurrenceID(taskID: task.id, scheduledFor: task.nextRunAt)
            let payload = ScheduledRunPayload(
                taskID: task.id,
                scheduledFor: task.nextRunAt,
                prompt: task.prompt,
                modelID: input.settings.agentInitialModel,
                intervalSeconds: task.intervalSeconds
            )
            jobs.append(DesiredJob(
                idempotencyKey: occurrence,
                kind: .scheduledAgentRun,
                subjectID: task.id,
                inputVersion: occurrence,
                occurrenceID: occurrence,
                payload: try? Self.encoder.encode(payload),
                priority: 60,
                resourceClass: .scheduledAgent,
                maxAttempts: 12
            ))
        }
        return jobs.sorted { $0.idempotencyKey < $1.idempotencyKey }
    }

    static func audioVersion(_ episode: Episode) -> String {
        ArtifactRepository.version(parts: [
            episode.enclosureURL.absoluteString,
            episode.enclosureMimeType ?? "",
            String(episode.duration ?? 0),
        ])
    }

    static func scheduledOccurrenceID(taskID: UUID, scheduledFor: Date) -> String {
        "scheduled:\(taskID.uuidString):\(Int(scheduledFor.timeIntervalSince1970))"
    }

    private static func transcriptProviderVersion(_ provider: STTProvider, settings: Settings) -> String {
        switch provider {
        case .elevenLabsScribe: settings.elevenLabsSTTModel
        case .openRouterWhisper: settings.openRouterWhisperModel
        case .assemblyAI: settings.assemblyAISTTModel
        case .appleNative: "apple-native-v1"
        }
    }

    static func transcriptIndexInputVersion(
        _ transcript: TranscriptWorkflowSnapshot,
        embeddingSpaceID: String
    ) -> String {
        ArtifactRepository.version(parts: [
            transcript.contentDigest, embeddingSpaceID,
            "rust-evidence-v1", "core-recall-index-v1",
        ])
    }

    private static func isCurrent(_ artifact: ArtifactRecord?, inputVersion: String) -> Bool {
        artifact?.integrity == .available && artifact?.inputVersion == inputVersion
    }

    private struct ArtifactKey: Hashable {
        let kind: ArtifactKind
        let subjectID: UUID
    }

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()
}
