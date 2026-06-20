import Foundation

// MARK: - Queue (Up Next)
//
// `PlaybackState.queue` is a PURE READ-ONLY projection of the Rust kernel queue.
// The kernel is the single source of truth; Swift never mutates `queue` locally.
// All user actions here dispatch to the kernel only; the fast in-process
// kernel round-trip updates `kernelQueue` (and therefore `queue`) on the next
// snapshot tick via `applyKernelQueue`. No optimistic overlay — no overlay races.
//
// `applyKernelQueue(_:)` — called only by `onQueueFromKernel` — is the sole writer.
//
// `pendingEnqueue` remains for the "Queued" button state: it gives instant
// feedback between an enqueue tap and the first kernel projection that confirms it.

extension PlaybackState {

    // MARK: - Kernel projection write (sole writer of kernelQueue)

    /// Called exclusively by the `onQueueFromKernel` callback. Replaces the
    /// authoritative kernel queue so the next `queue` read reflects the
    /// kernel's confirmed state.
    func applyKernelQueue(_ items: [QueueItem]) {
        kernelQueue = items
        // Clear any pendingEnqueue entries whose episode now appears in the
        // kernel projection (or whose queue slot was removed).
        let confirmedIDs = Set(items.map(\.episodeID))
        pendingEnqueue = pendingEnqueue.filter { !confirmedIDs.contains($0) }
    }

    // MARK: - Enqueueing

    /// Append a full-episode item to the Up Next queue. No-op when the episode
    /// is already the currently-playing episode. Unlike the previous
    /// `[UUID]`-based queue, the same episode *can* appear multiple times as
    /// bounded segments — but whole-episode duplicates are still deduplicated
    /// so a library-row "Queue" button can't stack the same full episode twice.
    ///
    /// Instant feedback is achieved via `pendingEnqueue`: on a synchronous
    /// `.accepted` result, the id is added to `pendingEnqueue` so `isQueued`
    /// returns `true` immediately. The kernel's authoritative queue projection
    /// (delivered via `onQueueFromKernel` / `applyKernelQueue`) is the sole
    /// writer to `kernelQueue`; `pendingEnqueue` entries are cleared in
    /// `applyKernelQueue` as each id is confirmed by the projection.
    func enqueue(_ episodeID: UUID) {
        guard episodeID != episode?.id else { return }
        let alreadyWhole = queue.contains { $0.episodeID == episodeID && $0.startSeconds == nil }
        guard !alreadyWhole else { return }
        guard !pendingEnqueue.contains(episodeID) else { return }
        if case .accepted = store?.kernelEnqueueLast(episodeID: episodeID) {
            pendingEnqueue.insert(episodeID)
        }
    }

    /// Append a `QueueItem` (possibly bounded) to the end of the queue.
    /// No deduplication — the agent intentionally queues multiple segments of
    /// the same episode. Dispatches to the kernel; the UI updates on the next
    /// projection tick.
    func enqueueItem(_ item: QueueItem) {
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
        store?.kernelDequeueEpisode(episodeID: episodeID)
    }

    /// Remove a single queue item by its stable slot identity.
    func removeFromQueue(itemID: UUID) {
        store?.kernelDequeueQueueItem(queueSlotID: itemID)
    }

    // MARK: - Reordering / pruning

    func moveQueue(from source: IndexSet, to destination: Int) {
        var reordered = kernelQueue
        reordered.move(fromOffsets: source, toOffset: destination)
        store?.kernelReorderQueue(queueSlotIDs: reordered.map(\.id))
    }

    /// Reorder the queue, pruning any items whose episode can no longer be
    /// resolved first. Dropped items are dispatched to the kernel for durable
    /// removal (so they don't survive a restart); the surviving items are
    /// reordered and dispatched via `kernelReorderQueue`.
    ///
    /// Previously this only dispatched `reorder_queue` for the survivors —
    /// Rust's `reorder_by_slot_ids` keeps omitted slot IDs at the tail, so
    /// unresolvable items were silently preserved across restarts. The explicit
    /// per-slot dequeue here makes pruning durable.
    func moveQueue(from source: IndexSet, to destination: Int, resolve: (UUID) -> Episode?) {
        let dropped = kernelQueue.filter { resolve($0.episodeID) == nil }
        var surviving = kernelQueue.filter { resolve($0.episodeID) != nil }
        surviving.move(fromOffsets: source, toOffset: min(destination, surviving.count))
        for item in dropped {
            store?.kernelDequeueQueueItem(queueSlotID: item.id)
        }
        store?.kernelReorderQueue(queueSlotIDs: surviving.map(\.id))
    }

    func clearQueue() {
        store?.kernelClearQueue()
    }

    /// Prune items whose episode can no longer be resolved (e.g. the user
    /// unsubscribed mid-queue). Dispatches a kernel dequeue for each dropped
    /// slot so the removal persists across restarts. The UI updates when the
    /// kernel confirms via `onQueueFromKernel`.
    ///
    /// Removing by slot ID (not episode ID) is safe here: each item has a
    /// stable slot UUID from the kernel; removing by episode ID would also
    /// strip any bounded segment of the same episode that IS resolvable.
    @discardableResult
    func pruneQueue(resolve: (UUID) -> Episode?) -> Int {
        let dropped = kernelQueue.filter { resolve($0.episodeID) == nil }
        for item in dropped {
            store?.kernelDequeueQueueItem(queueSlotID: item.id)
        }
        return dropped.count
    }

    // MARK: - Convenience

    /// Returns `true` when any queue item targets the given episode (by full-
    /// episode whole or bounded segment), OR when an enqueue dispatch for this
    /// episode is pending kernel confirmation. Used by UI affordances to show
    /// "Remove from queue" vs "Add to queue".
    func isQueued(_ episodeID: UUID) -> Bool {
        queue.contains { $0.episodeID == episodeID } || pendingEnqueue.contains(episodeID)
    }

}
