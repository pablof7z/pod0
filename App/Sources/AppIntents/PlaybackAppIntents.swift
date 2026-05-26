import AppIntents
import Foundation
import os.log

private let intentLog = Logger(subsystem: "io.f7z.podcast", category: "AppIntents")

// MARK: - PlayLatestEpisodeIntent

/// "Play my podcasts" - posts `.playLatestRequested`. `RootView` observes
/// the notification, picks the newest unplayed episode from the store, and
/// hands it to `PlaybackState`.
struct PlayLatestEpisodeIntent: AppIntent {

    static let title: LocalizedStringResource = "Play latest episode"

    static let description = IntentDescription(
        "Play the most recently published unplayed episode from your library.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        NotificationCenter.default.post(name: .playLatestRequested, object: nil)
        intentLog.info("PlayLatestEpisodeIntent: posted playLatestRequested")
        return .result(dialog: "Playing the latest episode.")
    }
}

// MARK: - PausePlaybackIntent

/// "Pause podcast" — posts `.pausePlaybackRequested`. The kernel drops the
/// pause silently when nothing is playing (D6).
struct PausePlaybackIntent: AppIntent {

    static let title: LocalizedStringResource = "Pause podcast"

    static let description = IntentDescription(
        "Pause the currently-playing podcast episode.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        NotificationCenter.default.post(name: .pausePlaybackRequested, object: nil)
        intentLog.info("PausePlaybackIntent: posted pausePlaybackRequested")
        return .result(dialog: "Paused.")
    }
}

// MARK: - ResumePlaybackIntent

/// "Resume podcast" — posts `.resumePlaybackRequested`. `RootView` resumes
/// `PlaybackState`, which replays the last-staged episode.
struct ResumePlaybackIntent: AppIntent {

    static let title: LocalizedStringResource = "Resume podcast"

    static let description = IntentDescription(
        "Resume the last-playing podcast episode.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        NotificationCenter.default.post(name: .resumePlaybackRequested, object: nil)
        intentLog.info("ResumePlaybackIntent: posted resumePlaybackRequested")
        return .result(dialog: "Resuming.")
    }
}

// MARK: - SkipForwardIntent

/// "Skip forward" — posts `.skipForwardRequested`. `PlaybackState.skipForward()`
/// reads the user-configured interval; the intent stays stateless (D0).
struct SkipForwardIntent: AppIntent {

    static let title: LocalizedStringResource = "Skip forward"

    static let description = IntentDescription(
        "Skip the currently-playing podcast episode forward by the configured interval.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        NotificationCenter.default.post(name: .skipForwardRequested, object: nil)
        intentLog.info("SkipForwardIntent: posted skipForwardRequested")
        return .result(dialog: "Skipped forward.")
    }
}

// MARK: - Notification names

extension Notification.Name {
    static let playLatestRequested =
        Notification.Name("io.f7z.podcast.playLatestRequested")
    static let pausePlaybackRequested =
        Notification.Name("io.f7z.podcast.pausePlaybackRequested")
    static let resumePlaybackRequested =
        Notification.Name("io.f7z.podcast.resumePlaybackRequested")
    static let skipForwardRequested =
        Notification.Name("io.f7z.podcast.skipForwardRequested")
}
