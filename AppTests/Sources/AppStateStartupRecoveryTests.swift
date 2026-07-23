import XCTest
@testable import Podcastr

@MainActor
final class AppStateStartupRecoveryTests: XCTestCase {
    func testCorruptMetadataBlocksStartupWithoutOverwritingSource() throws {
        let url = AppStateTestSupport.uniqueTempFileURL()
        defer { AppStateTestSupport.disposeIsolatedStore(at: url) }
        let persistence = Persistence(fileURL: url)
        let corrupt = Data(#"{"settings":"truncated""#.utf8)
        try persistence.episodeStore.commitMetadata(corrupt, generation: 7)

        let store = AppStateStore(
            persistence: persistence,
            startSubscriptionRefresh: false
        )

        XCTAssertTrue(store.startupRecoveryRequired)
        XCTAssertEqual(store.sharedLibraryUnavailableReason, "app_state_recovery_required")
        XCTAssertEqual(try persistence.episodeStore.loadMetadata(), corrupt)
        XCTAssertEqual(try persistence.episodeStore.loadGeneration(), 7)

        store.mutateState { $0.settings.hasCompletedOnboarding = true }
        let blockedJob = DesiredJob(
            idempotencyKey: "startup-recovery:blocked",
            kind: .scheduledAgentRun,
            subjectID: UUID(),
            inputVersion: "startup-recovery:blocked",
            occurrenceID: "startup-recovery:blocked",
            resourceClass: .scheduledAgent
        )
        store.mutateState(ensuring: [blockedJob]) {
            $0.settings.hasCompletedOnboarding = true
        }
        XCTAssertTrue(store.pendingAtomicJobs.isEmpty)
        XCTAssertEqual(try persistence.episodeStore.loadMetadata(), corrupt)
        XCTAssertEqual(try persistence.episodeStore.loadGeneration(), 7)
        XCTAssertThrowsError(try persistence.load())
    }
}
