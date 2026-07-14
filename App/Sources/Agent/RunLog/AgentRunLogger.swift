import Foundation
import Observation
import OSLog

@MainActor
final class AgentRunLogger: ObservableObject {
    static let shared = AgentRunLogger()
    nonisolated static let logger = Logger.app("AgentRunLogger")

    @Published private(set) var runs: [AgentRun] = []

    let directoryURL: URL
    let fileURL: URL

    /// Cap on how many run records we keep on disk. A power user who
    /// drives the agent dozens of times a day across months would
    /// otherwise grow `runs` (and the on-disk JSON) unboundedly — each
    /// record holds the full system prompt + per-turn message history
    /// + tool-call payloads (often several KB). 500 is plenty of
    /// diagnostic history without making `save()` a multi-MB write
    /// on every new run.
    static let maxRetainedRuns: Int = 500

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
    }

    func log(run: AgentRun) {
        runs.insert(run, at: 0)
        if runs.count > Self.maxRetainedRuns {
            runs.removeLast(runs.count - Self.maxRetainedRuns)
        }
        scheduleSave()
    }

    func clear() {
        runs = []
        scheduleSave()
    }
}
