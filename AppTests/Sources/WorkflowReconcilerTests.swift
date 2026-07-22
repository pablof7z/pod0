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

    func testReconcilerNeverCreatesSwiftPublisherChapterJobs() throws {
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

        XCTAssertEqual(try reconciler.reconcile().ensured, 0)
        XCTAssertEqual(try reconciler.reconcile().ensured, 0)
        XCTAssertTrue(try jobs.allJobs().isEmpty)

        try jobs.removeDerivableJobs()
        XCTAssertEqual(try reconciler.reconcile().ensured, 0)
        XCTAssertTrue(try jobs.allJobs().isEmpty)
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

}
