import Foundation

// MARK: - Agent Activity Log

extension AppStateStore {

    // MARK: - Rust policy bridge

    private struct AgentActivityEntriesResponse: Decodable {
        let entries: [AgentActivityEntry]
    }

    private struct AgentActivityCountResponse: Decodable {
        let count: Int
    }

    private struct AgentActivityIDsResponse: Decodable {
        let ids: [UUID]
    }

    private static let agentActivityMaxAgeSecs: TimeInterval = 30 * 24 * 3_600

    private static let agentActivityEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }()

    private static let agentActivityDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

    // MARK: - Recording

    func recordAgentActivity(_ entry: AgentActivityEntry) {
        guard let entries = agentActivityPolicyEntries(
            op: "agent_activity_record",
            extra: ["entry": agentActivityObject(entry)]
        ) else { return }
        state.agentActivity = entries
    }

    /// Prunes activity entries older than `activityMaxAgeSecs`.
    ///
    /// Call once at startup so the persisted log doesn't grow unboundedly across
    /// many months of use. This is a single state mutation so it fires only one
    /// `Persistence.save`.
    func pruneStaleActivityEntries() {
        let cutoff = Date().addingTimeInterval(-Self.agentActivityMaxAgeSecs)
        guard let entries = agentActivityPolicyEntries(
            op: "agent_activity_prune",
            extra: ["cutoff": Self.agentActivityISO8601(cutoff)]
        ),
              entries != state.agentActivity
        else { return }
        state.agentActivity = entries
    }

    func agentActivity(forBatch batchID: UUID) -> [AgentActivityEntry] {
        agentActivityPolicyEntries(
            op: "agent_activity_for_batch",
            extra: ["batch_id": batchID.uuidString]
        ) ?? []
    }

    /// All activity entries sorted newest-first — the canonical display order.
    /// Mirrors `activeNotes` so callers avoid touching `state.agentActivity`
    /// directly in view code.
    var sortedAgentActivity: [AgentActivityEntry] {
        agentActivityPolicyEntries(op: "agent_activity_sorted") ?? []
    }

    /// Count of active (not-yet-undone) activity entries.
    ///
    /// Used as the badge count on the Activity Log settings row and anywhere
    /// else that needs a quick summary of outstanding agent actions.
    var activeAgentActivityCount: Int {
        agentActivityPolicyCount(op: "agent_activity_active_count") ?? 0
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
            // Legacy Swift-owned memories are no longer imported into runtime
            // state. Preserve decode/display of old activity entries, but do
            // not resurrect the removed Swift memory store.
            _ = memoryID
        case .memoryFactRecorded(let key):
            kernel?.dispatch(namespace: "podcast.memory",
                             body: ["op": "forget", "key": key])
        }
        if let entries = agentActivityPolicyEntries(
            op: "agent_activity_mark_undone",
            extra: ["entry_id": entryID.uuidString]
        ) {
            state.agentActivity = entries
        }
    }

    func undoAgentActivityBatch(_ batchID: UUID) {
        let ids = agentActivityPolicyIDs(
            op: "agent_activity_undo_batch_ids",
            extra: ["batch_id": batchID.uuidString]
        ) ?? []
        for id in ids { undoAgentActivity(id) }
    }

    private func agentActivityPolicyEntries(
        op: String,
        extra: [String: Any] = [:]
    ) -> [AgentActivityEntry]? {
        guard let data = agentActivityPolicy(op: op, extra: extra),
              let decoded = try? Self.agentActivityDecoder.decode(
                AgentActivityEntriesResponse.self,
                from: data
              )
        else { return nil }
        return decoded.entries
    }

    private func agentActivityPolicyCount(op: String) -> Int? {
        guard let data = agentActivityPolicy(op: op),
              let decoded = try? Self.agentActivityDecoder.decode(
                AgentActivityCountResponse.self,
                from: data
              )
        else { return nil }
        return decoded.count
    }

    private func agentActivityPolicyIDs(
        op: String,
        extra: [String: Any] = [:]
    ) -> [UUID]? {
        guard let data = agentActivityPolicy(op: op, extra: extra),
              let decoded = try? Self.agentActivityDecoder.decode(
                AgentActivityIDsResponse.self,
                from: data
              )
        else { return nil }
        return decoded.ids
    }

    private func agentActivityPolicy(op: String, extra: [String: Any] = [:]) -> Data? {
        var payload: [String: Any] = [
            "op": op,
            "entries": agentActivityObjects(state.agentActivity),
        ]
        for (key, value) in extra {
            payload[key] = value
        }
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return json.withCString { ptr -> Data? in
            guard let result = nmp_app_podcast_agent_action_policy(ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            return String(cString: result).data(using: .utf8)
        }
    }

    private func agentActivityObjects(_ entries: [AgentActivityEntry]) -> [[String: Any]] {
        guard let data = try? Self.agentActivityEncoder.encode(entries),
              let object = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        return object
    }

    private func agentActivityObject(_ entry: AgentActivityEntry) -> [String: Any] {
        guard let data = try? Self.agentActivityEncoder.encode(entry),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [:] }
        return object
    }

    private static func agentActivityISO8601(_ date: Date) -> String {
        guard let data = try? agentActivityEncoder.encode(["date": date]),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: String],
              let encoded = object["date"]
        else { return "" }
        return encoded
    }
}
