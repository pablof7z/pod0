import Foundation

// MARK: - Queue (Up Next)

extension PlaybackState {

    // MARK: - Enqueueing

    /// Append a full-episode item to the Up Next queue. No-op when the episode
    /// is already the currently-playing episode. Unlike the previous
    /// `[UUID]`-based queue, the same episode *can* appear multiple times as
    /// bounded segments — but whole-episode duplicates are still deduplicated
    /// so a library-row "Queue" button can't stack the same full episode twice.
    func enqueue(_ episodeID: UUID) {
        guard episodeID != episode?.id else { return }
        let alreadyWhole = queue.contains { $0.episodeID == episodeID && $0.startSeconds == nil }
        guard !alreadyWhole else { return }
        queue.append(.episode(episodeID))
        store?.kernelEnqueueLast(episodeID: episodeID)
    }

    /// Append a `QueueItem` (possibly bounded) to the end of the queue.
    /// No deduplication — the agent intentionally queues multiple segments of
    /// the same episode.
    func enqueueItem(_ item: QueueItem) {
        queue.append(item)
        if let end = item.endSeconds {
            store?.kernelEnqueueSegmentLast(
                episodeID: item.episodeID.uuidString,
                startSeconds: item.startSeconds,
                endSeconds: end
            )
        } else {
            store?.kernelEnqueueLast(episodeID: item.episodeID)
        }
    }

    /// Insert a `QueueItem` at the head of Up Next so it plays after the
    /// currently-playing segment/episode finishes. No deduplication. Used by
    /// the agent's `play_episode` tool with `queue_position: "next"`.
    func insertNext(_ item: QueueItem) {
        queue.insert(item, at: 0)
        if let end = item.endSeconds {
            store?.kernelEnqueueSegmentNext(
                episodeID: item.episodeID.uuidString,
                startSeconds: item.startSeconds,
                endSeconds: end
            )
        } else {
            store?.kernelEnqueueNext(episodeID: item.episodeID)
        }
    }

    // MARK: - Removal

    /// Remove all queue items whose `episodeID` matches. Idempotent.
    func removeFromQueue(_ episodeID: UUID) {
        queue.removeAll { $0.episodeID == episodeID }
        store?.kernelDequeueEpisode(episodeID: episodeID)
    }

    /// Remove a single queue item by its stable slot identity.
    func removeFromQueue(itemID: UUID) {
        queue.removeAll { $0.id == itemID }
        store?.kernelDequeueQueueItem(queueSlotID: itemID)
    }

    // MARK: - Reordering / pruning

    func moveQueue(from source: IndexSet, to destination: Int) {
        queue.move(fromOffsets: source, toOffset: destination)
        store?.kernelReorderQueue(queueSlotIDs: queue.map(\.id))
    }

    func moveQueue(from source: IndexSet, to destination: Int, resolve: (UUID) -> Episode?) {
        pruneQueue(resolve: resolve)
        queue.move(fromOffsets: source, toOffset: min(destination, queue.count))
        store?.kernelReorderQueue(queueSlotIDs: queue.map(\.id))
    }

    func clearQueue() {
        queue.removeAll()
        store?.kernelClearQueue()
    }

    @discardableResult
    func pruneQueue(resolve: (UUID) -> Episode?) -> Int {
        let oldCount = queue.count
        queue.removeAll { resolve($0.episodeID) == nil }
        return oldCount - queue.count
    }

    // MARK: - Convenience

    /// Returns `true` when any queue item targets the given episode (by full-
    /// episode whole or bounded segment). Used by UI affordances to show
    /// "Remove from queue" vs "Add to queue".
    func isQueued(_ episodeID: UUID) -> Bool {
        queue.contains { $0.episodeID == episodeID }
    }

}
