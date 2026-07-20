import Foundation
import Pod0Core

struct DesiredStatePlanner: Sendable {
    struct Input: Sendable {
        let episodes: [Episode]
        let settings: Settings
        let artifacts: [ArtifactRecord]
        let transcripts: [TranscriptWorkflowSnapshot]
        let chapters: [ChapterWorkflowSnapshot]
        let publisherChapterWorkflows: [PublisherChapterWorkflowProjection]
        let chapterCompletions: [ChapterWorkflowCompletion]
        let transcriptDesiredEpisodeIDs: Set<UUID>
        let scheduledTasks: [AgentScheduledTask]
        let now: Date

        init(
            episodes: [Episode],
            settings: Settings,
            artifacts: [ArtifactRecord],
            transcripts: [TranscriptWorkflowSnapshot],
            chapters: [ChapterWorkflowSnapshot] = [],
            publisherChapterWorkflows: [PublisherChapterWorkflowProjection] = [],
            chapterCompletions: [ChapterWorkflowCompletion] = [],
            transcriptDesiredEpisodeIDs: Set<UUID>,
            scheduledTasks: [AgentScheduledTask],
            now: Date
        ) {
            self.episodes = episodes
            self.settings = settings
            self.artifacts = artifacts
            self.transcripts = transcripts
            self.chapters = chapters
            self.publisherChapterWorkflows = publisherChapterWorkflows
            self.chapterCompletions = chapterCompletions
            self.transcriptDesiredEpisodeIDs = transcriptDesiredEpisodeIDs
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
        let chapters = Dictionary(
            uniqueKeysWithValues: input.chapters.map { ($0.episodeID, $0) }
        )
        let publisherChapterWorkflows = Dictionary(uniqueKeysWithValues:
            input.publisherChapterWorkflows.compactMap { workflow in
                workflow.episodeId.uuid.map { ($0, workflow) }
            }
        )
        let chapterCompletions = Dictionary(
            grouping: input.chapterCompletions,
            by: \.episodeID
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

            let chapter = chapters[episode.id]
            let completions = chapterCompletions[episode.id] ?? []
            let publisherReady = episode.chaptersURL == nil
                || Self.hasSucceededPublisherWorkflow(
                    workflow: publisherChapterWorkflows[episode.id]
                )

            guard let transcript, transcript.sourceRevision == audioVersion else { continue }
            let indexVersion = Self.transcriptIndexInputVersion(
                transcript,
                settings: input.settings
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

            if !publisherReady || chapter?.provenance.source == .agentComposed {
                continue
            }
            let compilerVersion = Self.chapterCompilerInputVersion(
                transcript,
                settings: input.settings
            )
            if !Self.hasCurrentCompiledChapters(
                chapter,
                completions: completions,
                inputVersion: compilerVersion
            ) {
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

    private static func hasSucceededPublisherWorkflow(
        workflow: PublisherChapterWorkflowProjection?
    ) -> Bool {
        workflow?.stage == .succeeded
            && workflow?.selectedArtifactId != nil
    }

    private static func hasCurrentCompiledChapters(
        _ chapter: ChapterWorkflowSnapshot?,
        completions: [ChapterWorkflowCompletion],
        inputVersion: String
    ) -> Bool {
        guard let chapter else { return false }
        return completions.contains {
            $0.kind == .chapterArtifacts
                && $0.inputVersion == inputVersion
                && $0.artifactID == chapter.artifactID
        }
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
        settings: Settings
    ) -> String {
        ArtifactRepository.version(parts: [
            transcript.contentDigest, settings.embeddingsModel,
            "rust-evidence-v1", "core-recall-index-v1",
        ])
    }

    static func chapterCompilerInputVersion(
        _ transcript: TranscriptWorkflowSnapshot,
        settings: Settings
    ) -> String {
        ArtifactRepository.version(parts: [
            transcript.contentDigest, settings.chapterCompilationModel, "chapter-prompt-v1",
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
