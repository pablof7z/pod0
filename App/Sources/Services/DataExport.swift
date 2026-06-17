import Foundation

// MARK: - DataExport
//
// Pure, side-effect-free helpers for exporting the user's `AppState` as a
// portable JSON document. The view layer composes these into a `ShareLink`.
//
// Inspired by win-the-day-app's `FullBackupManager` (payload + version envelope)
// and cut-tracker's `CSVExporter` (filename + suggested file shape).
//
// SECURITY: This export must never include secrets. We deliberately route
// through `AppState` only (which holds non-secret connection metadata) and
// strip the legacy OpenRouter API key field before serialization. Keychain
// values (OpenRouter, ElevenLabs, Nostr private key) are NOT exported.

enum DataExport {

    /// Versioned envelope around `AppState`. Bumping `schemaVersion` lets
    /// future imports detect and migrate older exports.
    struct Payload: Codable, Sendable {
        var schemaVersion: Int
        var generatedAt: Date
        var appVersion: String?
        var buildNumber: String?
        var sourceBundleIdentifier: String?
        var state: AppState
        var memoryFacts: [MemoryFact]
    }

    static let currentSchemaVersion = 1

    private static let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.prettyPrinted, .sortedKeys]
        return e
    }()

    // ISO8601DateFormatter is thread-safe for reads after setup — nonisolated(unsafe) suppresses
    // the Swift 6 Sendable warning without changing runtime behaviour.
    private nonisolated(unsafe) static let iso8601 = ISO8601DateFormatter()

    private static let filenameDateFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd-HHmm"
        f.timeZone = TimeZone(identifier: "UTC")
        f.locale = Locale(identifier: "en_US_POSIX")
        return f
    }()

    // MARK: - Build

    /// Returns a redacted copy of `state` safe for export.
    ///
    /// No secret-shaped fields are persisted in `AppState` today — all
    /// credentials live in the Keychain — so this is currently an identity
    /// copy. It is kept as the single redaction seam: any future
    /// secret-shaped persisted field must be cleared here before export.
    static func redactedState(from state: AppState) -> AppState {
        state
    }

    /// Builds the export payload from the live in-memory state.
    static func makePayload(
        from state: AppState,
        memoryFacts: [MemoryFact] = [],
        now: Date = Date()
    ) -> Payload {
        let info = Bundle.main.infoDictionary
        return Payload(
            schemaVersion: currentSchemaVersion,
            generatedAt: now,
            appVersion: info?["CFBundleShortVersionString"] as? String,
            buildNumber: info?["CFBundleVersion"] as? String,
            sourceBundleIdentifier: Bundle.main.bundleIdentifier,
            state: redactedState(from: state),
            memoryFacts: memoryFacts
        )
    }

    /// Encodes the payload to pretty-printed UTF-8 JSON.
    static func encode(_ payload: Payload) throws -> Data {
        try encoder.encode(payload)
    }

    /// Suggested filename, e.g. `Pod0-Export-2026-05-05-1430.json`.
    static func suggestedFilename(at date: Date = Date()) -> String {
        "Pod0-Export-\(filenameDateFormatter.string(from: date)).json"
    }

    // MARK: - Write

    /// Writes the encoded payload to a fresh file in the temporary directory
    /// and returns its URL, suitable for `ShareLink(item:)`.
    static func writeTemporaryFile(_ data: Data, filename: String) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(filename)
        try data.write(to: url, options: [.atomic])
        return url
    }

    /// Convenience: build payload, encode, and write a temp file in one shot.
    static func writeExport(
        of state: AppState,
        memoryFacts: [MemoryFact] = [],
        now: Date = Date()
    ) throws -> URL {
        let payload = makePayload(from: state, memoryFacts: memoryFacts, now: now)
        let data = try encode(payload)
        let filename = suggestedFilename(at: now)
        return try writeTemporaryFile(data, filename: filename)
    }

    // MARK: - Stats (UI helpers)

    /// Counts of non-deleted records in `state`, used for the export preview.
    struct Stats: Sendable, Hashable {
        var subscriptions: Int
        var episodes: Int
        var notes: Int
        var friends: Int
        var memories: Int
        var agentActivity: Int

        var totalRecords: Int {
            subscriptions + episodes + notes + friends + memories + agentActivity
        }
    }

    static func stats(
        for state: AppState,
        subscriptionCount: Int? = nil,
        episodeCount: Int? = nil,
        memoryFactsCount: Int = 0
    ) -> Stats {
        Stats(
            subscriptions: subscriptionCount ?? state.subscriptions.count,
            episodes: episodeCount ?? state.episodes.count,
            notes: state.notes.filter { !$0.deleted }.count,
            friends: state.friends.count,
            memories: memoryFactsCount,
            agentActivity: state.agentActivity.count
        )
    }
}
