import Foundation
import Observation
import OSLog

@MainActor
final class AgentRunLogger: ObservableObject {
    private struct AgentRunsResponse: Decodable {
        let runs: [AgentRun]
    }

    static let shared = AgentRunLogger()
    nonisolated static let logger = Logger.app("AgentRunLogger")

    @Published private(set) var runs: [AgentRun] = []

    let directoryURL: URL
    let fileURL: URL

    /// Configured once and reused — every `log(run:)` triggered a save
    /// that allocated a fresh encoder, configured it (`.iso8601` +
    /// `.sortedKeys`), then discarded it. Per-run agent transcripts can
    /// grow to several KB once turns + tool calls + system prompts add
    /// up, so the per-call configuration was a real (if small) tax.
    /// `nonisolated` because the actual `JSONEncoder` is `Sendable` and
    /// we never mutate it after the closure runs — the persistence
    /// extension reads it from a background queue.
    nonisolated static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.sortedKeys]
        return e
    }()

    nonisolated static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }()

    /// Serial queue that owns every encode + write to `runs.json`.
    /// `log(run:)` is called on the main actor from agent run
    /// finalisation; serialising the disk side-effect keeps the
    /// MainActor responsive while guaranteeing writes don't interleave
    /// and corrupt the file (which an `.atomic` write only protects
    /// against per-call, not across racing tasks).
    let ioQueue = DispatchQueue(label: "AgentRunLogger.io", qos: .utility)

    private init() {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.temporaryDirectory
        directoryURL = base.appendingPathComponent("AgentRunLog", isDirectory: true)
        fileURL = directoryURL.appendingPathComponent("runs.json")
        do {
            try FileManager.default.createDirectory(at: directoryURL, withIntermediateDirectories: true)
        } catch {
            let path = directoryURL.path
            Self.logger.error("AgentRunLogger: failed to create directory at \(path, privacy: .public) — \(error.localizedDescription, privacy: .public)")
        }
        runs = Self.load(from: fileURL)
        runs = normalizeLoadedRuns(runs)
    }

    func log(run: AgentRun) {
        guard let normalized = agentRunPolicyRuns(
            op: "agent_run_record",
            extra: ["run": agentRunObject(run)]
        ) else { return }
        runs = normalized
        scheduleSave()
    }

    func clear() {
        runs = []
        scheduleSave()
    }

    func filteredRuns(filter: AgentRunFilter) -> [AgentRun] {
        guard !filter.isEmpty else { return runs }
        return agentRunPolicyRuns(
            op: "agent_run_filter",
            extra: [
                "sources": filter.sources.map(\.rawValue),
                "outcomes": filter.outcomes.map(\.rawValue),
                "tool_name_query": filter.toolNameQuery,
            ]
        ) ?? []
    }

    func normalizeLoadedRuns(_ loadedRuns: [AgentRun]) -> [AgentRun] {
        agentRunPolicyRuns(op: "agent_run_normalize", runs: loadedRuns) ?? []
    }

    private func agentRunPolicyRuns(
        op: String,
        runs inputRuns: [AgentRun]? = nil,
        extra: [String: Any] = [:]
    ) -> [AgentRun]? {
        var payload: [String: Any] = [
            "op": op,
            "runs": agentRunObjects(inputRuns ?? runs),
        ]
        for (key, value) in extra {
            payload[key] = value
        }
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return nil }
        return json.withCString { ptr -> [AgentRun]? in
            guard let result = nmp_app_podcast_agent_action_policy(ptr) else {
                return nil
            }
            defer { nmp_free_string(result) }
            let response = String(cString: result)
            guard let data = response.data(using: .utf8),
                  let decoded = try? Self.decoder.decode(AgentRunsResponse.self, from: data)
            else { return nil }
            return decoded.runs
        }
    }

    private func agentRunObjects(_ runs: [AgentRun]) -> [[String: Any]] {
        guard let data = try? Self.encoder.encode(runs),
              let object = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        return object
    }

    private func agentRunObject(_ run: AgentRun) -> [String: Any] {
        guard let data = try? Self.encoder.encode(run),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [:] }
        return object
    }
}
