import Foundation

extension Persistence {

    /// The App Group suite name, resolved from the target's build setting.
    static var appGroupIdentifier: String {
        Bundle.main.object(forInfoDictionaryKey: "AppGroupIdentifier") as? String
            ?? "group.com.podcastr.app"
    }

    /// Retained only for the one-shot legacy state migration.
    static var appGroupDefaults: UserDefaults {
        UserDefaults(suiteName: appGroupIdentifier) ?? .standard
    }

    /// Production state location inside the shared App Group container.
    static var appGroupStateFileURL: URL {
        let manager = FileManager.default
        let base: URL
        if let groupContainer = manager.containerURL(
            forSecurityApplicationGroupIdentifier: appGroupIdentifier
        ) {
            base = groupContainer.appendingPathComponent(
                "Library/Application Support",
                isDirectory: true
            )
        } else {
            base = (try? manager.url(
                for: .cachesDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            )) ?? URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
        }
        return base.appendingPathComponent("podcastr-state.v1.json", isDirectory: false)
    }

    static func episodeStoreURL(for stateFileURL: URL) -> URL {
        let baseName = stateFileURL.deletingPathExtension().lastPathComponent
        return stateFileURL
            .deletingLastPathComponent()
            .appendingPathComponent("\(baseName).episodes.sqlite", isDirectory: false)
    }
}
