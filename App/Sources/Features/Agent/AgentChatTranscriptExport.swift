import Foundation
import os.log

// MARK: - AgentChatTranscriptExport

/// Formats a chat transcript as a Markdown document and writes it to a
/// temporary file that can be shared via `ShareLink` or `ShareSheet`.
enum AgentChatTranscriptExport {

    private static let logger = Logger.app("AgentChatTranscriptExport")

    // MARK: - Constants

    private enum Const {
        static let tmpFilename = "agent-transcript.md"
        static let separator = "\n\n---\n\n"
    }

    // MARK: - Public API

    /// Builds a Markdown string from `messages` and writes it to a temp file.
    ///
    /// - Parameters:
    ///   - messages: The chat messages to export.
    ///   - batchSummaries: A map from batch UUID to the ordered list of
    ///     action-summary strings for that batch (already resolved on the
    ///     main actor by the caller). When present, tool-batch rows expand
    ///     into a bullet list instead of a bare count label.
    ///
    /// Returns the file URL on success, or `nil` if writing fails.
    static func write(_ messages: [ChatMessage], batchSummaries: [UUID: [String]] = [:]) -> URL? {
        let markdown = format(messages, batchSummaries: batchSummaries)
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(Const.tmpFilename)
        do {
            try markdown.write(to: url, atomically: true, encoding: .utf8)
            return url
        } catch {
            logger.error("AgentChatTranscriptExport: failed to write tmp file: \(error, privacy: .public)")
            return nil
        }
    }

    // MARK: - Formatting

    /// Produces a human-readable Markdown transcript from `messages`.
    ///
    /// Pass `batchSummaries` (a map keyed by batch UUID) to expand each
    /// tool-batch row into an itemised bullet list of the actions the agent
    /// took during that turn.
    static func format(_ messages: [ChatMessage], batchSummaries: [UUID: [String]] = [:]) -> String {
        guard !messages.isEmpty else { return "" }
        let header = buildHeader(messages: messages)
        let body = messages
            .map { formatMessage($0, batchSummaries: batchSummaries) }
            .joined(separator: Const.separator)
        return header + "\n\n" + body + "\n"
    }

    // MARK: - Private helpers

    private static func buildHeader(messages: [ChatMessage]) -> String {
        let first = messages.first?.timestamp
        let last  = messages.last?.timestamp
        let dateRange: String
        if let first, let last, first != last {
            dateRange = "\(formatted(first)) – \(formatted(last))"
        } else if let first {
            dateRange = formatted(first)
        } else {
            dateRange = ""
        }
        return "# Agent Transcript\n\n_\(dateRange)_\n\n_\(messages.count) message\(messages.count == 1 ? "" : "s")_"
    }

    private static func formatMessage(_ message: ChatMessage, batchSummaries: [UUID: [String]]) -> String {
        let label = roleLabel(for: message.role)
        let time  = formatted(message.timestamp)

        // For tool-batch rows, expand into a bullet list of summaries when
        // the caller has provided the resolved activity data.
        if case .toolBatch(let batchID, _) = message.role,
           let summaries = batchSummaries[batchID], !summaries.isEmpty {
            let bullets = summaries.map { "- \($0)" }.joined(separator: "\n")
            return "**\(label)** · \(time)\n\n\(bullets)"
        }

        return "**\(label)** · \(time)\n\n\(message.text)"
    }

    private static func roleLabel(for role: ChatMessage.Role) -> String {
        switch role {
        case .user:
            return "You"
        case .assistant:
            return "Agent"
        case .toolBatch(_, let count):
            return "Tools (\(count) action\(count == 1 ? "" : "s"))"
        case .error:
            return "Error"
        case .skillActivated(_, let displayName):
            return "Skill: \(displayName)"
        }
    }

    private static let timestampFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .short
        return f
    }()

    private static func formatted(_ date: Date) -> String {
        timestampFormatter.string(from: date)
    }
}
