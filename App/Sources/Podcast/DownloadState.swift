import Foundation

/// Stable local-audio artifact projection. Pending, running, retry, failure,
/// and cancellation live exclusively in JobStore.
enum DownloadState: Codable, Sendable, Hashable {
    /// No verified current local artifact.
    case notDownloaded
    /// Verified current local artifact selected by ArtifactRepository.
    case downloaded(localFileURL: URL, byteCount: Int64)
}

extension DownloadState {
    var localFileURL: URL? {
        guard case .downloaded(let url, _) = self else { return nil }
        return url
    }

    var byteCount: Int64? {
        guard case .downloaded(_, let byteCount) = self else { return nil }
        return byteCount
    }

    var isAvailable: Bool { localFileURL != nil }
}
