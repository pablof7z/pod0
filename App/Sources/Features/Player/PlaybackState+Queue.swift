import Foundation
import Pod0Core

// MARK: - Queue (Up Next)

extension PlaybackState {

    // MARK: - Enqueueing

    /// Append a full-episode item to the Up Next queue. No-op when the episode
    /// is already the currently-playing episode. Unlike the previous
    /// `[UUID]`-based queue, the same episode *can* appear multiple times as
    /// bounded segments — but whole-episode duplicates are still deduplicated
    /// so a library-row "Queue" button can't stack the same full episode twice.
    func enqueue(_ episodeID: UUID) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.enqueue(
            entry: QueueItem.episode(episodeID).coreValue,
            placement: .back
        ))
    }

    /// Append a `QueueItem` (possibly bounded) to the end of the queue.
    /// No deduplication — the agent intentionally queues multiple segments of
    /// the same episode.
    func enqueueItem(_ item: QueueItem) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.enqueue(entry: item.coreValue, placement: .back))
    }

    /// Insert a `QueueItem` at the head of Up Next so it plays after the
    /// currently-playing segment/episode finishes. No deduplication. Used by
    /// the agent's `play_episode` tool with `queue_position: "next"`.
    func insertNext(_ item: QueueItem) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.enqueue(entry: item.coreValue, placement: .next))
    }

    /// Replace the current queue with an ordered list of `QueueItem`s and,
    /// if `playNow` is true, immediately dequeue and play the first one.
    /// The `playNow` path is the engine primitive behind the agent's
    /// `play_episode` tool with `queue_position: "now"` (single item) and
    /// behind callers that want to start a chain of segments.
    func enqueueSegments(_ items: [QueueItem], playNow: Bool) {
        guard !items.isEmpty, let sharedCore else { return }
        if playNow {
            let first = items[0]
            sharedCore.dispatchPlayback(.select(
                episodeId: EpisodeId(uuid: first.episodeID),
                segment: first.coreValue.segment,
                label: first.label
            ))
            for item in items.dropFirst().reversed() {
                sharedCore.dispatchPlayback(.enqueue(entry: item.coreValue, placement: .next))
            }
            sharedCore.dispatchPlayback(.play)
        } else {
            for item in items {
                sharedCore.dispatchPlayback(.enqueue(entry: item.coreValue, placement: .back))
            }
        }
    }

    // MARK: - Removal

    /// Remove all queue items whose `episodeID` matches. Idempotent.
    func removeFromQueue(_ episodeID: UUID) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.removeEpisodeFromQueue(
            episodeId: EpisodeId(uuid: episodeID)
        ))
    }

    /// Remove a single queue item by its stable slot identity.
    func removeFromQueue(itemID: UUID) {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.removeQueueEntry(
            queueEntryId: QueueEntryId(uuid: itemID)
        ))
    }

    // MARK: - Reordering / pruning

    func moveQueue(from source: IndexSet, to destination: Int) {
        guard let sharedCore else { return }
        var reordered = queue
        reordered.move(fromOffsets: source, toOffset: min(destination, reordered.count))
        sharedCore.dispatchPlayback(.replaceQueueOrder(
            queueEntryIds: reordered.map { QueueEntryId(uuid: $0.id) }
        ))
    }

    func clearQueue() {
        guard let sharedCore else { return }
        sharedCore.dispatchPlayback(.clearQueue)
    }

    // MARK: - Convenience

    /// Returns `true` when any queue item targets the given episode (by full-
    /// episode whole or bounded segment). Used by UI affordances to show
    /// "Remove from queue" vs "Add to queue".
    func isQueued(_ episodeID: UUID) -> Bool {
        queue.contains { $0.episodeID == episodeID }
    }

}
