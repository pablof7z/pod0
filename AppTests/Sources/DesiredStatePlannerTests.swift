import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class DesiredStatePlannerTests: XCTestCase {
    func testPlanIsPureIdempotentAndVersionDriven() throws {
        let episode = makeEpisode()
        var settings = Settings()
        let planner = DesiredStatePlanner()
        let input = DesiredStatePlanner.Input(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: Date()
        )

        let first = planner.plan(input)
        XCTAssertEqual(first, planner.plan(input))
        XCTAssertEqual(Set(first.map(\.kind)), [.transcriptIngest])

        let transcript = TranscriptWorkflowSnapshot(
            episodeID: episode.id,
            sourceRevision: DesiredStatePlanner.audioVersion(episode),
            contentDigest: String(repeating: "1", count: 64),
            selectionRevision: 1
        )
        let withTranscript = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [transcript],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: input.now
        ))
        XCTAssertEqual(Set(withTranscript.map(\.kind)), [.transcriptIndex, .chapterArtifacts])

        let indexJob = try XCTUnwrap(withTranscript.first { $0.kind == .transcriptIndex })
        let chapterJob = try XCTUnwrap(withTranscript.first { $0.kind == .chapterArtifacts })
        let completeArtifacts = [
            artifact(kind: .semanticIndex, subject: episode.id, input: indexJob.inputVersion),
        ]
        let selectedChapter = chapterSnapshot(
            episodeID: episode.id,
            artifactID: "compiled-artifact",
            source: .generated
        )
        let chapterCompletion = ChapterWorkflowCompletion(
            episodeID: episode.id,
            kind: .chapterArtifacts,
            inputVersion: chapterJob.inputVersion,
            artifactID: selectedChapter.artifactID
        )
        XCTAssertTrue(planner.plan(.init(
            episodes: [episode], settings: settings,
            artifacts: completeArtifacts, transcripts: [transcript],
            chapters: [selectedChapter], chapterCompletions: [chapterCompletion],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: input.now
        )).isEmpty)

        settings.embeddingsModel = "openai/text-embedding-3-small"
        let modelChanged = planner.plan(.init(
            episodes: [episode], settings: settings,
            artifacts: completeArtifacts, transcripts: [transcript],
            chapters: [selectedChapter], chapterCompletions: [chapterCompletion],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: input.now
        ))
        XCTAssertEqual(Set(modelChanged.map(\.kind)), [.transcriptIndex])
    }

    func testPolicyAndInputChangesProduceDeterministicPlanChanges() {
        var episode = makeEpisode()
        let planner = DesiredStatePlanner()
        let settings = Settings()
        let desired = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: Date()
        ))
        XCTAssertTrue(desired.contains { $0.kind == .transcriptIngest })

        let disabled = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [], now: Date()
        ))
        XCTAssertFalse(disabled.contains { $0.kind == .transcriptIngest })

        let oldKey = desired.first { $0.kind == .transcriptIngest }?.idempotencyKey
        episode.enclosureURL = URL(string: "https://example.com/replaced.mp3")!
        let changed = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: Date()
        ))
        XCTAssertNotEqual(oldKey, changed.first { $0.kind == .transcriptIngest }?.idempotencyKey)
    }

    func testScheduledOccurrenceIdentityAndPayloadAreImmutable() throws {
        let due = Date(timeIntervalSince1970: 10_000)
        var task = AgentScheduledTask(
            id: UUID(), label: "Daily brief", prompt: "Original prompt",
            intervalSeconds: 3_600, createdAt: due.addingTimeInterval(-100),
            lastRunAt: nil, nextRunAt: due
        )
        let settings = Settings()
        let planner = DesiredStatePlanner()
        let first = try XCTUnwrap(planner.plan(.init(
            episodes: [], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [task], now: due
        )).first)
        let firstPayload = try XCTUnwrap(first.payload)

        task.prompt = "Edited prompt"
        task.nextRunAt = due.addingTimeInterval(3_600)
        let edited = try XCTUnwrap(planner.plan(.init(
            episodes: [], settings: settings, artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [task],
            now: due.addingTimeInterval(3_600)
        )).first)

        XCTAssertEqual(
            first.idempotencyKey,
            DesiredStatePlanner.scheduledOccurrenceID(taskID: task.id, scheduledFor: due)
        )
        XCTAssertNotEqual(first.idempotencyKey, edited.idempotencyKey)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        XCTAssertEqual(
            try decoder.decode(ScheduledRunPayload.self, from: firstPayload).prompt,
            "Original prompt"
        )
    }

    func testPlannerNeverCreatesSwiftPublisherChapterWork() {
        var episode = makeEpisode()
        episode.chaptersURL = URL(string: "https://example.com/chapters-v1.json")!
        let jobs = DesiredStatePlanner().plan(.init(
            episodes: [episode], settings: Settings(), artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [], now: Date()
        ))
        XCTAssertFalse(jobs.contains { $0.kind == .publisherChapters })
    }

    func testPublisherObservationMustCommitBeforeModelEnrichmentIsOwed() throws {
        var episode = makeEpisode()
        episode.chaptersURL = URL(string: "https://example.com/chapters.json")!
        let transcript = TranscriptWorkflowSnapshot(
            episodeID: episode.id,
            sourceRevision: DesiredStatePlanner.audioVersion(episode),
            contentDigest: String(repeating: "1", count: 64),
            selectionRevision: 1
        )
        let jobs = DesiredStatePlanner().plan(.init(
            episodes: [episode], settings: Settings(), artifacts: [], transcripts: [transcript],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [], now: Date()
        ))

        XCTAssertFalse(jobs.contains { $0.kind == .publisherChapters })
        XCTAssertTrue(jobs.contains { $0.kind == .transcriptIndex })
        XCTAssertFalse(jobs.contains { $0.kind == .chapterArtifacts })
    }

    func testRustPublisherProjectionUnlocksEnrichmentAndCompletionStopsCompiler() throws {
        var episode = makeEpisode()
        episode.chaptersURL = URL(string: "https://example.com/chapters.json")!
        let transcript = TranscriptWorkflowSnapshot(
            episodeID: episode.id,
            sourceRevision: DesiredStatePlanner.audioVersion(episode),
            contentDigest: String(repeating: "1", count: 64),
            selectionRevision: 1
        )
        let publisher = chapterSnapshot(
            episodeID: episode.id,
            artifactID: "publisher-artifact",
            source: .publisher
        )
        let publisherWorkflow = publisherWorkflow(episodeID: episode.id, stage: .succeeded)
        let enrichmentJobs = DesiredStatePlanner().plan(.init(
            episodes: [episode], settings: Settings(), artifacts: [], transcripts: [transcript],
            chapters: [publisher], publisherChapterWorkflows: [publisherWorkflow],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [], now: Date()
        ))
        let enrichment = try XCTUnwrap(
            enrichmentJobs.first { $0.kind == .chapterArtifacts }
        )
        XCTAssertFalse(enrichmentJobs.contains { $0.kind == .publisherChapters })

        let enriched = chapterSnapshot(
            episodeID: episode.id,
            artifactID: "enriched-artifact",
            source: .publisherEnriched
        )
        let enrichedCompletion = ChapterWorkflowCompletion(
            episodeID: episode.id,
            kind: .chapterArtifacts,
            inputVersion: enrichment.inputVersion,
            artifactID: enriched.artifactID
        )
        let current = DesiredStatePlanner().plan(.init(
            episodes: [episode], settings: Settings(), artifacts: [], transcripts: [transcript],
            chapters: [enriched], publisherChapterWorkflows: [publisherWorkflow],
            chapterCompletions: [enrichedCompletion],
            transcriptDesiredEpisodeIDs: [], scheduledTasks: [], now: Date()
        ))
        XCTAssertFalse(current.contains { $0.kind == .publisherChapters })
        XCTAssertFalse(current.contains { $0.kind == .chapterArtifacts })
    }

    private func publisherWorkflow(
        episodeID: UUID,
        stage: PublisherChapterWorkflowStage
    ) -> PublisherChapterWorkflowProjection {
        PublisherChapterWorkflowProjection(
            episodeId: EpisodeId(uuid: episodeID),
            sourceVersion: "rust-source-v1",
            stage: stage,
            workflowRevision: StateRevision(value: 2),
            attempt: 1,
            maxAttempts: 5,
            requestId: nil,
            cancellationId: CancellationId(high: 1, low: 2),
            notBefore: nil,
            selectedArtifactId: stage == .succeeded
                ? ChapterArtifactId(high: 3, low: 4) : nil,
            failure: nil,
            createdAt: UnixTimestampMilliseconds(value: 1_000),
            updatedAt: UnixTimestampMilliseconds(value: 2_000),
            canRetry: false,
            canCancel: false
        )
    }

    private func makeEpisode() -> Episode {
        Episode(
            podcastID: UUID(), guid: "planner", title: "Planner",
            pubDate: Date(), enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
    }

    private func artifact(
        kind: ArtifactKind,
        subject: UUID,
        input: String,
        output: String = "output"
    ) -> ArtifactRecord {
        ArtifactRecord(
            kind: kind, subjectID: subject, inputVersion: input,
            outputVersion: output, contentHash: output,
            location: nil, origin: "test", schemaVersion: 1,
            integrity: .available, verifiedAt: Date()
        )
    }

    private func chapterSnapshot(
        episodeID: UUID,
        artifactID: String,
        source: ChapterArtifactSource
    ) -> ChapterWorkflowSnapshot {
        ChapterWorkflowSnapshot(
            episodeID: episodeID,
            artifactID: artifactID,
            sourceRevision: "chapter-source-v1",
            contentDigest: String(repeating: "1", count: 64),
            selectionRevision: 1,
            provenance: ChapterArtifactProvenance(
                source: source,
                provider: source == .publisher ? nil : "test",
                model: source == .publisher ? nil : "test-model",
                policyVersion: source == .publisher ? 0 : 1,
                sourcePayloadDigest: ContentDigest(word0: 1, word1: 2, word2: 3, word3: 4),
                transcriptVersionId: nil,
                transcriptContentDigest: nil,
                legacyImport: nil
            )
        )
    }
}
