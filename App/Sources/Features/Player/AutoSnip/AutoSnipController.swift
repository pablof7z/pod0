import Foundation
import MediaPlayer
import Observation
import SwiftUI
import os.log

// MARK: - AutoSnipController
//
// Captures the last 30 seconds (+5s margin forward) of the currently playing
// episode as a `Clip`. Triggered three ways:
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

    /// How far back from the playhead to start the clip.
    static let lookbackSeconds: TimeInterval = 30
    /// Forward margin so the user catches the tail of the moment they wanted.
    static let leadSeconds: TimeInterval = 5

    // MARK: - Wiring

    /// Live playback handle. Wired once by `RootView` (or whichever owner
    /// holds the engine) so the controller can read the playhead from any
    /// trigger surface without owning the engine itself.
    var playback: PlaybackState?
    /// Live state-store handle. Same wiring story as `playback`.
    var store: AppStateStore?
    var transcriptReader: any TranscriptReading = UnavailableTranscriptReader.shared

    // MARK: - UI surface

    /// Last captured snip — the toast banner observes this and animates in.
    /// Clears itself after `bannerVisibleSeconds` so back-to-back snips each
    /// retrigger the toast cleanly.
    private(set) var lastCapture: CaptureResult?
    /// Transient presentation request for the native share sheet. The clip
    /// value is the exact result returned after the shared-core commit.
    var pendingShareClip: Clip?

    /// Bumped on every successful capture. The banner watches this so an
    /// identical-payload back-to-back snip still re-fires the animation.
    private(set) var captureGeneration: Int = 0

    /// Set to `true` when a snip / quote action ran but no LLM API key was
    /// configured, so we couldn't refine the boundaries. Triggers the
    /// one-time "Add an AI key" hint banner. The banner clears this back
    /// to `false` after showing once (also persists to UserDefaults so the
    /// hint doesn't re-fire across sessions).
    var noLLMKeyHintPending: Bool = false

    static let bannerVisibleSeconds: TimeInterval = 8

    struct CaptureResult: Hashable, Identifiable {
        let id: UUID
        let clip: Clip
        let createdAt: Date
        let summary: String
    }

    // MARK: - Init / wiring

    private var didWireRemote = false

    private init() {}

    func presentShare(for clip: Clip) {
        pendingShareClip = clip
        Self.logger.debug("presenting captured clip \(clip.id, privacy: .public)")
    }

    /// Idempotent. Called from `RootView.onAppear`.
    func attach(playback: PlaybackState, store: AppStateStore) {
        self.playback = playback
        self.store = store
        self.transcriptReader = store.transcriptReader
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

    /// Capture a snip from the live playhead. Returns the proposed clip
    /// immediately so lock-screen commands can acknowledge without blocking;
    /// transcript extraction and the authoritative Rust commit run off-main.
    @discardableResult
    func captureSnip(source: Clip.Source = .touch) -> Clip? {
        guard let playback, let store, let episode = playback.episode else {
            Self.logger.notice("captureSnip: no episode / playback not attached")
            return nil
        }
        let now = playback.currentTime
        let durationCap = max(playback.duration, episode.duration ?? 0)
        let startSeconds = max(0, now - Self.lookbackSeconds)
        let proposedEnd = now + Self.leadSeconds
        let endSeconds = durationCap > 0 ? min(proposedEnd, durationCap) : proposedEnd
        let startMs = Int((startSeconds * 1000).rounded())
        let endMs = Int((endSeconds * 1000).rounded())
        guard endMs > startMs else {
            Self.logger.notice("captureSnip: zero-length window — playhead at start of stream")
            return nil
        }

        let proposedClip = Clip(
            episodeID: episode.id,
            subscriptionID: episode.podcastID,
            startMs: startMs,
            endMs: endMs,
            transcriptText: "",
            source: source
        )
        let modelID = store.state.settings.wikiModel
        let reader = transcriptReader
        Task { @MainActor [weak self] in
            guard let self else { return }
            let (text, speaker) = await Task.detached(priority: .userInitiated) {
                Self.transcriptWindow(
                    reader: reader,
                    episodeID: episode.id,
                    startSeconds: startSeconds,
                    endSeconds: endSeconds,
                    atSeconds: now
                )
            }.value
            var clip = proposedClip
            clip.transcriptText = text ?? ""
            clip.speakerID = speaker?.uuidString
            guard let savedClip = await store.addClip(clip) else {
                Self.logger.error("captureSnip: shared clip commit failed")
                return
            }
            Haptics.success()
            lastCapture = CaptureResult(
                id: UUID(),
                clip: savedClip,
                createdAt: savedClip.createdAt,
                summary: formatSummary(
                    startSeconds: startSeconds,
                    endSeconds: endSeconds
                )
            )
            captureGeneration &+= 1
            Self.logger.info(
                "captured clip \(savedClip.id, privacy: .public) [\(startMs, privacy: .public)..\(endMs, privacy: .public)] source=\(String(describing: source), privacy: .public)"
            )
            await refine(
                clipID: savedClip.id,
                episodeID: episode.id,
                playheadSeconds: now,
                modelID: modelID,
                store: store
            )
        }
        return proposedClip
    }

    // MARK: - Refinement

    /// Ask `ClipBoundaryResolver` for semantic boundaries and apply them in
    /// place. Best-effort — any failure (no transcript yet, no API key,
    /// network blip, malformed response) leaves the mechanical clip intact.
    private func refine(
        clipID: UUID,
        episodeID: UUID,
        playheadSeconds: TimeInterval,
        modelID: String,
        store: AppStateStore
    ) async {
        let reader = transcriptReader
        let transcript = await Task.detached(priority: .utility) {
            reader.load(episodeID: episodeID)
        }.value
        guard let transcript else {
            Self.logger.debug("refine: no transcript yet for \(episodeID, privacy: .public)")
            return
        }
        let modelReference = LLMModelReference(storedID: modelID)
        if !LLMProviderCredentialResolver.hasAPIKey(for: modelReference.provider) {
            noLLMKeyHintPending = true
            return
        }
        let resolved = await ClipBoundaryResolver.shared.resolveBoundaries(
            transcript: transcript,
            playheadSeconds: playheadSeconds,
            intent: .clip,
            modelID: modelID
        )
        guard let resolved else { return }
        let startMs = Int((resolved.startSeconds * 1000).rounded())
        let endMs = Int((resolved.endSeconds * 1000).rounded())
        guard endMs > startMs else { return }
        await store.updateClipBoundaries(
            id: clipID,
            startMs: startMs,
            endMs: endMs,
            transcriptText: resolved.quotedText,
            speakerID: resolved.speakerID
        )
        Self.logger.info("refine: clip \(clipID, privacy: .public) -> [\(startMs, privacy: .public)..\(endMs, privacy: .public)]")
    }

    /// Hand-off the caller can invoke 1.5s after a capture — clears
    /// `lastCapture` so the toast disappears even if no new snip arrives.
    func dismissBanner(for captureID: UUID) {
        if lastCapture?.id == captureID {
            lastCapture = nil
        }
    }

    // MARK: - Transcript helpers

    /// Pull the transcript span [startSeconds, endSeconds] and the speaker
    /// at the trigger moment. Returns `(nil, nil)` when no transcript is
    /// available — the snip is still valid as a span-grounded clip.
    nonisolated private static func transcriptWindow(
        reader: any TranscriptReading,
        episodeID: UUID,
        startSeconds: TimeInterval,
        endSeconds: TimeInterval,
        atSeconds: TimeInterval
    ) -> (String?, UUID?) {
        guard let transcript = reader.load(episodeID: episodeID) else {
            return (nil, nil)
        }
        // Overlapping segments: any segment that intersects the window.
        let overlapping = transcript.segments.filter { seg in
            seg.end >= startSeconds && seg.start <= endSeconds
        }
        let text = overlapping.map(\.text)
            .joined(separator: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let speaker = transcript.segment(at: atSeconds)?.speakerID
        return (text.isEmpty ? nil : text, speaker)
    }

    private func formatSummary(startSeconds: TimeInterval, endSeconds: TimeInterval) -> String {
        let total = Int(round(endSeconds - startSeconds))
        let body: String
        if total <= 60 {
            body = "\(total)s"
        } else if total <= 600 {
            body = "\(total / 60)m \(total % 60)s"
        } else {
            body = "\(total / 60)m"
        }
        return "Snipped · \(body) clipped"
    }
}
