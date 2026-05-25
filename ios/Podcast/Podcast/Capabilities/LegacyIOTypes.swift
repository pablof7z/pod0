import Foundation

// MARK: - Legacy I/O capability — wire types
//
// Split out of `LegacyIOCapability.swift` to keep both files under the
// 300-LOC soft cap (`Plans/nmp-migration/06-cross-cutting.md` §5). The
// request enum + result envelope here are the *only* shapes the kernel
// sees over the capability boundary; they live alongside the capability
// implementation, not in a generic location, because the namespace is
// iOS-only — the new app's Android/web targets stub this capability out.

/// Request the kernel sends to read a legacy bytestream or to check/set
/// the migration sentinel. Decoded out of `CapabilityRequest.payloadJSON`.
enum LegacyIORequest: Decodable, Equatable {
    /// Read the legacy state JSON file. First tries the App Group file at
    /// `Library/Application Support/podcastr-state.v1.json`; falls back to
    /// the legacy `UserDefaults(suiteName: group).data(forKey:
    /// "podcastr.state.v1")` blob if the file is absent.
    case readStateJson
    /// Read the legacy episode SQLite sidecar. Returns the raw `.sqlite`
    /// bytes the kernel can hand to `podcast-core::migration::from_episode_db`.
    case readEpisodeDb
    /// List every per-episode audit log file the legacy app wrote.
    case listAuditLogs
    /// Read one episode's audit log JSON.
    case readAuditLog(episodeID: String)
    /// Check whether the `pcst.migration.v1.done` sentinel is set.
    case migrationDoneRead
    /// Set the `pcst.migration.v1.done` sentinel. Idempotent.
    case migrationDoneSet

    private enum CodingKeys: String, CodingKey {
        case op
        case episodeID = "episode_id"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let op = try c.decode(String.self, forKey: .op)
        switch op {
        case "read_state_json":
            self = .readStateJson
        case "read_episode_db":
            self = .readEpisodeDb
        case "list_audit_logs":
            self = .listAuditLogs
        case "read_audit_log":
            let id = try c.decode(String.self, forKey: .episodeID)
            self = .readAuditLog(episodeID: id)
        case "migration_done_read":
            self = .migrationDoneRead
        case "migration_done_set":
            self = .migrationDoneSet
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .op, in: c,
                debugDescription: "unknown legacy_io op: \(op)")
        }
    }
}

/// Result envelope payload. `status` is the only non-optional discriminator;
/// the rest are populated per-op. D6: failure is a `status == "error"`
/// value, never a Swift throw.
///
/// Binary blobs (`read_state_json`, `read_episode_db`, `read_audit_log`)
/// ride in `dataBase64` so JSON encoding is lossless — every byte of the
/// legacy `.sqlite`/`.json` file round-trips into Rust's
/// `serde_json::from_slice`.
struct LegacyIOResult: Codable, Equatable {
    let status: String          // "ok" | "not_found" | "error"
    let dataBase64: String?     // populated by read_* ops on success
    let episodeIDs: [String]?   // populated by list_audit_logs
    let done: Bool?             // populated by migration_done_read/_set
    let message: String?        // human-readable diagnostic for error
    let source: String?         // diagnostic — "file" / "user_defaults" / "missing"

    enum CodingKeys: String, CodingKey {
        case status
        case dataBase64 = "data_base64"
        case episodeIDs = "episode_ids"
        case done
        case message
        case source
    }

    static func ok(dataBase64: String? = nil,
                   episodeIDs: [String]? = nil,
                   done: Bool? = nil,
                   source: String? = nil) -> LegacyIOResult {
        LegacyIOResult(status: "ok", dataBase64: dataBase64,
                       episodeIDs: episodeIDs, done: done,
                       message: nil, source: source)
    }
    static let notFound = LegacyIOResult(
        status: "not_found", dataBase64: nil, episodeIDs: nil,
        done: nil, message: nil, source: nil)
    static func error(_ message: String) -> LegacyIOResult {
        LegacyIOResult(status: "error", dataBase64: nil, episodeIDs: nil,
                       done: nil, message: message, source: nil)
    }
}
