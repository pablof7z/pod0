import Foundation
import XCTest
@testable import Podcastr

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

}
