import Foundation
import Pod0Core

@MainActor
enum ProductSettingsWriterIdentity {
    private static let defaultsKey = "product-settings.writer-id.v1"

    static func current() -> ContentDigest {
        let defaults = UserDefaults.standard
        let identity: String
        if let stored = defaults.string(forKey: defaultsKey), !stored.isEmpty {
            identity = stored
        } else {
            identity = UUID().uuidString.lowercased()
            defaults.set(identity, forKey: defaultsKey)
        }
        return SharedLibraryBootstrap.stableDigest("pod0-product-settings-device:\(identity)")
    }
}
