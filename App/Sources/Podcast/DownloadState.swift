import Foundation

/// Stable local-audio artifact projection. Pending, running, retry, failure,
/// and cancellation live exclusively in JobStore.
enum DownloadState: Codable, Sendable, Hashable {
    /// No verified current local artifact.
    case notDownloaded
    /// Verified current local artifact selected by ArtifactRepository.
    case downloaded(localFileURL: URL, byteCount: Int64)
}
