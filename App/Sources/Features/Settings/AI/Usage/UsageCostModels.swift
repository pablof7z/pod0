import Foundation

// MARK: - Range

enum CostRange: String, CaseIterable, Identifiable {
    case today
    case last7Days
    case last30Days
    case all

    var id: String { rawValue }

    var shortLabel: String {
        switch self {
        case .today:      return "Today"
        case .last7Days:  return "7 days"
        case .last30Days: return "30 days"
        case .all:        return "All"
        }
    }

    var displayLabel: String {
        switch self {
        case .today:      return "Today"
        case .last7Days:  return "Last 7 days"
        case .last30Days: return "Last 30 days"
        case .all:        return "All time"
        }
    }

    func since(now: Date) -> Date? {
        let cal = Calendar.current
        switch self {
        case .today:      return cal.startOfDay(for: now)
        case .last7Days:  return cal.date(byAdding: .day, value: -7, to: now)
        case .last30Days: return cal.date(byAdding: .day, value: -30, to: now)
        case .all:        return nil
        }
    }
}

// MARK: - Aggregation helpers

struct CostBucket: Identifiable {
    let id: String
    let name: String
    let cost: Double
    let count: Int
    let cachedTokens: Int
}

struct DailyCostPoint: Identifiable {
    let id: String
    let day: Date
    let feature: String
    let cost: Double
}

enum CostAggregator {
    static func filter(_ records: [UsageRecord], by range: CostRange) -> [UsageRecord] {
        guard let since = range.since(now: Date()) else { return records }
        return records.filter { $0.at >= since }
    }

    static func aggregate(_ records: [UsageRecord], by key: (UsageRecord) -> String) -> [CostBucket] {
        var grouped: [String: (cost: Double, count: Int, cached: Int)] = [:]
        for r in records {
            let k = key(r)
            var entry = grouped[k] ?? (0, 0, 0)
            entry.cost += r.costUSD
            entry.count += 1
            entry.cached += r.cachedTokens
            grouped[k] = entry
        }
        return grouped
            .map { CostBucket(id: $0.key, name: $0.key, cost: $0.value.cost, count: $0.value.count, cachedTokens: $0.value.cached) }
            .sorted { $0.cost > $1.cost }
    }

    static func dailySeries(for records: [UsageRecord]) -> [DailyCostPoint] {
        let cal = Calendar.current
        var grouped: [String: (day: Date, feature: String, cost: Double)] = [:]
        for r in records {
            let day = cal.startOfDay(for: r.at)
            let key = "\(day.timeIntervalSince1970)|\(r.feature)"
            var entry = grouped[key] ?? (day, r.feature, 0)
            entry.cost += r.costUSD
            grouped[key] = entry
        }
        return grouped
            .map { DailyCostPoint(id: $0.key, day: $0.value.day, feature: $0.value.feature, cost: $0.value.cost) }
            .sorted { $0.day < $1.day }
    }
}

// MARK: - Formatting

enum CostFormatter {
    static func usd(_ value: Double) -> String {
        if value == 0  { return "$0.00" }
        if value < 0.01 { return String(format: "$%.4f", value) }
        if value < 1    { return String(format: "$%.3f", value) }
        return String(format: "$%.2f", value)
    }

    static func usdCompact(_ value: Double) -> String {
        if value == 0   { return "$0" }
        if value < 0.001 { return String(format: "$%.4f", value) }
        if value < 1     { return String(format: "$%.3f", value) }
        return String(format: "$%.2f", value)
    }

    static func usdAxis(_ value: Double) -> String {
        if value == 0  { return "$0" }
        if value < 0.01 { return String(format: "$%.3f", value) }
        if value < 1    { return String(format: "$%.2f", value) }
        return String(format: "$%.0f", value)
    }

    static func latency(_ ms: Int) -> String {
        if ms < 1000 { return "\(ms)ms" }
        return String(format: "%.1fs", Double(ms) / 1000)
    }
}
