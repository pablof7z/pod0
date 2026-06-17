import AppIntents
import Foundation
import os.log

private let intentLog = Logger(subsystem: "io.f7z.podcast", category: "AppIntents")

// MARK: - PausePlaybackIntent

/// "Pause podcast" — dispatches the Rust-owned player action directly.
struct PausePlaybackIntent: AppIntent {

    static let title: LocalizedStringResource = "Pause podcast"

    static let description = IntentDescription(
        "Pause the currently-playing podcast episode.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        let dialog = dispatchPlaybackIntent(
            namespace: "podcast.player",
            body: ["op": "pause"],
            successDialog: "Paused."
        )
        return .result(dialog: IntentDialog(stringLiteral: dialog))
    }
}

// MARK: - ResumePlaybackIntent

/// "Resume podcast" — dispatches the Rust-owned Siri resume policy. Rust
/// resumes the staged episode when present, otherwise it selects the latest
/// unplayed episode.
struct ResumePlaybackIntent: AppIntent {

    static let title: LocalizedStringResource = "Resume podcast"

    static let description = IntentDescription(
        "Resume the last-playing podcast episode.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        let dialog = dispatchPlaybackIntent(
            namespace: "podcast.siri",
            body: ["op": "resume"],
            successDialog: "Resuming."
        )
        return .result(dialog: IntentDialog(stringLiteral: dialog))
    }
}

// MARK: - SkipForwardIntent

/// "Skip forward" — dispatches the Rust-owned relative seek. Rust reads the
/// user-configured interval when the intent supplies no explicit seconds.
struct SkipForwardIntent: AppIntent {

    static let title: LocalizedStringResource = "Skip forward"

    static let description = IntentDescription(
        "Skip the currently-playing podcast episode forward by the configured interval.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        let dialog = dispatchPlaybackIntent(
            namespace: "podcast.player",
            body: ["op": "skip_forward"],
            successDialog: "Skipped forward."
        )
        return .result(dialog: IntentDialog(stringLiteral: dialog))
    }
}

// MARK: - Kernel dispatch

@MainActor
private func dispatchPlaybackIntent(
    namespace: String,
    body: [String: Any],
    successDialog: String
) -> String {
    guard let kernel = KernelModel.shared else {
        intentLog.error("Playback AppIntent invoked without a live KernelModel")
        return "Open Pod0 first, then try again."
    }

    switch kernel.dispatch(namespace: namespace, body: body) {
    case .accepted:
        intentLog.info("Playback AppIntent dispatched \(namespace, privacy: .public)")
        return successDialog
    case .failure(let message):
        intentLog.error("Playback AppIntent rejected: \(message, privacy: .public)")
        return message
    }
}
