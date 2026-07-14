import XCTest
@testable import Podcastr

/// Exercises the Up Next queue API on `PlaybackState`.
///
/// Scope is intentionally tight: we verify the array operations
/// (`enqueue`, `removeFromQueue`, `moveQueue`, `clearQueue`) and the
/// resolver-based head-pop behaviour of `playNext(resolve:)`. We don't
/// touch `setEpisode`/`play()` semantics — those are covered (or will be)
/// by audio-engine integration tests; mixing the two would force these
/// tests to depend on AVFoundation side effects.
@MainActor
final class PlaybackQueueTests: XCTestCase {

    // MARK: - enqueue

    func testEnqueueAppendsInOrder() {
        let state = PlaybackState()
        let a = UUID(), b = UUID(), c = UUID()

        state.enqueue(a)
        state.enqueue(b)
        state.enqueue(c)

        XCTAssertEqual(state.queue.map(\.episodeID), [a, b, c])
    }

    func testEnqueueIgnoresDuplicate() {
        let state = PlaybackState()
        let a = UUID()

        state.enqueue(a)
        state.enqueue(a)

        XCTAssertEqual(state.queue.map(\.episodeID), [a])
    }

    func testEnqueueIgnoresCurrentEpisode() {
        let state = PlaybackState()
        let episode = makeEpisode()
        state.episode = episode

        state.enqueue(episode.id)

        XCTAssertTrue(state.queue.isEmpty)
    }

    // MARK: - removeFromQueue

    func testRemoveFromQueueDropsEntry() {
        let state = PlaybackState()
        let a = UUID(), b = UUID()
        state.enqueue(a)
        state.enqueue(b)

        state.removeFromQueue(a)

        XCTAssertEqual(state.queue.map(\.episodeID), [b])
    }

    func testRemoveFromQueueIsIdempotent() {
        let state = PlaybackState()
        let a = UUID()

        state.removeFromQueue(a)  // no-op on empty queue
        state.enqueue(a)
        state.removeFromQueue(a)
        state.removeFromQueue(a)  // no-op the second time

        XCTAssertTrue(state.queue.isEmpty)
    }

    // MARK: - moveQueue

    func testMoveQueueReordersEntries() {
        let state = PlaybackState()
        let a = UUID(), b = UUID(), c = UUID()
        state.enqueue(a)
        state.enqueue(b)
        state.enqueue(c)

        // Move first item to the end. SwiftUI .onMove convention: destination
        // index is in the post-removal array, so end-of-list is `count`.
        state.moveQueue(from: IndexSet(integer: 0), to: 3)

        XCTAssertEqual(state.queue.map(\.episodeID), [b, c, a])
    }

    // MARK: - clearQueue

    func testClearQueueEmptiesEverything() {
        let state = PlaybackState()
        state.enqueue(UUID())
        state.enqueue(UUID())
        state.enqueue(UUID())

        state.clearQueue()

        XCTAssertTrue(state.queue.isEmpty)
    }

    // MARK: - playNext

    func testPlayNextReturnsFalseWhenQueueIsEmpty() {
        let state = PlaybackState()
        let played = state.playNext { _ in nil }
        XCTAssertFalse(played)
    }

    func testPlayNextPopsHeadAndCallsResolver() {
        let state = PlaybackState()
        let head = makeEpisode(guid: "head")
        let tail = makeEpisode(guid: "tail")
        state.enqueue(head.id)
        state.enqueue(tail.id)

        var resolverCalls: [UUID] = []
        let played = state.playNext { id in
            resolverCalls.append(id)
            return id == head.id ? head : nil
        }

        XCTAssertTrue(played)
        XCTAssertEqual(resolverCalls, [head.id])
        XCTAssertEqual(state.queue.map(\.episodeID), [tail.id])
        XCTAssertEqual(state.episode?.id, head.id)
    }

    func testPlayNextReturnsFalseWhenNoQueuedEpisodeResolves() {
        let state = PlaybackState()
        let stale = UUID()
        state.enqueue(stale)

        let played = state.playNext { _ in nil }

        XCTAssertFalse(played)
        XCTAssertTrue(state.queue.isEmpty)
    }

    func testMoveQueueCanPruneStaleEntriesBeforeReordering() {
        let state = PlaybackState()
        let stale = UUID()
        let a = UUID(), b = UUID(), c = UUID()
        state.queue = [stale, a, b, c].map { .episode($0) }

        state.moveQueue(from: IndexSet(integer: 0), to: 3) { id in
            id == stale ? nil : makeEpisode(id: id)
        }

        XCTAssertEqual(state.queue.map(\.episodeID), [b, c, a])
    }

    func testPruneQueueDropsAllStaleEntries() {
        let state = PlaybackState()
        state.queue = [UUID(), UUID(), UUID()].map { .episode($0) }

        let pruned = state.pruneQueue { _ in nil }

        XCTAssertEqual(pruned, 3)
        XCTAssertTrue(state.queue.isEmpty)
    }

    // MARK: - Fixtures

    private func makeEpisode(id: UUID = UUID(), guid: String = UUID().uuidString) -> Episode {
        Episode(
            id: id,
            podcastID: UUID(),
            guid: guid,
            title: "Episode \(guid)",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
    }
}
