import Foundation

// MARK: - RelativeDateBucket

/// Backward-looking relative-date grouping for sorted display of past entries.
/// Used by AgentMemoriesView, AgentNotesView, and any other view that groups
/// historical content by recency.
enum RelativeDateBucket: String, CaseIterable {
    case today     = "Today"
    case yesterday = "Yesterday"
    case thisWeek  = "This Week"
    case thisMonth = "This Month"
    case earlier   = "Earlier"

    static func bucket(for date: Date, now: Date, calendar: Calendar) -> RelativeDateBucket {
        if calendar.isDateInToday(date)     { return .today }
        if calendar.isDateInYesterday(date) { return .yesterday }
        let weekAgo  = calendar.date(byAdding: .day,   value: -7,  to: now) ?? now
        let monthAgo = calendar.date(byAdding: .month, value: -1,  to: now) ?? now
        if date >= weekAgo  { return .thisWeek }
        if date >= monthAgo { return .thisMonth }
        return .earlier
    }

    /// Groups `items` into ordered (bucket, items) pairs, preserving the
    /// `allCases` ordering of buckets and the input order within each bucket.
    ///
    /// - Parameters:
    ///   - items: The elements to group.
    ///   - dateKey: Key path (or closure) that returns the relevant `Date` for each element.
    ///   - now: Reference point for bucketing; defaults to the current time.
    ///   - calendar: Calendar used for day-boundary checks; defaults to `.current`.
    /// - Returns: Non-empty (bucket, items) pairs in `allCases` order, omitting empty buckets.
    static func grouped<T>(
        _ items: [T],
        dateKey: (T) -> Date,
        now: Date = Date(),
        calendar: Calendar = .current
    ) -> [(bucket: RelativeDateBucket, items: [T])] {
        var dict: [RelativeDateBucket: [T]] = [:]
        for item in items {
            let key = bucket(for: dateKey(item), now: now, calendar: calendar)
            dict[key, default: []].append(item)
        }
        return allCases.compactMap { b in
            guard let group = dict[b], !group.isEmpty else { return nil }
            return (b, group)
        }
    }
}
