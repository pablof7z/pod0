import Foundation

extension EpisodeWebPageMetadataParser {
    private static let namedEntities: [String: String] = [
        "amp": "&", "quot": "\"", "apos": "'", "lt": "<", "gt": ">", "nbsp": " ",
        "mdash": "—", "ndash": "–", "hellip": "…",
        "lsquo": "\u{2018}", "rsquo": "\u{2019}",
        "ldquo": "\u{201C}", "rdquo": "\u{201D}"
    ]

    private static let entityPattern = try! NSRegularExpression(
        pattern: #"&(#x[0-9A-Fa-f]+|#[0-9]+|[A-Za-z]+);"#
    )

    /// Decodes each `&entity;` in a single left-to-right pass so a decoded
    /// replacement is never re-scanned (sequential per-entity string
    /// replacement can turn `&amp;lt;` into `<` instead of the correct `&lt;`,
    /// and a Dictionary-driven replacement order is not stable across runs).
    static func decodeHTMLEntities(_ value: String) -> String {
        let range = NSRange(value.startIndex..., in: value)
        let matches = entityPattern.matches(in: value, range: range)
        guard !matches.isEmpty else { return value }

        var result = ""
        var cursor = value.startIndex
        for match in matches {
            guard let matchRange = Range(match.range, in: value),
                  let bodyRange = Range(match.range(at: 1), in: value)
            else { continue }
            result += value[cursor..<matchRange.lowerBound]
            let body = value[bodyRange]
            if let replacement = decodedEntity(body: String(body)) {
                result += replacement
            } else {
                result += value[matchRange]
            }
            cursor = matchRange.upperBound
        }
        result += value[cursor...]
        return result
    }

    private static func decodedEntity(body: String) -> String? {
        if let named = namedEntities[body] { return named }
        let scalarValue: UInt32?
        if body.hasPrefix("#x") || body.hasPrefix("#X") {
            scalarValue = UInt32(body.dropFirst(2), radix: 16)
        } else if body.hasPrefix("#") {
            scalarValue = UInt32(body.dropFirst())
        } else {
            scalarValue = nil
        }
        guard let scalarValue, let scalar = Unicode.Scalar(scalarValue) else { return nil }
        return String(Character(scalar))
    }
}
