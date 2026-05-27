import Foundation

// MARK: - Download capability wire vocabulary
//
// Swift mirror of the Rust types in
// `apps/nmp-app-podcast/src/capability/download.rs`. The Rust enums are
// `#[serde(tag = "type", rename_all = "snake_case")]`; the manual
// `Codable` impls below match that wire shape exactly so a JSON string
// produced on one side decodes on the other.
//
// Split out of `DownloadCapability.swift` to keep that file under the
// 300-LOC soft limit (AGENTS.md).

/// Commands Rust dispatches to the iOS download executor.
///
/// Wire shape (Rust side, `serde` tagged on `"type"`, snake_case):
///
/// ```text
/// {"type":"start_download","url":"…","episode_id":"…","expected_bytes":12345}
/// {"type":"pause_download","episode_id":"…"}
/// {"type":"resume_download","episode_id":"…"}
/// {"type":"cancel_download","episode_id":"…"}
/// {"type":"cancel_all"}
/// ```
enum DownloadCommand: Decodable, Equatable {
    case startDownload(url: String, episodeID: String, expectedBytes: UInt64?)
    case pauseDownload(episodeID: String)
    case resumeDownload(episodeID: String)
    case cancelDownload(episodeID: String)
    case cancelAll

    private enum CodingKeys: String, CodingKey {
        case type
        case url
        case episodeID = "episode_id"
        case expectedBytes = "expected_bytes"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "start_download":
            self = .startDownload(
                url: try c.decode(String.self, forKey: .url),
                episodeID: try c.decode(String.self, forKey: .episodeID),
                expectedBytes: try c.decodeIfPresent(UInt64.self, forKey: .expectedBytes))
        case "pause_download":
            self = .pauseDownload(
                episodeID: try c.decode(String.self, forKey: .episodeID))
        case "resume_download":
            self = .resumeDownload(
                episodeID: try c.decode(String.self, forKey: .episodeID))
        case "cancel_download":
            self = .cancelDownload(
                episodeID: try c.decode(String.self, forKey: .episodeID))
        case "cancel_all":
            self = .cancelAll
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: c, debugDescription: "unknown download command: \(type)")
        }
    }
}

/// Events the iOS download executor pushes back to Rust.
///
/// Wire shape (Rust side, `serde` tagged on `"type"`, snake_case):
///
/// ```text
/// {"type":"progress","episode_id":"…","bytes_downloaded":N,"total_bytes":M}
/// {"type":"completed","episode_id":"…","local_path":"…"}
/// {"type":"failed","episode_id":"…","error":"…"}
/// {"type":"cancelled","episode_id":"…"}
/// {"type":"paused","episode_id":"…","bytes_downloaded":N}
/// ```
enum DownloadReport: Encodable, Equatable {
    case progress(episodeID: String, bytesDownloaded: UInt64, totalBytes: UInt64?)
    case completed(episodeID: String, localPath: String)
    case failed(episodeID: String, error: String)
    case cancelled(episodeID: String)
    case paused(episodeID: String, bytesDownloaded: UInt64)

    private enum CodingKeys: String, CodingKey {
        case type
        case episodeID = "episode_id"
        case bytesDownloaded = "bytes_downloaded"
        case totalBytes = "total_bytes"
        case localPath = "local_path"
        case error
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .progress(episodeID, bytesDownloaded, totalBytes):
            try c.encode("progress", forKey: .type)
            try c.encode(episodeID, forKey: .episodeID)
            try c.encode(bytesDownloaded, forKey: .bytesDownloaded)
            // Rust's `#[serde(skip_serializing_if = "Option::is_none")]` omits
            // `total_bytes` entirely when unknown. Mirror that exactly so the
            // wire shape is bit-identical.
            if let totalBytes {
                try c.encode(totalBytes, forKey: .totalBytes)
            }
        case let .completed(episodeID, localPath):
            try c.encode("completed", forKey: .type)
            try c.encode(episodeID, forKey: .episodeID)
            try c.encode(localPath, forKey: .localPath)
        case let .failed(episodeID, error):
            try c.encode("failed", forKey: .type)
            try c.encode(episodeID, forKey: .episodeID)
            try c.encode(error, forKey: .error)
        case let .cancelled(episodeID):
            try c.encode("cancelled", forKey: .type)
            try c.encode(episodeID, forKey: .episodeID)
        case let .paused(episodeID, bytesDownloaded):
            try c.encode("paused", forKey: .type)
            try c.encode(episodeID, forKey: .episodeID)
            try c.encode(bytesDownloaded, forKey: .bytesDownloaded)
        }
    }

    /// Encode to a JSON string. Returns `nil` on the (impossible) serde
    /// failure — callers treat `nil` as "no-op" per D6.
    func jsonString() -> String? {
        guard let data = try? JSONEncoder().encode(self) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
