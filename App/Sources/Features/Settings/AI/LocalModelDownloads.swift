import Foundation

// MARK: - Local model download state + disk helpers
//
// Local model downloads now flow through the unified, kernel-driven download
// queue (DownloadKind.localModel) and the shared DownloadCapability executor —
// the same path episodes use, so models inherit resume / retry / background
// transfer. This file holds the small UI-facing state type and the disk-backed
// "is it downloaded" helpers that replaced the bespoke LocalModelDownloadManager.

/// UI state for a catalog model on the Settings → Providers → Local page.
enum LocalModelState: Equatable, Sendable {
    case notDownloaded
    case downloading(progress: Double)
    case downloaded
    /// Downloaded and currently the resident on-device engine ("In use").
    case active
}

extension LocalModelCatalog {
    /// Whether a model's weights are present on disk, at the canonical location
    /// the unified download executor writes to. Drives which local models are
    /// offered in the per-role selector (only downloaded ones), so "download to
    /// make available" stays honest.
    static func isDownloaded(_ modelID: String) -> Bool {
        FileManager.default.fileExists(
            atPath: DownloadCapability.localModelFileURL(for: modelID).path)
    }
}

extension AppStateStore {
    /// Resolve a catalog model's UI state from the unified download snapshot
    /// (in-flight) and disk (downloaded), tagging the resident engine "In use".
    func localModelState(for spec: LocalModelSpec) -> LocalModelState {
        if let dl = localModelDownloads[spec.id] {
            switch dl.state {
            case "active", "queued", "paused":
                return .downloading(progress: dl.progress)
            default:
                break
            }
        }
        guard LocalModelCatalog.isDownloaded(spec.id) else { return .notDownloaded }
        let inUse = effectiveLocalModelID(state.settings) == spec.id
        return inUse ? .active : .downloaded
    }
}
