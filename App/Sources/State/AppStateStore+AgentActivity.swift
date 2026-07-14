import Foundation

// MARK: - Agent Activity Log

extension AppStateStore {

    // MARK: - Limits

    /// Maximum number of activity entries retained in memory and on disk.
    ///
    /// When the log exceeds this cap, `recordAgentActivity` evicts the oldest
    /// entries first — preferring fully-undone batches so active undo state is
    /// preserved as long as possible. Mirrors the 100-message cap in
    /// `ChatHistoryStore`.
    private static let maxActivityEntries = 200

    /// Age threshold (in seconds) after which entries are pruned at app launch.
    /// 30 days — entries older than this carry no useful undo state and only
    /// bloat the persisted JSON file.
    private static let activityMaxAgeSecs: TimeInterval = 30 * 24 * 3_600

    // MARK: - Recording

    func recordAgentActivity(_ entry: AgentActivityEntry) {
        state.agentActivity.append(entry)
        trimActivityLogIfNeeded()
    }

    /// Drops the oldest entries when the log exceeds `maxActivityEntries`.
    ///
    /// Eviction order (youngest surviving):
    /// 1. Fully-undone entries — undo state is already applied; they carry no
    ///    further actionable information.
    /// 2. Oldest entries by timestamp — keeps the most recent batches intact.
    ///
    /// This is done as a single array assignment so it fires exactly one
    /// `state.didSet` (and therefore one Persistence.save / Spotlight reindex).
    private func trimActivityLogIfNeeded() {
        let count = state.agentActivity.count
        guard count > Self.maxActivityEntries else { return }
        let excess = count - Self.maxActivityEntries
        var log = state.agentActivity

        // First pass: remove fully-undone entries (oldest first) to fill the quota.
        var removed = 0
        var indicesToRemove: [Int] = []
        for (idx, entry) in log.enumerated() {
            guard removed < excess else { break }
            if entry.undone {
                indicesToRemove.append(idx)
                removed += 1
            }
        }

        // Second pass: if still over cap, remove the oldest entries regardless.
        if removed < excess {
            for (idx, _) in log.enumerated() {
                guard removed < excess else { break }
                if !indicesToRemove.contains(idx) {
                    indicesToRemove.append(idx)
                    removed += 1
                }
            }
        }

        let removeSet = Set(indicesToRemove)
        log = log.enumerated()
            .filter { !removeSet.contains($0.offset) }
            .map(\.element)
        state.agentActivity = log
    }

    /// Prunes activity entries older than `activityMaxAgeSecs`.
    ///
    /// Call once at startup so the persisted log doesn't grow unboundedly across
    /// many months of use. This is a single state mutation so it fires only one
    /// `Persistence.save`.
    func pruneStaleActivityEntries() {
        let cutoff = Date().addingTimeInterval(-Self.activityMaxAgeSecs)
        let trimmed = state.agentActivity.filter { $0.timestamp >= cutoff }
        guard trimmed.count != state.agentActivity.count else { return }
        state.agentActivity = trimmed
    }

    func agentActivity(forBatch batchID: UUID) -> [AgentActivityEntry] {
        state.agentActivity
            .filter { $0.batchID == batchID }
            .sorted { $0.timestamp > $1.timestamp }
    }

    /// All activity entries sorted newest-first — the canonical display order.
    /// Mirrors `activeMemories` / `activeNotes` so callers avoid touching
    /// `state.agentActivity` directly in view code.
    var sortedAgentActivity: [AgentActivityEntry] {
        state.agentActivity.sorted { $0.timestamp > $1.timestamp }
    }

    /// Count of active (not-yet-undone) activity entries.
    ///
    /// Used as the badge count on the Activity Log settings row and anywhere
    /// else that needs a quick summary of outstanding agent actions.
    var activeAgentActivityCount: Int {
        state.agentActivity.filter { !$0.undone }.count
    }

    /// Reverses the side-effect of an agent activity entry and marks it `undone`.
    /// Idempotent — calling on an already-undone entry is a no-op.
    func undoAgentActivity(_ entryID: UUID) {
        guard let idx = state.agentActivity.firstIndex(where: { $0.id == entryID }) else { return }
        guard !state.agentActivity[idx].undone else { return }
        switch state.agentActivity[idx].kind {
        case .noteCreated(let noteID):
            deleteNote(noteID)
        case .memoryRecorded(let memoryID):
            deleteAgentMemory(memoryID)
        }
        state.agentActivity[idx].undone = true
    }

    func undoAgentActivityBatch(_ batchID: UUID) {
        let ids = state.agentActivity
            .filter { $0.batchID == batchID && !$0.undone }
            .map(\.id)
        for id in ids { undoAgentActivity(id) }
    }
}
