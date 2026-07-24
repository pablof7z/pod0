import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class SharedMemoryVerticalSliceTests: XCTestCase {
    func testLegacyMemoriesCutOverLosslesslyAndCommandsSurviveRelaunch() async throws {
        let fileURL = AppStateTestSupport.uniqueTempFileURL()
        let persistence = Persistence(fileURL: fileURL)
        defer { persistence.reset() }
        let firstID = UUID(uuidString: "11111111-1111-1111-1111-111111111111")!
        let secondID = UUID(uuidString: "22222222-2222-2222-2222-222222222222")!
        let firstDate = Date(timeIntervalSince1970: 1_704_153_600.125)
        let secondDate = Date(timeIntervalSince1970: 1_704_240_000)
        var legacy = AppState()
        legacy.agentMemories = [
            AgentMemory(
                id: firstID,
                revision: 1,
                content: "Prefers primary sources",
                createdAt: firstDate,
                deleted: false
            ),
            AgentMemory(
                id: secondID,
                revision: 1,
                content: "A deleted preference",
                createdAt: secondDate,
                deleted: true
            )
        ]
        legacy.compiledMemory = CompiledAgentMemory(
            text: "The listener prefers primary sources.",
            compiledAt: secondDate,
            sourceMemoryCount: 2,
            sourceMemoryIDs: [firstID, secondID]
        )
        XCTAssertTrue(persistence.write(legacy, revision: 7))
        let persistedFirstDate = try XCTUnwrap(
            persistence.load().agentMemories.first(where: { $0.id == firstID })?.createdAt
        )

        var store: AppStateStore? = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(store?.sharedLibraryUnavailableReason)
        let imported = try XCTUnwrap(store?.state.agentMemories.first {
            $0.id == firstID
        })
        XCTAssertEqual(imported.revision, 1)
        XCTAssertEqual(imported.content, "Prefers primary sources")
        XCTAssertEqual(
            imported.createdAt.timeIntervalSince1970,
            persistedFirstDate.timeIntervalSince1970,
            accuracy: 0.001
        )
        XCTAssertTrue(try XCTUnwrap(store?.state.agentMemories.first {
            $0.id == secondID
        }).deleted)
        XCTAssertEqual(store?.state.compiledMemory?.sourceMemoryIDs, [firstID, secondID])
        let retired = try persistence.load()
        XCTAssertTrue(retired.agentMemories.isEmpty, "Swift metadata must stop persisting memories")
        XCTAssertNil(retired.compiledMemory, "Swift metadata must stop persisting compiled memory")

        let createdMemory = await store?.addAgentMemory(content: "Likes concise answers")
        let created = try XCTUnwrap(createdMemory)
        let stale = created
        let didUpdate = await store?.updateAgentMemory(
            created.id,
            content: "Likes concise, evidence-backed answers"
        )
        XCTAssertTrue(didUpdate == true)
        let edited = try XCTUnwrap(store?.state.agentMemories.first {
            $0.id == created.id
        })
        XCTAssertEqual(edited.revision, 2)
        XCTAssertEqual(edited.content, "Likes concise, evidence-backed answers")
        let sharedLibrary = try XCTUnwrap(store?.sharedLibrary)
        do {
            try await sharedLibrary.updateMemory(stale, content: "A stale overwrite")
            XCTFail("Expected stale memory update to fail")
        } catch {
            XCTAssertEqual(error as? SharedLibraryError, .revisionConflict)
        }
        let didDelete = await store?.deleteAgentMemory(created.id)
        XCTAssertTrue(didDelete == true)
        XCTAssertFalse(store?.activeMemories.contains(where: { $0.id == created.id }) == true)
        let didRestore = await store?.restoreAgentMemory(created.id)
        XCTAssertTrue(didRestore == true)

        store = nil
        store = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(store?.sharedLibraryUnavailableReason)
        XCTAssertEqual(
            Set(store?.state.agentMemories.map(\.id) ?? []),
            Set([firstID, secondID, created.id])
        )
        XCTAssertEqual(
            store?.state.agentMemories.first(where: { $0.id == created.id })?.content,
            "Likes concise, evidence-backed answers"
        )

        let didClear = await store?.clearAllAgentMemories()
        XCTAssertTrue(didClear == true)
        XCTAssertTrue(store?.activeMemories.isEmpty == true)
        store = nil
        let relaunched = AppStateStore(
            persistence: persistence,
            sharedFeedHost: QueuedCoreFeedHost([]),
            startSubscriptionRefresh: false
        )
        XCTAssertNil(relaunched.sharedLibraryUnavailableReason)
        XCTAssertTrue(relaunched.activeMemories.isEmpty)
        XCTAssertEqual(
            relaunched.state.agentMemories.count,
            3,
            "Rust clear preserves revisioned tombstones"
        )
        XCTAssertTrue(relaunched.state.agentMemories.allSatisfy(\.deleted))
    }
}
