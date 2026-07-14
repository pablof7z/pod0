import Foundation

// MARK: - WhatsNewEntry
//
// One entry == one user-facing change. Identity and ordering are both
// driven by `shippedAt` — entries are surfaced when their timestamp is
// strictly newer than the user's `lastSeenAt` marker. Timestamps must
// be unique across entries (use the next minute if you need to
// disambiguate).

struct WhatsNewEntry: Decodable, Sendable, Identifiable, Equatable {
    let shippedAt: Date
    let lines: [String]

    var id: Date { shippedAt }

    private enum CodingKeys: String, CodingKey {
        case shippedAt = "shipped_at"
        case lines
    }
}

// MARK: - Decoded payload shape

private struct WhatsNewPayload: Decodable {
    let schemaVersion: Int
    let entries: [WhatsNewEntry]

    private enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version"
        case entries
    }
}

// MARK: - WhatsNewService
//
// Loads the bundled `whats-new.json` and answers two questions:
//   1) Which entries have shipped since the user's last-seen marker?
//   2) Persists the "I've seen up to timestamp X" marker so the same
//      content doesn't re-surface on every cold launch.
//
// The marker is a timestamp (ISO-8601, stored in `UserDefaults.standard`
// under `whatsNew.lastSeenAt`).
//
// First-launch semantics: when no marker exists at all, the marker is
// silently seeded to the newest entry's `shippedAt`. The user sees an
// empty sheet on first install — i.e. NO sheet is shown, because there's
// nothing newer than "everything that already shipped." From the next
// build forward, any newly appended entry surfaces.

@MainActor
enum WhatsNewService {

    // MARK: Constants

    static let lastSeenAtKey = "whatsNew.lastSeenAt"

    private static let resourceName = "whats-new"
    private static let resourceExtension = "json"

    // MARK: Loading

    /// Loads `whats-new.json` from the app bundle. Empty when missing or
    /// malformed — the sheet just won't surface in that case (we'd rather
    /// fail closed than crash on launch).
    static func loadEntries(bundle: Bundle = .main) -> [WhatsNewEntry] {
        guard let url = bundle.url(forResource: resourceName, withExtension: resourceExtension) else {
            return []
        }
        do {
            let data = try Data(contentsOf: url)
            return try decode(data)
        } catch {
            return []
        }
    }

    /// Internal decode helper — exposed so tests can feed a JSON literal
    /// through it without depending on the bundled file.
    static func decode(_ data: Data) throws -> [WhatsNewEntry] {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let payload = try decoder.decode(WhatsNewPayload.self, from: data)
        return payload.entries
    }

    // MARK: Marker

    /// The "I've seen everything up through this timestamp" marker, read
    /// from `UserDefaults.standard`. `nil` on a brand-new install (before
    /// `seedIfNeeded` has run).
    static var lastSeenAt: Date? {
        guard let s = UserDefaults.standard.string(forKey: lastSeenAtKey),
              !s.isEmpty else {
            return nil
        }
        return Self.iso8601.date(from: s)
    }

    /// Persist the marker. Call when the user dismisses the sheet, with the
    /// newest visible entry's `shippedAt`.
    static func markSeen(at date: Date) {
        UserDefaults.standard.set(Self.iso8601.string(from: date), forKey: lastSeenAtKey)
    }

    /// On a fresh install (no marker yet), silently seed the marker to the
    /// newest entry's `shippedAt` so the user doesn't see "the entire
    /// changelog ever" as their first impression of the app. Future
    /// entries appended after this build will still surface.
    ///
    /// Idempotent: a no-op once any marker is present.
    static func seedIfNeeded(entries: [WhatsNewEntry]? = nil) {
        let defaults = UserDefaults.standard
        if defaults.string(forKey: lastSeenAtKey) != nil { return }
        let sorted = (entries ?? loadEntries()).sorted { $0.shippedAt > $1.shippedAt }
        if let newest = sorted.first {
            markSeen(at: newest.shippedAt)
        }
    }

    // MARK: Diff

    /// Entries strictly newer than `lastSeenAt`, in newest-first order.
    ///
    /// `lastSeenAt == nil` returns `[]` rather than the full changelog —
    /// `seedIfNeeded` is responsible for seeding the marker on fresh
    /// installs; if it hasn't run yet, the caller should not surface
    /// anything.
    static func unseenEntries(
        lastSeenAt: Date?,
        entries: [WhatsNewEntry]? = nil
    ) -> [WhatsNewEntry] {
        guard let marker = lastSeenAt else { return [] }
        let all = entries ?? loadEntries()
        return all
            .filter { $0.shippedAt > marker }
            .sorted { $0.shippedAt > $1.shippedAt }
    }

    // MARK: Helpers

    private static let iso8601: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()
}
