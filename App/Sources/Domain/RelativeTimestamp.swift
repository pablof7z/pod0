import Foundation

// MARK: - RelativeTimestamp

/// Shared compact relative-timestamp formatter used across agent views.
///
/// Two display styles are offered so callers don't need to hand-roll
/// interval arithmetic:
///
/// - **compact** ("5s ago", "3m ago", "2h ago") — for live activity
///   feeds where items are seconds to hours old (``AgentActivitySheet``).
/// - **extended** ("just now", "5m ago", "2h ago", "3d ago", "2w ago",
///   then an absolute date) — for memory / note lists that can span
///   weeks or months (``AgentContentRow``).
///
/// Both styles are pure functions of their inputs so they are safe to
/// call from any thread or actor context.
enum RelativeTimestamp {

    // MARK: - Styles

    /// Compact style: shows seconds for sub-minute recency, then minutes,
    /// then hours. Falls back to hours for anything older.
    ///
    /// Thresholds (seconds):
    /// - < 5 → "just now"
    /// - < 60 → "Xs ago"
    /// - < 3 600 → "Xm ago"
    /// - ≥ 3 600 → "Xh ago"
    ///
    /// Negative intervals (the timestamp is in the future — typically a
    /// clock-skew artifact on imported content) collapse to "just now"
    /// rather than rendering "-3s ago".
    static func compact(_ date: Date, now: Date = Date()) -> String {
        let interval = max(0, now.timeIntervalSince(date))
        if interval < Threshold.justNow  { return "just now" }
        if interval < Threshold.minutes  { return "\(Int(interval))s ago" }
        if interval < Threshold.hours    { return "\(Int(interval / Threshold.minutes))m ago" }
        return "\(Int(interval / Threshold.hours))h ago"
    }

    /// Extended style: "just now" for sub-minute recency, then minutes,
    /// hours, days, weeks. Falls back to an abbreviated absolute date
    /// for content older than four weeks.
    ///
    /// Thresholds (seconds):
    /// - < 60 → "just now"
    /// - < 3 600 → "Xm ago"
    /// - < 86 400 → "Xh ago"
    /// - < 604 800 → "Xd ago"
    /// - < 2 419 200 → "Xw ago"
    /// - ≥ 2 419 200 → abbreviated date + shortened time
    ///
    /// Negative intervals collapse to "just now" — same rationale as
    /// `compact(_:)`.
    static func extended(_ date: Date, now: Date = Date()) -> String {
        let interval = max(0, now.timeIntervalSince(date))
        if interval < Threshold.minutes  { return "just now" }
        if interval < Threshold.hours    { return "\(Int(interval / Threshold.minutes))m ago" }
        if interval < Threshold.day      { return "\(Int(interval / Threshold.hours))h ago" }
        if interval < Threshold.week     { return "\(Int(interval / Threshold.day))d ago" }
        if interval < Threshold.fourWeeks { return "\(Int(interval / Threshold.week))w ago" }
        return date.shortDateTime
    }

    // MARK: - Private

    private enum Threshold {
        static let justNow:    TimeInterval = 5
        static let minutes:    TimeInterval = 60
        static let hours:      TimeInterval = 3_600
        static let day:        TimeInterval = 86_400
        static let week:       TimeInterval = 7 * 86_400
        static let fourWeeks:  TimeInterval = 4 * 7 * 86_400
    }
}
