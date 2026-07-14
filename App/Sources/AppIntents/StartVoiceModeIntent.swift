import AppIntents
import Foundation

// MARK: - StartVoiceModeIntent

/// Single entry point that wires Voice Mode into the system. Once an
/// `AppShortcut` is published from `PodcastrShortcuts` below, this intent
/// is reachable from:
///   - The iPhone 15 Pro+ Action Button (Settings → Action Button →
///     Shortcut → "Talk to my podcasts").
///   - AirPods Pro 2 stem squeeze (Settings → AirPods → "Press and Hold
///     AirPods" → Shortcut).
///   - Lock Screen / Control Center widgets via `ControlWidget`.
///   - Siri ("Hey Siri, talk to my podcasts").
///   - Spotlight search.
///
/// On invocation the intent posts `voiceModeRequestedNotification` and
/// opens the app. The host (`RootView`) observes the notification, switches
/// to the Voice surface, and asks the conversation manager to enter
/// ambient mode. We deliberately keep the intent state-free — it does not
/// hold a reference to any singleton — so it survives between app launches
/// and cold starts.
struct StartVoiceModeIntent: AppIntent {

    static let title: LocalizedStringResource = "Talk to my podcasts"

    static let description = IntentDescription(
        "Open Podcastr and start a hands-free conversation with the agent. Ask about anything you've listened to.",
        categoryName: "Voice"
    )

    /// Bring the app to the foreground so the orb is visible. Without this
    /// the intent runs in the background and the orb wouldn't render.
    static let openAppWhenRun: Bool = true

    @MainActor
    func perform() async throws -> some IntentResult {
        NotificationCenter.default.post(
            name: .voiceModeRequested,
            object: nil
        )
        return .result()
    }
}

// MARK: - Notification

extension Notification.Name {
    /// Posted when the user invokes `StartVoiceModeIntent` (Action Button,
    /// AirPods squeeze, Lock Screen control, Siri, Spotlight). Observed by
    /// `RootView` to switch to Voice mode.
    static let voiceModeRequested = Notification.Name("io.f7z.podcast.voiceModeRequested")
}

// MARK: - AppShortcuts

/// Surfaces `StartVoiceModeIntent` to Shortcuts, Spotlight, and Siri.
///
/// `appShortcuts` is the only place where phrases are localised — the
/// `\(.applicationName)` token is replaced at runtime with the user-facing
/// app name so the same shortcut works for re-branded builds.
struct PodcastrShortcuts: AppShortcutsProvider {

    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: StartVoiceModeIntent(),
            phrases: [
                "Talk to my podcasts in \(.applicationName)",
                "Ask \(.applicationName) about my podcasts",
                "Start a podcast conversation in \(.applicationName)"
            ],
            shortTitle: "Talk to podcasts",
            systemImageName: "waveform.circle.fill"
        )
    }
}
