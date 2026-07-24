import Foundation
import Pod0Core

extension SharedLibraryClient {
    func setNewEpisodeNotificationsEnabled(_ enabled: Bool) {
        dispatchCoreCommand(.setNewEpisodeNotificationsEnabled(enabled: enabled))
    }

    func publishNewEpisodeNotificationSettings(to store: AppStateStore) {
        let projection: NewEpisodeNotificationSettingsProjection
        if let cachedNewEpisodeNotificationSettings {
            projection = cachedNewEpisodeNotificationSettings
        } else {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .newEpisodeNotificationSettings,
                offset: 0,
                maxItems: 1
            ))
            guard case .newEpisodeNotificationSettings(let value) = envelope.projection else {
                return
            }
            projection = value
            cachedNewEpisodeNotificationSettings = value
        }
        store.applySharedNewEpisodeNotificationSettings(projection)
    }
}
