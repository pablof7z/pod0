import Foundation
import os

// MARK: - Social native-store migration

private let socialNativeMigrationLog = OSLog(
    subsystem: "io.f7z.podcast",
    category: "SocialNativeMigration"
)

extension AppStateStore {
    /// One-shot migration: seed Rust-owned `notes.json` and `friends.json`
    /// from the legacy Swift `AppState.notes` / `AppState.friends` fields.
    ///
    /// The payload is captured during `AppStateStore.init`, before the first
    /// kernel projection can replace those arrays. The completion flag is set
    /// only after every synchronous dispatch is accepted; a rejected dispatch
    /// leaves the flag unset so the next launch retries the idempotent seed.
    @MainActor
    func migrateSocialNativeStoresToKernel(defaults: UserDefaults = .standard) {
        guard !defaults.bool(forKey: SocialNativeStoreMigration.flagKey) else {
            pendingSocialNativeStoreMigration = nil
            return
        }
        guard let payload = pendingSocialNativeStoreMigration else { return }
        guard let kern = kernel else { return }

        let commands = SocialNativeStoreMigration.commands(from: payload)
        for command in commands {
            let result = kern.dispatch(namespace: command.namespace, body: command.body)
            if case let .failure(message) = result {
                os_log(
                    .error,
                    log: socialNativeMigrationLog,
                    "Social native migration dispatch rejected: %{public}@",
                    message
                )
                return
            }
        }

        SocialNativeStoreMigration.markComplete(defaults: defaults)
        pendingSocialNativeStoreMigration = nil
        os_log(
            .info,
            log: socialNativeMigrationLog,
            "Social native migration complete: %d notes, %d friends",
            payload.notes.count,
            payload.friends.count
        )
    }
}
