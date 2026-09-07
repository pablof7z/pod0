import Foundation
import Pod0Core

extension SharedLibraryBootstrap {
    static func importLegacyProductSettings(
        _ settings: Settings,
        sourceGeneration: UInt64,
        target: URL,
        into facade: Pod0Facade
    ) throws -> ProductSettings {
        let values = try ProductSettingsBridge.values(from: settings)
        let path = target.standardizedFileURL.path
        let projection = try facade.importLegacyProductSettings(
            commandId: stableID("pod0-product-settings-import:\(path):\(sourceGeneration)"),
            sourceGeneration: sourceGeneration,
            writerId: stableDigest("pod0-product-settings-writer:\(path)"),
            values: values
        )
        guard projection.authoritative, let imported = projection.settings else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
        return imported
    }
}
