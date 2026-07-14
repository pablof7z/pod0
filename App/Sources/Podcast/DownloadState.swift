import Foundation

/// Lifecycle of a single episode's enclosure download.
///
/// Lane 1 (audio engine) and the Library UI both observe this; keep cases
/// stable and additive. Progress is encoded as a `Double` in the 0...1 range.
enum DownloadState: Codable, Sendable, Hashable {
    /// Not requested. Default state for any newly ingested episode.
    case notDownloaded
    /// Queued for download but not yet started (e.g. waiting for Wi-Fi).
    case queued
    /// Currently downloading. `progress` is 0...1; `bytesWritten` may be `nil`
    /// when the URLSession does not yet report `expectedContentLength`.
    case downloading(progress: Double, bytesWritten: Int64?)
    /// Successfully downloaded. `localFileURL` points at the on-disk MP3/MP4.
    /// `byteCount` is the size on disk for storage UI.
    case downloaded(localFileURL: URL, byteCount: Int64)
    /// Download failed; surfaced in Downloads/Failed bucket. `message` is
    /// already user-facing (caller should localize at the source).
    case failed(message: String)
}
