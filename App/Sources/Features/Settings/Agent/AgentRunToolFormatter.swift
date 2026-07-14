import Foundation

enum AgentRunToolFormatter {
    struct Formatted {
        let title: String
        let detail: String?
    }

    /// Optional value resolver — called per `(key, value)` pair before
    /// the generic scalar render. Return a non-nil string to override
    /// the rendering for known-friendly fields (e.g. UUID episode/show
    /// IDs → titles, raw seconds → clock time). The view injects a
    /// resolver that knows about `AppStateStore`; the formatter itself
    /// stays decoupled from the live state.
    typealias ValueResolver = (String, AnyCodable) -> String?

    static func format(
        toolName: String,
        arguments: [String: AnyCodable],
        resolveValue: ValueResolver? = nil
    ) -> Formatted {
        Formatted(
            title: humanizeName(toolName),
            detail: genericDetail(arguments, resolveValue: resolveValue)
        )
    }

    private static func genericDetail(
        _ args: [String: AnyCodable],
        resolveValue: ValueResolver?
    ) -> String? {
        guard !args.isEmpty else { return nil }
        let pieces = args
            .sorted { $0.key < $1.key }
            .map { (key, value) -> String in
                if let resolved = resolveValue?(key, value) {
                    return "\(key): \(resolved)"
                }
                return "\(key): \(scalarDescription(value))"
            }
        return pieces.joined(separator: ", ")
    }

    private static func scalarDescription(_ value: AnyCodable) -> String {
        switch value {
        case .null: return "null"
        case .bool(let b): return b ? "true" : "false"
        case .int(let i): return String(i)
        case .double(let d): return String(d)
        case .string(let s): return "“\(truncate(s, to: 60))”"
        case .array(let arr): return "[\(arr.count) items]"
        case .object(let obj): return "{\(obj.count) keys}"
        }
    }

    private static func humanizeName(_ name: String) -> String {
        name
            .replacingOccurrences(of: "_", with: " ")
            .capitalized
    }

    private static func truncate(_ text: String, to length: Int) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.count <= length { return trimmed }
        let end = trimmed.index(trimmed.startIndex, offsetBy: length)
        return String(trimmed[..<end]) + "…"
    }
}
