import Pod0Core

extension AppStateStore {
    func setNewEpisodeNotificationsEnabled(_ enabled: Bool) {
        sharedLibrary?.setNewEpisodeNotificationsEnabled(enabled)
    }

    func applySharedNewEpisodeNotificationSettings(
        _ projection: NewEpisodeNotificationSettingsProjection
    ) {
        newEpisodeNotificationsEnabled = projection.enabled
    }
}
