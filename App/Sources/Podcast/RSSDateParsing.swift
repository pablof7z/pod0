import Foundation

/// RFC 822 / RFC 1123 date parsing for RSS `<pubDate>`.
///
/// Lifted out of `RSSParser` to keep it under the 300-line soft cap. Kept as
/// an enum-namespace because none of these helpers need state.
enum DateParsing {

    /// Parses common RFC 822 / RFC 1123 forms emitted by RSS publishers. The
    /// spec is `EEE, dd MMM yyyy HH:mm:ss zzz`, but in the wild we see
    /// missing-second, two-digit-year, and offset-only variants. We try a
    /// fixed cascade in `en_US_POSIX` to keep the result locale-stable.
    static func parseRFC822(_ s: String) -> Date? {
        let trimmed = s.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        for f in rfc822Formatters {
            if let date = f.date(from: trimmed) { return date }
        }
        // Last-ditch: ISO 8601 (some Atom-flavored feeds emit it under
        // `<pubDate>`). `[.withInternetDateTime, .withFractionalSeconds]`
        // *requires* fractional seconds — so a clean "2024-01-01T12:00:00Z"
        // wouldn't parse if we used that combination alone. Try the
        // fractional-seconds variant first, then the plain form.
        if let date = isoFractionalFormatter.date(from: trimmed) { return date }
        return isoFormatter.date(from: trimmed)
    }

    /// RFC 822 / 1123 cascade. Ordered most-strict to most-tolerant so the
    /// earliest match wins.
    private static let rfc822Formats: [String] = [
        "EEE, dd MMM yyyy HH:mm:ss zzz",
        "EEE, d MMM yyyy HH:mm:ss zzz",
        "EEE, dd MMM yyyy HH:mm:ss Z",
        "EEE, d MMM yyyy HH:mm:ss Z",
        "dd MMM yyyy HH:mm:ss zzz",
        "EEE, dd MMM yyyy HH:mm zzz",
        "EEE, dd MMM yyyy HH:mm:ss",
    ]

    /// Pre-built `DateFormatter` per format. RSS feeds with 100+ episodes
    /// were re-allocating seven formatters per pubDate during parsing —
    /// ~700 allocations per feed refresh. `DateFormatter.date(from:)` is
    /// reentrant once configured.
    nonisolated(unsafe) private static let rfc822Formatters: [DateFormatter] = {
        let locale = Locale(identifier: "en_US_POSIX")
        let timeZone = TimeZone(identifier: "GMT")
        return rfc822Formats.map { fmt in
            let f = DateFormatter()
            f.locale = locale
            f.timeZone = timeZone
            f.dateFormat = fmt
            return f
        }
    }()

    nonisolated(unsafe) private static let isoFractionalFormatter: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()

    nonisolated(unsafe) private static let isoFormatter: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()
}
