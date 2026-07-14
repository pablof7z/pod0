import Foundation
import OSLog

extension AgentRunLogger {

    /// Snapshot the current `runs` array on MainActor and hand the
    /// encode + write off to the background serial queue. UI updates
    /// stay synchronous (the `@Published` array mutation already
    /// happened in `log(run:)` / `clear()`); the disk write is a
    /// fire-and-forget side effect that never blocks an agent turn.
    func scheduleSave() {
        let snapshot = runs
        let url = fileURL
        ioQueue.async {
            let data: Data
            do {
                data = try Self.encoder.encode(snapshot)
            } catch {
                Self.logger.error("AgentRunLogger: failed to encode snapshot — \(error.localizedDescription, privacy: .public)")
                return
            }
            do {
                try data.write(to: url, options: [.atomic])
            } catch {
                Self.logger.error("AgentRunLogger: failed to write snapshot to \(url.lastPathComponent, privacy: .public) — \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    static func load(from url: URL) -> [AgentRun] {
        guard let data = try? Data(contentsOf: url) else { return [] }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return (try? decoder.decode([AgentRun].self, from: data)) ?? []
    }
}
