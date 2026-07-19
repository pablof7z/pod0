import Foundation

struct TranscriptJobPayload: Codable, Sendable, Equatable {
    let provider: STTProvider
    let modelID: String
    let audioURL: URL
    let audioVersion: String
    let userInitiated: Bool
}

struct ScheduledRunPayload: Codable, Sendable, Equatable {
    let taskID: UUID
    let scheduledFor: Date
    let prompt: String
    let modelID: String
    let intervalSeconds: TimeInterval
}

struct NotificationJobPayload: Codable, Sendable, Equatable {
    let discoveredAt: Date
    let podcastID: UUID
    let episodeTitle: String
}

struct AutoDownloadJobPayload: Codable, Sendable, Equatable {
    let discoveryOccurrenceID: String
    let policyVersion: String
}

enum DownloadIntentOrigin: String, Codable, Sendable, Equatable {
    case user
    case playback
    case autoDownload

    var priority: Int {
        switch self {
        case .user: 100
        case .playback: 80
        case .autoDownload: 20
        }
    }
}

struct DownloadJobPayload: Codable, Sendable, Equatable {
    let origin: DownloadIntentOrigin
    let enclosureURL: URL
    let audioVersion: String
}

struct PublisherChaptersJobPayload: Codable, Sendable, Equatable {
    let url: URL
    let sourceVersion: String
}

struct FeedDiscoveryPayload: Codable, Sendable, Equatable {
    struct EpisodeInput: Codable, Sendable, Equatable {
        let episodeID: UUID
        let inputVersion: String
        let pubDate: Date
        let title: String
    }

    let podcastID: UUID
    let occurrenceID: String
    let discoveredAt: Date
    let episodes: [EpisodeInput]
    let autoDownloadPolicy: AutoDownloadPolicy?
    let notificationsEnabled: Bool
    let policyVersion: String
}

struct DesiredStatePlanner: Sendable {
    struct Input: Sendable {
        let episodes: [Episode]
        let settings: Settings
        let artifacts: [ArtifactRecord]
        let transcriptDesiredEpisodeIDs: Set<UUID>
        let scheduledTasks: [AgentScheduledTask]
        let now: Date
    }

    func plan(_ input: Input) -> [DesiredJob] {
        let artifacts = Dictionary(
            input.artifacts.map { (ArtifactKey(kind: $0.kind, subjectID: $0.subjectID), $0) },
            uniquingKeysWith: { _, newest in newest }
        )
        var jobs: [DesiredJob] = []
        for episode in input.episodes {
            let audioVersion = Self.audioVersion(episode)
            let transcript = artifacts[ArtifactKey(kind: .transcript, subjectID: episode.id)]
            if input.transcriptDesiredEpisodeIDs.contains(episode.id),
               !Self.isCurrent(transcript, inputVersion: audioVersion) {
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

            let chapters = artifacts[ArtifactKey(kind: .chapters, subjectID: episode.id)]
            if let publisherVersion = Self.publisherChapterInputVersion(episode),
               let url = episode.chaptersURL,
               !Self.hasCurrentPublisherChapters(chapters, sourceVersion: publisherVersion) {
                let payload = PublisherChaptersJobPayload(
                    url: url,
                    sourceVersion: publisherVersion
                )
                jobs.append(DesiredJob(
                    idempotencyKey: "publisher-chapters:\(episode.id):\(publisherVersion)",
                    kind: .publisherChapters,
                    subjectID: episode.id,
                    inputVersion: publisherVersion,
                    payload: try? Self.encoder.encode(payload),
                    priority: 55,
                    resourceClass: .planning
                ))
            }

            guard let transcript, transcript.integrity == .available else { continue }
            let indexVersion = Self.indexInputVersion(transcript, settings: input.settings)
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

            let compilerVersion = Self.compilerInputVersion(transcript, settings: input.settings)
            let ads = artifacts[ArtifactKey(kind: .adSegments, subjectID: episode.id)]
            if !Self.isCurrent(chapters, inputVersion: compilerVersion)
                || !Self.isCurrent(ads, inputVersion: compilerVersion) {
                jobs.append(DesiredJob(
                    idempotencyKey: "compile:\(episode.id):\(compilerVersion)",
                    kind: .chapterArtifacts,
                    subjectID: episode.id,
                    inputVersion: compilerVersion,
                    priority: 30,
                    resourceClass: .utilityLLM
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

    static func publisherChapterInputVersion(_ episode: Episode) -> String? {
        if let url = episode.chaptersURL {
            return ArtifactRepository.version(parts: [
                url.absoluteString,
                "podcasting2-chapters-v1",
            ])
        }
        guard let chapters = episode.chapters,
              !chapters.isEmpty,
              chapters.contains(where: { !$0.isAIGenerated }) else { return nil }
        return ArtifactRepository.version(parts: chapters.flatMap {
            [
                String($0.startTime),
                String($0.endTime ?? -1),
                $0.title,
                $0.imageURL?.absoluteString ?? "",
                $0.linkURL?.absoluteString ?? "",
            ]
        } + ["inline-publisher-chapters-v1"])
    }

    static func publisherChapterOrigin(sourceVersion: String, enriched: Bool) -> String {
        "\(enriched ? "publisherEnriched" : "publisher"):\(sourceVersion)"
    }

    private static func hasCurrentPublisherChapters(
        _ artifact: ArtifactRecord?,
        sourceVersion: String
    ) -> Bool {
        guard artifact?.integrity == .available else { return false }
        return artifact?.origin == publisherChapterOrigin(
            sourceVersion: sourceVersion,
            enriched: false
        ) || artifact?.origin == publisherChapterOrigin(
            sourceVersion: sourceVersion,
            enriched: true
        )
    }

    private static func transcriptProviderVersion(_ provider: STTProvider, settings: Settings) -> String {
        switch provider {
        case .elevenLabsScribe: settings.elevenLabsSTTModel
        case .openRouterWhisper: settings.openRouterWhisperModel
        case .assemblyAI: settings.assemblyAISTTModel
        case .appleNative: "apple-native-v1"
        }
    }

    private static func indexInputVersion(_ transcript: ArtifactRecord, settings: Settings) -> String {
        ArtifactRepository.version(parts: [
            transcript.contentHash, settings.embeddingsModel,
            "rust-evidence-v1", "core-recall-index-v1",
        ])
    }

    private static func compilerInputVersion(_ transcript: ArtifactRecord, settings: Settings) -> String {
        ArtifactRepository.version(parts: [
            transcript.contentHash, settings.chapterCompilationModel, "chapter-prompt-v1",
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
