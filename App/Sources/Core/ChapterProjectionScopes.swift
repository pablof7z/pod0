import Foundation

/// Admission policy for transient chapter projections.
///
/// The capacity bounds how many episodes hold a loaded chapter projection at
/// once. It is a budget to reallocate, never a reason to refuse: refusing left
/// the player showing its "no chapters" placeholder over an episode whose
/// chapters existed and were selected, with nothing to retry it.
///
/// Pure value logic so the policy is testable without a facade.
struct ChapterProjectionScopes {
    enum Admission: Equatable {
        /// Already held by another view; the caller has nothing to load.
        case alreadyRetained
        /// Newly admitted. The caller loads it, and drops `evicting` first.
        case load(evicting: UUID?)
    }

    let capacity: Int
    private var counts: [UUID: Int] = [:]
    /// Retain order, coldest first. Only ever holds ids present in `counts`.
    private var order: [UUID] = []

    init(capacity: Int) {
        self.capacity = max(1, capacity)
    }

    var count: Int { counts.count }

    /// Episodes currently holding a projection, for scoping bounded reads.
    var retainedEpisodeIDs: Set<UUID> { Set(counts.keys) }

    func isRetained(_ episodeID: UUID) -> Bool {
        counts[episodeID] != nil
    }

    mutating func removeAll() {
        counts.removeAll()
        order.removeAll()
    }

    /// Claims a scope. Admission never fails; at capacity the coldest other
    /// scope is evicted so the requested episode always loads.
    mutating func retain(_ episodeID: UUID) -> Admission {
        if let existing = counts[episodeID] {
            counts[episodeID] = existing + 1
            touch(episodeID)
            return .alreadyRetained
        }
        var evicted: UUID?
        if counts.count >= capacity, let coldest = order.first {
            counts[coldest] = nil
            order.removeFirst()
            evicted = coldest
        }
        counts[episodeID] = 1
        order.append(episodeID)
        return .load(evicting: evicted)
    }

    /// Drops one hold. Returns whether that was the last one, meaning the
    /// caller should tear the projection down.
    @discardableResult
    mutating func release(_ episodeID: UUID) -> Bool {
        guard let existing = counts[episodeID] else { return false }
        guard existing <= 1 else {
            counts[episodeID] = existing - 1
            return false
        }
        counts[episodeID] = nil
        order.removeAll { $0 == episodeID }
        return true
    }

    private mutating func touch(_ episodeID: UUID) {
        guard let index = order.firstIndex(of: episodeID) else { return }
        order.remove(at: index)
        order.append(episodeID)
    }
}
