import Foundation
import MediaPlayer
import Observation
import SwiftUI
import os.log

// MARK: - AutoSnipController
//
// Dispatches an autosnip request for the current playhead. Rust owns the clip
// window, transcript refinement, pending-transcript retry, and projection.
// Triggered three ways:
//
//   1. **Lock-screen / Control Center** via `MPRemoteCommandCenter.bookmarkCommand`
//      — a dedicated MPFeedbackCommand, distinct from the play/pause and skip
//      commands `NowPlayingCenter` already wires. Multiple targets per command
//      are safe; we only own this one.
//   2. **In-app button** (`AutoSnipButton`) on the player controls row — this is
//      the universal fallback. iOS does not expose AirPods double-tap or wired
//      headphone middle-button as a discrete remote command, so the button is
//      the reliable trigger surface on iPhone.
//   3. **Programmatic** — siri / agent / future surfaces call `captureSnip(source:)`
//      directly.
//
// State is intentionally tiny: the controller doesn't own playback or store
// — it pulls them in at call time. Singleton lifetime so the bookmark command
// target survives view recomposition.

@MainActor
@Observable
final class AutoSnipController {

    // MARK: - Singleton

    static let shared = AutoSnipController()

    // MARK: - Logger

    nonisolated private static let logger = Logger.app("AutoSnipController")

    // MARK: - Tunables

    // MARK: - Wiring

    /// Live playback handle. Wired once by `RootView` (or whichever owner
    /// holds the engine) so the controller can read the playhead from any
    /// trigger surface without owning the engine itself.
    var playback: PlaybackState?
    /// Live state-store handle. Same wiring story as `playback`.
    var store: AppStateStore?

    // MARK: - UI surface

    /// Last captured snip — the toast banner observes this and animates in.
    /// Clears itself after `bannerVisibleSeconds` so back-to-back snips each
    /// retrigger the toast cleanly.
    private(set) var lastCapture: CaptureResult?

    /// Bumped on every successful capture. The banner watches this so an
    /// identical-payload back-to-back snip still re-fires the animation.
    private(set) var captureGeneration: Int = 0

    /// Set to `true` when a snip / quote action needs a user-facing provider
    /// setup hint. The banner clears this back to `false` after showing once
    /// (also persists to UserDefaults so the hint doesn't re-fire across
    /// sessions).
    var noLLMKeyHintPending: Bool = false

    static let bannerVisibleSeconds: TimeInterval = 1.5

    struct CaptureResult: Hashable, Identifiable {
        let id: UUID
        let clipID: UUID
        let episodeID: UUID
        let createdAt: Date
        let summary: String
    }

    // MARK: - Init / wiring

    private var didWireRemote = false

    private init() {}

    /// Idempotent. Called from `RootView.onAppear`.
    func attach(playback: PlaybackState, store: AppStateStore) {
        self.playback = playback
        self.store = store
        wireRemoteCommandIfNeeded()
    }

    private func wireRemoteCommandIfNeeded() {
        guard !didWireRemote else { return }
        didWireRemote = true
        let center = MPRemoteCommandCenter.shared()
        let bookmark = center.bookmarkCommand
        bookmark.isEnabled = true
        bookmark.localizedTitle = "Snip last 30s"
        bookmark.addTarget { [weak self] _ in
            guard let self else { return .commandFailed }
            let captured = self.captureSnip(source: .auto)
            return captured == nil ? .noActionableNowPlayingItem : .success
        }
        Self.logger.debug("AutoSnipController: bookmarkCommand wired")
    }

    // MARK: - Capture

    /// Capture a snip from the live playhead. Returns `true` on dispatch
    /// success, or `nil` when there's nothing to capture (no episode loaded,
    /// no store attached, etc.).
    @discardableResult
    func captureSnip(source: Clip.Source = .touch) -> Bool? {
        guard let playback, let store, let episode = playback.episode else {
            Self.logger.notice("captureSnip: no episode / playback not attached")
            return nil
        }
        let now = playback.currentTime
        guard let clipID = store.kernelAutoSnip(episodeID: episode.id, positionSecs: now, source: source) else {
            Self.logger.notice("captureSnip: kernel not attached")
            return nil
        }

        Haptics.success()
        lastCapture = CaptureResult(
            id: UUID(),
            clipID: clipID,
            episodeID: episode.id,
            createdAt: Date(),
            summary: "Snipped · refining in kernel"
        )
        captureGeneration &+= 1
        Self.logger.info(
            "dispatched autosnip episode=\(episode.id, privacy: .public) position=\(now, privacy: .public) source=\(String(describing: source), privacy: .public)"
        )

        return true
    }

    /// Hand-off the caller can invoke 1.5s after a capture — clears
    /// `lastCapture` so the toast disappears even if no new snip arrives.
    func dismissBanner(for captureID: UUID) {
        if lastCapture?.id == captureID {
            lastCapture = nil
        }
    }

}
