import Foundation
import Pod0Core

enum CategoryBridge {
    static func inputs(
        categories: [PodcastCategory],
        settings: [UUID: CategorySettings]
    ) -> [CategoryReplacementInput] {
        categories.map { category in
            CategoryReplacementInput(
                categoryId: CategoryId(uuid: category.id),
                name: category.name,
                description: category.description,
                colorHex: category.colorHex,
                origin: .generated,
                podcastIds: category.subscriptionIDs.map(PodcastId.init(uuid:)),
                settings: coreSettings(settings[category.id] ?? .default(for: category.id)),
                generatedAt: UnixTimestampMilliseconds(date: category.generatedAt)
            )
        }
    }

    static func applying(
        _ projection: CategoryAuthorityProjection,
        to state: inout AppState
    ) {
        state.categories = projection.categories.compactMap { category in
            guard let id = category.categoryId.uuid else { return nil }
            return PodcastCategory(
                id: id,
                name: category.name,
                slug: category.slug,
                description: category.description,
                colorHex: category.colorHex,
                subscriptionIDs: category.podcastIds.compactMap(\.uuid),
                generatedAt: category.generatedAt.date,
                model: nil
            )
        }
        state.categorySettings = Dictionary(uniqueKeysWithValues: projection.categories.compactMap {
            category -> (UUID, CategorySettings)? in
            guard let id = category.categoryId.uuid else { return nil }
            return (id, swiftSettings(category.settings, id: id))
        })
    }

    static func coreSettings(_ settings: CategorySettings) -> Pod0Core.CategorySettings {
        Pod0Core.CategorySettings(
            autoDownloadOverride: settings.autoDownloadOverride?.coreValue,
            ragEnabled: settings.ragEnabled,
            notificationsEnabled: settings.notificationsEnabled
        )
    }

    private static func swiftSettings(
        _ settings: Pod0Core.CategorySettings,
        id: UUID
    ) -> CategorySettings {
        CategorySettings(
            categoryID: id,
            autoDownloadOverride: settings.autoDownloadOverride?.swiftValue,
            ragEnabled: settings.ragEnabled,
            notificationsEnabled: settings.notificationsEnabled
        )
    }
}
