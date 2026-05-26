import Foundation

// MARK: - WhatsNewEntry

struct WhatsNewEntry: Decodable, Sendable, Identifiable, Equatable {
    let shippedAt: Date
    let lines: [String]

    var id: Date { shippedAt }

    private enum CodingKeys: String, CodingKey {
        case shippedAt = "shipped_at"
        case lines
    }
}

// MARK: - WhatsNewService

@MainActor
enum WhatsNewService {

    static let lastSeenAtKey = "whatsNew.lastSeenAt"

    static func loadEntries(bundle: Bundle = .main) -> [WhatsNewEntry] {
        guard let url = bundle.url(forResource: "whats-new", withExtension: "json") else { return [] }
        guard let data = try? Data(contentsOf: url) else { return [] }
        return (try? decode(data)) ?? []
    }

    static func decode(_ data: Data) throws -> [WhatsNewEntry] {
        struct Payload: Decodable {
            let entries: [WhatsNewEntry]
        }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(Payload.self, from: data).entries
    }

    static var lastSeenAt: Date? {
        guard let s = UserDefaults.standard.string(forKey: lastSeenAtKey), !s.isEmpty else { return nil }
        return iso8601.date(from: s)
    }

    static func markSeen(at date: Date) {
        UserDefaults.standard.set(iso8601.string(from: date), forKey: lastSeenAtKey)
    }

    static func seedIfNeeded(entries: [WhatsNewEntry]? = nil) {
        guard UserDefaults.standard.string(forKey: lastSeenAtKey) == nil else { return }
        let sorted = (entries ?? loadEntries()).sorted { $0.shippedAt > $1.shippedAt }
        if let newest = sorted.first { markSeen(at: newest.shippedAt) }
    }

    static func unseenEntries(lastSeenAt: Date?, entries: [WhatsNewEntry]? = nil) -> [WhatsNewEntry] {
        guard let marker = lastSeenAt else { return [] }
        return (entries ?? loadEntries())
            .filter { $0.shippedAt > marker }
            .sorted { $0.shippedAt > $1.shippedAt }
    }

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()
}
