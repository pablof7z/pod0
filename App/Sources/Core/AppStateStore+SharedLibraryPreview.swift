import Foundation

#if DEBUG
extension AppStateStore {
    /// Builds an isolated legacy-import fixture for SwiftUI previews. Release
    /// code has no writer or convenience initializer for listening state.
    static func previewStore(importing state: AppState, name: String) -> AppStateStore {
        let persistence = Persistence(
            fileURL: FileManager.default.temporaryDirectory.appendingPathComponent(
                "pod0-\(name)-preview-\(UUID().uuidString).json"
            )
        )
        _ = persistence.write(state, revision: 1)
        return AppStateStore(
            persistence: persistence,
            startSubscriptionRefresh: false
        )
    }
}
#endif
