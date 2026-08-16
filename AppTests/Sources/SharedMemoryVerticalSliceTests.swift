import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class SharedMemoryVerticalSliceTests: XCTestCase {
    func testRustOwnedMemoryCommandsSurviveRelaunch() async throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        var store: AppStateStore? = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(store?.sharedLibraryUnavailableReason)

        let added = await store?.addAgentMemory(content: "Likes concise answers")
        let created = try XCTUnwrap(added)
        let updated = await store?.updateAgentMemory(
            created.id,
            content: "Likes concise, evidence-backed answers"
        )
        XCTAssertEqual(updated, true)
        XCTAssertEqual(
            store?.state.agentMemories.first(where: { $0.id == created.id })?.revision,
            2
        )
        let deleted = await store?.deleteAgentMemory(created.id)
        XCTAssertEqual(deleted, true)
        XCTAssertFalse(store?.activeMemories.contains(where: { $0.id == created.id }) == true)
        let restored = await store?.restoreAgentMemory(created.id)
        XCTAssertEqual(restored, true)

        store = nil
        store = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertEqual(
            store?.state.agentMemories.first(where: { $0.id == created.id })?.content,
            "Likes concise, evidence-backed answers"
        )
    }
}
