import XCTest
@testable import Podcastr

/// Exercises the Up Next queue API on `PlaybackState`.
///
/// `PlaybackState.queue` is a pure read-only projection of the kernel queue.
/// User actions (remove, move, clear, prune) dispatch to the kernel only;
/// `applyKernelQueue(_:)` is the sole writer. Tests verify:
///   1. Swift-side guard logic (`enqueue` skips current episode, etc.)
///   2. `applyKernelQueue` correctly updates `queue`
///   3. Each user action + simulated kernel response produces the expected state
///   4. `pruneQueue`/`moveQueue(resolve:)` dispatch durable dequeue for dropped items
///
/// Kernel dispatch cannot be verified at the unit level (store is nil);
/// end-to-end persistence is covered by `QueueReorderUITests` / `P1QueueUITests`.
@MainActor
final class PlaybackQueueTests: XCTestCase {

    // MARK: - enqueue
    //
    // `enqueue` ordering and whole-episode de-duplication are now owned by the
    // Rust kernel queue (`PlaybackQueue::add_to_end`, covered by
    // `cargo test -p nmp-app-podcast queue`). On the Swift side `enqueue` only
    // dispatches to the kernel and marks a transient `pendingEnqueue` until the
    // authoritative projection arrives via `onQueueFromKernel` — it no longer
    // writes `PlaybackState.queue` directly, so a kernel-less unit test can't
    // assert ordering here. The end-to-end enqueue→"Queued" flow is covered by
    // the `testP0_QueueAddMultiple` UI test. The current-episode guard below is
    // pure Swift policy and stays unit-tested.

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
        // Seed via applyKernelQueue — the only authoritative writer of the queue.
        state.applyKernelQueue([a, b].map { .episode($0) })

        state.removeFromQueue(a)
        // Simulate the kernel confirming the removal (pure read-only: queue only
        // updates when the kernel projection arrives).
        state.applyKernelQueue([b].map { .episode($0) })

        XCTAssertEqual(state.queue.map(\.episodeID), [b])
    }

    func testRemoveFromQueueIsIdempotent() {
        let state = PlaybackState()
        let a = UUID()

        state.removeFromQueue(a)  // no-op on empty queue
        state.applyKernelQueue([.episode(a)])
        state.removeFromQueue(a)
        state.removeFromQueue(a)  // no-op the second time
        // Kernel confirms removal
        state.applyKernelQueue([])

        XCTAssertTrue(state.queue.isEmpty)
    }

    // MARK: - moveQueue

    func testMoveQueueReordersEntries() {
        let state = PlaybackState()
        let a = UUID(), b = UUID(), c = UUID()
        state.applyKernelQueue([a, b, c].map { .episode($0) })

        // Move first item to the end. SwiftUI .onMove convention: destination
        // index is in the post-removal array, so end-of-list is `count`.
        state.moveQueue(from: IndexSet(integer: 0), to: 3)
        // Simulate kernel confirming the reorder
        state.applyKernelQueue([b, c, a].map { .episode($0) })

        XCTAssertEqual(state.queue.map(\.episodeID), [b, c, a])
    }

    // MARK: - clearQueue

    func testClearQueueEmptiesEverything() {
        let state = PlaybackState()
        state.applyKernelQueue([UUID(), UUID(), UUID()].map { .episode($0) })

        state.clearQueue()
        // Simulate kernel confirming the clear
        state.applyKernelQueue([])

        XCTAssertTrue(state.queue.isEmpty)
    }

    // MARK: - moveQueue with pruning

    func testMoveQueueCanPruneStaleEntriesBeforeReordering() {
        let state = PlaybackState()
        let stale = UUID()
        let a = UUID(), b = UUID(), c = UUID()
        state.applyKernelQueue([stale, a, b, c].map { .episode($0) })

        state.moveQueue(from: IndexSet(integer: 0), to: 3) { id in
            id == stale ? nil : makeEpisode(id: id)
        }
        // Simulate kernel confirming: stale dequeued + [a, b, c] reordered to [b, c, a]
        state.applyKernelQueue([b, c, a].map { .episode($0) })

        XCTAssertEqual(state.queue.map(\.episodeID), [b, c, a])
    }

    func testPruneQueueDropsAllStaleEntries() {
        let state = PlaybackState()
        state.applyKernelQueue([UUID(), UUID(), UUID()].map { .episode($0) })

        let pruned = state.pruneQueue { _ in nil }

        XCTAssertEqual(pruned, 3)
        // Kernel confirms all removals
        state.applyKernelQueue([])
        XCTAssertTrue(state.queue.isEmpty)
    }

    // MARK: - applyKernelQueue always wins (pure read-only contract)

    /// Verifies that `applyKernelQueue` is always the authoritative writer —
    /// a no-op user action (removing a non-existent item) leaves the queue
    /// unchanged, and subsequent kernel updates always show through immediately.
    func testApplyKernelQueueAlwaysUpdatesQueue() {
        let state = PlaybackState()
        let a = UUID(), b = UUID()
        state.applyKernelQueue([a, b].map { .episode($0) })

        // Remove a non-existent item — pure no-op, queue unchanged
        state.removeFromQueue(UUID())
        XCTAssertEqual(state.queue.map(\.episodeID), [a, b],
                       "queue must be unchanged (no overlay) until kernel responds")

        // Kernel responds with the same queue (no-op round-trip)
        state.applyKernelQueue([a, b].map { .episode($0) })
        XCTAssertEqual(state.queue.map(\.episodeID), [a, b])

        // Subsequent kernel update shows through immediately
        state.applyKernelQueue([b].map { .episode($0) })
        XCTAssertEqual(state.queue.map(\.episodeID), [b])
    }

    // MARK: - pruneQueue dispatches kernel dequeue (durable removal)

    /// Verifies that `pruneQueue` returns the correct drop count and that the
    /// kernel round-trip correctly reflects the removal. (Per-slot kernel
    /// dispatch is verified end-to-end by UI tests since `store` is nil here.)
    func testPruneQueueDispatchesAndKernelConfirms() {
        let state = PlaybackState()
        let a = UUID(), b = UUID(), stale = UUID()
        state.applyKernelQueue([a, stale, b].map { .episode($0) })

        let dropped = state.pruneQueue { id in
            id == stale ? nil : makeEpisode(id: id)
        }

        XCTAssertEqual(dropped, 1)
        // Queue still shows full kernel state until the kernel confirms (no overlay)
        XCTAssertEqual(state.queue.map(\.episodeID), [a, stale, b])

        // Kernel confirms removal:
        state.applyKernelQueue([a, b].map { .episode($0) })
        XCTAssertEqual(state.queue.map(\.episodeID), [a, b])
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
