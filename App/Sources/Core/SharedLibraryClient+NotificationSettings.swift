import Foundation
import Pod0Core

extension SharedLibraryClient {
    func setNewEpisodeNotificationsEnabled(_ enabled: Bool) {
        dispatchCoreCommand(.setNewEpisodeNotificationsEnabled(enabled: enabled))
    }

    func publishNewEpisodeNotificationSettings(to store: AppStateStore) {
        guard let projection = cachedNewEpisodeNotificationSettings else { return }
        store.applySharedNewEpisodeNotificationSettings(projection)
    }

    nonisolated static func loadNewEpisodeNotificationSettings(
        facade: Pod0Facade
    ) -> NewEpisodeNotificationSettingsProjection? {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .newEpisodeNotificationSettings,
            offset: 0,
            maxItems: 1
        ))
        guard case .newEpisodeNotificationSettings(let value) = envelope.projection else {
            return nil
        }
        return value
    }
}
