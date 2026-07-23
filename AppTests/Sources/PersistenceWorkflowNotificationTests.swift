import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class PersistenceWorkflowNotificationTests: XCTestCase {
    func testMetadataOnlySaveDoesNotPublishAWorkflowCommit() {
        let fixture = makeFixture()
        defer { AppStateTestSupport.disposeIsolatedStore(at: fixture.url) }
        let unexpected = expectation(description: "metadata save remains workflow-silent")
        unexpected.isInverted = true
        let observer = observe(fixture.persistence) { unexpected.fulfill() }
        defer { NotificationCenter.default.removeObserver(observer) }

        fixture.persistence.save(AppState())

        wait(for: [unexpected], timeout: 0.1)
    }

    func testAtomicJobSavePublishesOneWorkflowCommit() {
        let fixture = makeFixture()
        defer { AppStateTestSupport.disposeIsolatedStore(at: fixture.url) }
        let committed = expectation(description: "workflow commit published")
        let observer = observe(fixture.persistence) { committed.fulfill() }
        defer { NotificationCenter.default.removeObserver(observer) }
        let job = DesiredJob(
            idempotencyKey: "notification:atomic-job",
            kind: .feedDiscovery,
            subjectID: UUID(),
            inputVersion: "v1",
            occurrenceID: "notification:atomic-job",
            resourceClass: .planning
        )

        fixture.persistence.save(AppState(), ensuring: [job])

        wait(for: [committed], timeout: 1)
    }

    private func makeFixture() -> (url: URL, persistence: Persistence) {
        let url = AppStateTestSupport.uniqueTempFileURL()
        return (url, Persistence(fileURL: url))
    }

    private func observe(
        _ persistence: Persistence,
        callback: @escaping @Sendable () -> Void
    ) -> NSObjectProtocol {
        NotificationCenter.default.addObserver(
            forName: .persistenceDidCommitWorkflowJobs,
            object: persistence,
            queue: .main
        ) { _ in callback() }
    }
}
