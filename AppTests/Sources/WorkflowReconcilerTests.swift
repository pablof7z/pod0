import Foundation
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
            contentDigest: "transcript-hash",
            selectionRevision: 1
        )
        let withTranscript = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: [], transcripts: [transcript],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: input.now
        ))
        XCTAssertEqual(
            Set(withTranscript.map(\.kind)),
            [.transcriptIndex, .chapterArtifacts]
        )

        let indexJob = try XCTUnwrap(withTranscript.first { $0.kind == .transcriptIndex })
        let chapterJob = try XCTUnwrap(withTranscript.first { $0.kind == .chapterArtifacts })
        let completeArtifacts = [
            artifact(kind: .semanticIndex, subject: episode.id, input: indexJob.inputVersion),
            artifact(kind: .chapters, subject: episode.id, input: chapterJob.inputVersion),
            artifact(kind: .adSegments, subject: episode.id, input: chapterJob.inputVersion),
        ]
        XCTAssertTrue(planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: completeArtifacts, transcripts: [transcript],
            transcriptDesiredEpisodeIDs: [episode.id], scheduledTasks: [], now: input.now
        )).isEmpty)

        settings.embeddingsModel = "openai/text-embedding-3-small"
        let modelChanged = planner.plan(.init(
            episodes: [episode], settings: settings, artifacts: completeArtifacts, transcripts: [transcript],
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
            id: UUID(),
            label: "Daily brief",
            prompt: "Original prompt",
            intervalSeconds: 3_600,
            createdAt: due.addingTimeInterval(-100),
            lastRunAt: nil,
            nextRunAt: due
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

    func testPublisherChapterURLCreatesVersionedOwedWorkUntilArtifactIsCurrent() throws {
        var episode = makeEpisode()
        episode.chaptersURL = URL(string: "https://example.com/chapters-v1.json")!
        let planner = DesiredStatePlanner()
        let settings = Settings()
        let first = planner.plan(.init(
            episodes: [episode],
            settings: settings,
            artifacts: [], transcripts: [],
            transcriptDesiredEpisodeIDs: [],
            scheduledTasks: [],
            now: Date()
        ))
        let publisher = try XCTUnwrap(first.first { $0.kind == .publisherChapters })
        XCTAssertEqual(publisher.resourceClass, .planning)
        let sourceVersion = try XCTUnwrap(
            DesiredStatePlanner.publisherChapterInputVersion(episode)
        )
        XCTAssertEqual(publisher.inputVersion, sourceVersion)

        let artifact = ArtifactRecord(
            kind: .chapters,
            subjectID: episode.id,
            inputVersion: sourceVersion,
            outputVersion: "publisher-output",
            contentHash: "publisher-output",
            location: "/tmp/publisher-chapters.json",
            origin: DesiredStatePlanner.publisherChapterOrigin(
                sourceVersion: sourceVersion,
                enriched: false
            ),
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date()
        )
        let current = planner.plan(.init(
            episodes: [episode],
            settings: settings,
            artifacts: [artifact], transcripts: [],
            transcriptDesiredEpisodeIDs: [],
            scheduledTasks: [],
            now: Date()
        ))
        XCTAssertFalse(current.contains { $0.kind == .publisherChapters })

        episode.chaptersURL = URL(string: "https://example.com/chapters-v2.json")!
        let changed = planner.plan(.init(
            episodes: [episode],
            settings: settings,
            artifacts: [artifact], transcripts: [],
            transcriptDesiredEpisodeIDs: [],
            scheduledTasks: [],
            now: Date()
        ))
        XCTAssertNotEqual(
            changed.first { $0.kind == .publisherChapters }?.inputVersion,
            sourceVersion
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
}

@MainActor
final class WorkflowReconcilerTests: XCTestCase {
    private var fileURL: URL!
    private var appStore: AppStateStore!
    private var jobs: JobStore!
    private var artifacts: ArtifactRepository!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        appStore = made.store
        fileURL = made.fileURL
        let database = appStore.persistence.episodeStore.fileURL
        jobs = JobStore(fileURL: database)
        artifacts = ArtifactRepository(fileURL: database)
        try jobs.removeAll()
    }

    override func tearDown() async throws {
        if let fileURL { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        artifacts = nil
        jobs = nil
        appStore = nil
        fileURL = nil
        try await super.tearDown()
    }

    func testSecondPassIsNoOpAndDerivableJournalWipeRecreatesOwedWork() throws {
        var episode = Episode(
            podcastID: UUID(), guid: "owed", title: "Owed", pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/owed.mp3")!
        )
        episode.chaptersURL = URL(string: "https://example.com/owed-chapters.json")
        appStore.installEpisodeFixtures([episode], forPodcast: episode.podcastID)
        try jobs.removeAll()
        let reconciler = Reconciler(
            appStore: appStore, jobStore: jobs, artifacts: artifacts
        )

        XCTAssertEqual(try reconciler.reconcile().ensured, 1)
        XCTAssertEqual(try reconciler.reconcile().ensured, 0)
        XCTAssertEqual(try jobs.allJobs().map(\.kind), [.publisherChapters])

        try jobs.removeDerivableJobs()
        XCTAssertEqual(try reconciler.reconcile().ensured, 1)
        XCTAssertEqual(try jobs.allJobs().map(\.kind), [.publisherChapters])
    }
    func testReconcilerNeverInventsOrObsoletesAuthoritativeOccurrence() throws {
        let occurrence = DesiredJob(
            idempotencyKey: "notification:authoritative", kind: .newEpisodeNotification,
            subjectID: UUID(), inputVersion: "v1",
            occurrenceID: "notification:authoritative",
            resourceClass: .notification
        )
        _ = try jobs.ensureJob(occurrence)

        _ = try Reconciler(
            appStore: appStore, jobStore: jobs, artifacts: artifacts
        ).reconcile()

        XCTAssertEqual(
            try jobs.job(idempotencyKey: occurrence.idempotencyKey)?.state,
            .pending
        )
    }

    func testGlobalNotificationToggleObsoletesPendingDeliveryPermanently() throws {
        var settings = appStore.state.settings
        settings.notifyOnNewEpisodes = true
        appStore.updateSettings(settings)
        let occurrence = DesiredJob(
            idempotencyKey: "notification:toggle-off",
            kind: .newEpisodeNotification,
            subjectID: UUID(),
            inputVersion: "v1",
            occurrenceID: "notification:toggle-off",
            resourceClass: .notification
        )
        _ = try jobs.ensureJob(occurrence)
        let reconciler = Reconciler(
            appStore: appStore,
            jobStore: jobs,
            artifacts: artifacts
        )

        XCTAssertEqual(try reconciler.reconcile().obsoletedJobs, 0)
        XCTAssertEqual(
            try jobs.job(idempotencyKey: occurrence.idempotencyKey)?.state,
            .pending
        )

        settings.notifyOnNewEpisodes = false
        appStore.updateSettings(settings)

        XCTAssertEqual(try reconciler.reconcile().obsoletedJobs, 1)
        XCTAssertEqual(
            try jobs.job(idempotencyKey: occurrence.idempotencyKey)?.state,
            .obsolete
        )

        settings.notifyOnNewEpisodes = true
        appStore.updateSettings(settings)
        XCTAssertEqual(try reconciler.reconcile().obsoletedJobs, 0)
        XCTAssertEqual(
            try jobs.job(idempotencyKey: occurrence.idempotencyKey)?.state,
            .obsolete
        )
    }

    func testPolicyDisableObsoletesOnlyAutomaticDownloadOrigins() throws {
        let podcast = Podcast(
            id: UUID(),
            feedURL: URL(string: "https://example.com/feed.xml"),
            title: "Policy"
        )
        let episode = Episode(
            podcastID: podcast.id, guid: "policy", title: "Policy",
            pubDate: Date(), enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
        appStore.mutateState {
            $0.podcasts = [podcast]
            $0.subscriptions = [PodcastSubscription(
                podcastID: podcast.id,
                autoDownload: AutoDownloadPolicy(mode: .off, wifiOnly: false)
            )]
            $0.episodes = [episode]
        }
        let currentInputVersion = DesiredStatePlanner.audioVersion(episode)
        for origin in [DownloadIntentOrigin.autoDownload, .user, .playback] {
            let payload = DownloadJobPayload(
                origin: origin,
                enclosureURL: episode.enclosureURL,
                audioVersion: currentInputVersion
            )
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            _ = try jobs.ensureJob(DesiredJob(
                idempotencyKey: "download:\(origin.rawValue)",
                kind: .download,
                subjectID: episode.id,
                inputVersion: currentInputVersion,
                occurrenceID: "download:\(origin.rawValue)",
                payload: try encoder.encode(payload),
                resourceClass: .download
            ))
        }

        _ = try Reconciler(
            appStore: appStore,
            jobStore: jobs,
            artifacts: artifacts
        ).reconcile()

        XCTAssertEqual(try jobs.job(idempotencyKey: "download:autoDownload")?.state, .obsolete)
        XCTAssertEqual(try jobs.job(idempotencyKey: "download:user")?.state, .pending)
        XCTAssertEqual(try jobs.job(idempotencyKey: "download:playback")?.state, .pending)
    }

    func testInputChangeObsoletesOldDownloadAttemptWithoutLosingOccurrence() throws {
        let episode = Episode(
            podcastID: UUID(),
            guid: "changed-download",
            title: "Changed download",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/current.mp3")!
        )
        appStore.mutateState { $0.episodes = [episode] }
        let occurrence = "download:\(episode.id):old:user"
        let payload = DownloadJobPayload(
            origin: .user,
            enclosureURL: URL(string: "https://example.com/old.mp3")!,
            audioVersion: "old-input"
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        _ = try jobs.ensureJob(DesiredJob(
            idempotencyKey: occurrence,
            kind: .download,
            subjectID: episode.id,
            inputVersion: "old-input",
            occurrenceID: occurrence,
            payload: try encoder.encode(payload),
            resourceClass: .download
        ))

        _ = try Reconciler(
            appStore: appStore,
            jobStore: jobs,
            artifacts: artifacts
        ).reconcile()

        let obsolete = try XCTUnwrap(jobs.job(idempotencyKey: occurrence))
        XCTAssertEqual(obsolete.state, .obsolete)
        XCTAssertEqual(obsolete.occurrenceID, occurrence)
    }

    func testInlinePublisherChaptersAreAdoptedAsVersionedArtifact() throws {
        let episode = Episode(
            podcastID: UUID(),
            guid: "inline-chapters",
            title: "Inline chapters",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/inline.mp3")!,
            chapters: [
                .init(startTime: 0, title: "Opening"),
                .init(startTime: 120, title: "Main topic"),
            ]
        )
        appStore.mutateState { $0.episodes = [episode] }

        let report = try Reconciler(
            appStore: appStore,
            jobStore: jobs,
            artifacts: artifacts
        ).reconcile()

        XCTAssertEqual(report.adoptedArtifacts, 1)
        let selected = try XCTUnwrap(
            artifacts.current(kind: .chapters, subjectID: episode.id)
        )
        let sourceVersion = try XCTUnwrap(
            DesiredStatePlanner.publisherChapterInputVersion(episode)
        )
        XCTAssertEqual(selected.inputVersion, sourceVersion)
        XCTAssertEqual(
            selected.origin,
            DesiredStatePlanner.publisherChapterOrigin(
                sourceVersion: sourceVersion,
                enriched: false
            )
        )
        XCTAssertNotNil(selected.location)
    }
}
