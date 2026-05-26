import AppIntents
import Foundation
import os.log

// MARK: - PodcastAppIntents
//
// iOS `AppIntents` performers — the M11 platform-integration surface that
// lets Siri / Shortcuts / Spotlight / Action Button drive playback. Each
// intent is a thin shell over `KernelModel.dispatch`: per D7, the iOS side
// only **translates** the user's intent into a kernel action and then steps
// out of the way. Policy ("which episode is `latest`?", "what does `resume`
// load when nothing was playing?") lives in the Rust kernel.
//
// Architecture:
//   - All intents are `@MainActor` because `KernelModel.dispatch` and
//     `KernelModel.podcastSnapshot` are `@MainActor`-isolated.
//   - The intents resolve the live `KernelModel` via the static `weak`
//     reference published by `KernelModel.init`. An `AppIntent` cannot
//     reach `@Environment` — it runs on the OS-driven extension queue
//     with no SwiftUI view tree attached.
//   - `openAppWhenRun = false` everywhere. Media-control intents that
//     force-foreground are bad UX (a Siri "Pause" should not yank the
//     app into view). The exception is `StartVoiceModeIntent` (defined
//     in the legacy `Podcastr` target) which needs the orb visible.
//   - Each intent maps to one Rust action id (`podcast.player.*`).
//     There is no `podcast.player.skip_forward` in the Rust contract;
//     skip is expressed as a `seek` to `position + 30`. There is no
//     `podcast.player.play_latest` either; iOS selects the episode
//     from `snapshot.library` and dispatches the existing `play`.
//
// File-length budget: four intents + one provider ≈ 200 lines. If a
// fifth intent lands and we cross 300 lines, split per-intent into
// individual files.

private let intentLog = Logger(subsystem: "io.f7z.podcast", category: "AppIntents")

// MARK: - PlayLatestEpisodeIntent

/// "Play the latest podcast episode" — selects the most recently published
/// episode across all subscribed podcasts that still have an unplayed
/// backlog and dispatches `podcast.player.play` with its id.
///
/// Selection policy (iOS-side, because no Rust action carries it yet):
///   1. Filter `snapshot.library` to podcasts with `unplayedCount > 0`.
///   2. Flatten their episode rows.
///   3. Pick the episode with the highest `publishedAt`.
///
/// If nothing matches (empty library, every show fully played, kernel
/// snapshot not yet ticked) the intent reports a no-op result — Siri
/// reads back the dialog and stops. D6: never throws.
struct PlayLatestEpisodeIntent: AppIntent {

    static let title: LocalizedStringResource = "Play latest episode"

    static let description = IntentDescription(
        "Play the most recently published unplayed episode from your library.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard let model = KernelModel.shared else {
            intentLog.error("PlayLatestEpisodeIntent: KernelModel.shared is nil")
            return .result(dialog: "Podcastr isn't ready yet. Try again in a moment.")
        }
        guard let episode = Self.selectLatestUnplayedEpisode(in: model.library) else {
            intentLog.info("PlayLatestEpisodeIntent: no unplayed episodes in library")
            return .result(dialog: "You're all caught up — no new episodes to play.")
        }
        model.dispatch(
            namespace: "podcast.player",
            body: ["op": "play", "episode_id": episode.id]
        )
        intentLog.info("PlayLatestEpisodeIntent: dispatched play for episode_id=\(episode.id, privacy: .public)")
        let title = episode.title
        return .result(dialog: "Playing \(title).")
    }

    /// Latest-published episode across podcasts with an unplayed backlog.
    /// Tied published-at values fall back to the kernel's natural order.
    static func selectLatestUnplayedEpisode(in library: [PodcastSummary]) -> EpisodeSummary? {
        library
            .filter { $0.unplayedCount > 0 }
            .flatMap { $0.episodes }
            .max(by: { ($0.publishedAt ?? 0) < ($1.publishedAt ?? 0) })
    }
}

// MARK: - PausePlaybackIntent

/// "Pause podcast" — dispatches `podcast.player.pause`. The kernel
/// drops the pause silently when nothing is playing (D6).
struct PausePlaybackIntent: AppIntent {

    static let title: LocalizedStringResource = "Pause podcast"

    static let description = IntentDescription(
        "Pause the currently-playing podcast episode.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard let model = KernelModel.shared else {
            intentLog.error("PausePlaybackIntent: KernelModel.shared is nil")
            return .result(dialog: "Podcastr isn't ready yet.")
        }
        model.dispatch(namespace: "podcast.player", body: ["op": "pause"])
        intentLog.info("PausePlaybackIntent: dispatched pause")
        return .result(dialog: "Paused.")
    }
}

// MARK: - ResumePlaybackIntent

/// "Resume podcast" — re-dispatches `podcast.player.play` for the
/// episode id currently held in `nowPlaying`. If no episode is loaded
/// (cold start, kernel snapshot not yet ticked) the intent falls back
/// to the same selection used by `PlayLatestEpisodeIntent` — matching
/// what a user expects from "resume" on a fresh device.
struct ResumePlaybackIntent: AppIntent {

    static let title: LocalizedStringResource = "Resume podcast"

    static let description = IntentDescription(
        "Resume the last-playing podcast episode.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard let model = KernelModel.shared else {
            intentLog.error("ResumePlaybackIntent: KernelModel.shared is nil")
            return .result(dialog: "Podcastr isn't ready yet.")
        }
        if let epId = model.podcastSnapshot?.nowPlaying?.episodeId {
            model.dispatch(
                namespace: "podcast.player",
                body: ["op": "play", "episode_id": epId]
            )
            intentLog.info("ResumePlaybackIntent: dispatched play for active episode_id=\(epId, privacy: .public)")
            return .result(dialog: "Resuming.")
        }
        if let episode = PlayLatestEpisodeIntent.selectLatestUnplayedEpisode(in: model.library) {
            model.dispatch(
                namespace: "podcast.player",
                body: ["op": "play", "episode_id": episode.id]
            )
            intentLog.info("ResumePlaybackIntent: no active episode; fell back to latest=\(episode.id, privacy: .public)")
            let title = episode.title
            return .result(dialog: "Playing \(title).")
        }
        intentLog.info("ResumePlaybackIntent: nothing to resume and library is empty")
        return .result(dialog: "Nothing to resume.")
    }
}

// MARK: - SkipForwardIntent

/// "Skip forward 30 seconds" — dispatches `podcast.player.skip_forward`
/// with `secs: 30`. The Rust kernel reads the live `PlayerActor` position
/// and emits an absolute seek, so iOS never needs to know the current time
/// (D0 — policy in Rust).
struct SkipForwardIntent: AppIntent {

    static let title: LocalizedStringResource = "Skip forward 30 seconds"

    static let description = IntentDescription(
        "Skip the currently-playing podcast episode forward by 30 seconds.",
        categoryName: "Playback"
    )

    static let openAppWhenRun: Bool = false

    @MainActor
    func perform() async throws -> some IntentResult & ProvidesDialog {
        guard let model = KernelModel.shared else {
            intentLog.error("SkipForwardIntent: KernelModel.shared is nil")
            return .result(dialog: "Podcastr isn't ready yet.")
        }
        guard model.podcastSnapshot?.nowPlaying != nil else {
            intentLog.info("SkipForwardIntent: no active episode; dropping skip")
            return .result(dialog: "Nothing is playing.")
        }
        model.dispatch(
            namespace: "podcast.player",
            body: ["op": "skip_forward", "secs": 30.0]
        )
        intentLog.info("SkipForwardIntent: dispatched skip_forward 30s")
        return .result(dialog: "Skipped forward.")
    }
}
