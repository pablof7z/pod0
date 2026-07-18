import Foundation

struct ProductSignalCount: Codable, Sendable, Equatable, Identifiable {
    let name: ProductSignalName
    let outcome: ProductSignalOutcome
    let count: Int

    var id: String { "\(name.rawValue):\(outcome.rawValue)" }
}

struct ProductSignalReport: Codable, Sendable, Equatable {
    let schemaVersion: Int
    let generatedAt: Date
    let signalCount: Int
    let distinctActiveDays: Int
    let activatedAt: Date?
    let counts: [ProductSignalCount]

    init(signals: [ProductSignal], generatedAt: Date = Date()) {
        schemaVersion = ProductSignal.currentSchemaVersion
        self.generatedAt = generatedAt
        signalCount = signals.count
        let calendar = Calendar(identifier: .iso8601)
        distinctActiveDays = Set(signals.map { calendar.startOfDay(for: $0.occurredAt) }).count
        activatedAt = signals.first(where: { $0.name == .firstSubscription })?.occurredAt
        let grouped = Dictionary(grouping: signals) { signal in
            "\(signal.name.rawValue):\(signal.outcome.rawValue)"
        }
        counts = grouped.values.compactMap { group in
            guard let first = group.first else { return nil }
            return ProductSignalCount(
                name: first.name,
                outcome: first.outcome,
                count: group.count
            )
        }.sorted { $0.id < $1.id }
    }
}

struct ProductSignalSnapshot: Sendable, Equatable {
    let isEnabled: Bool
    let anonymousInstallID: UUID
    let signals: [ProductSignal]

    var report: ProductSignalReport { ProductSignalReport(signals: signals) }
}

struct ProductSignalExport: Codable, Sendable, Equatable {
    let schemaVersion: Int
    let privacy: String
    let report: ProductSignalReport
    let signals: [ProductSignal]
}
