import Foundation
import Pod0Core

extension SharedLibraryBootstrap {
    static func importLegacyCategories(
        from state: AppState,
        into facade: Pod0Facade
    ) throws -> CategoryAuthorityProjection {
        let projection = try facade.importLegacyCategories(
            commandId: stableID(
                "pod0-category-import:\(state.persistenceGeneration)"
            ),
            sourceGeneration: state.persistenceGeneration,
            categories: CategoryBridge.inputs(
                categories: state.categories,
                settings: state.categorySettings
            )
        )
        guard projection.authoritative, !projection.truncated else {
            throw SharedLibraryBootstrapError.verificationFailed
        }
        return projection
    }
}
