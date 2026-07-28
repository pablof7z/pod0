import XCTest
@testable import Podcastr

@MainActor
final class SharedEpisodeImportRequestStoreTests: XCTestCase {
    func testQueuePersistsOrdersAndRemovesRequests() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "pod0-share-queue-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let store = SharedEpisodeImportRequestStore(directoryURL: root)
        let later = Date(timeIntervalSince1970: 200)
        let earlier = Date(timeIntervalSince1970: 100)

        let second = try store.enqueue(
            sourceURL: URL(string: "https://example.com/second")!,
            now: later
        )
        let first = try store.enqueue(
            sourceURL: URL(string: "https://example.com/first")!,
            now: earlier
        )

        XCTAssertEqual(try store.pendingRequests().map(\.id), [first.id, second.id])
        try store.remove(first)
        XCTAssertEqual(try store.pendingRequests(), [second])
    }

    func testRemovingMissingRequestIsIdempotent() throws {
        let root = FileManager.default.temporaryDirectory
            .appending(path: "pod0-share-queue-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let store = SharedEpisodeImportRequestStore(directoryURL: root)
        let request = SharedEpisodeImportRequest(
            sourceURL: URL(string: "https://example.com/episode")!
        )

        XCTAssertNoThrow(try store.remove(request))
    }
}
