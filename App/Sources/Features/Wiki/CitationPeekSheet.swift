import SwiftUI

// MARK: - Citation peek sheet

/// Wrapper that adds the UX-04 §5 "citation peek" lifecycle around the
/// existing `CitationPeekView` content:
///
/// - Rises ~1/3 from the bottom (caller picks the detents).
/// - **Autoplays** the cited 12 seconds the moment the sheet appears.
/// - On dismiss, **restores** the prior playback context — episode,
///   playhead, and play/pause state — so a peek never hijacks what the user
///   was already listening to.
///
/// The 12-second window matches the brief; if the citation's own span is
/// longer, the sheet still autoplays from `startMS` and lets natural
/// dismissal stop the audio. If the citation is shorter, we play out the
/// citation and idle until the user dismisses.
struct CitationPeekSheet: View {

    let citation: WikiCitation
    /// Episode resolver injected by the host (typically
    /// `store.episode(id:)`). The sheet only takes over playback if the
    /// lookup succeeds — otherwise it falls back to the inner view's
    /// metadata-only treatment.
    let resolveEpisode: (UUID) -> Episode?

    @Environment(PlaybackState.self) private var playback
    @Environment(\.dismiss) private var dismiss

    /// Snapshot of the playback context captured the moment the sheet
    /// appears. Restored on dismiss so the user is returned to exactly where
    /// they were.
    @State private var prior: PriorPlayback?
    /// `true` once the autoplay path has run for this presentation. Guards
    /// against duplicate `onAppear` calls inside the same lifecycle.
    @State private var hasAutoplayed = false
    /// Set when the user taps "Play clip" inside the peek view to commit
    /// to the full clip. Suppresses the auto-restore that normally fires
    /// on dismiss, so audio keeps rolling at the cited timestamp.
    @State private var didCommit = false

    /// 12-second peek window. UX-04 §5: "autoplays the cited 12s".
    static let peekWindowSeconds: Double = 12

    var body: some View {
        CitationPeekView(citation: citation, onCommitFullClip: commitAndDismiss)
            .onAppear { startPeek() }
            .onDisappear { endPeek() }
    }

    /// User wants to keep playing past the 12s peek window. Mark the
    /// commit flag so `endPeek` skips the restore, then dismiss so the
    /// sheet gets out of the way of full-screen playback context.
    private func commitAndDismiss() {
        didCommit = true
        Haptics.success()
        dismiss()
    }

    // MARK: - Lifecycle

    private func startPeek() {
        guard !hasAutoplayed else { return }
        hasAutoplayed = true
        // Snapshot the prior context before we mutate playback so the
        // restore on dismiss is exact. Captured even when there is no
        // current episode — `nil` is a valid prior state.
        prior = PriorPlayback(
            episode: playback.episode,
            currentTime: playback.currentTime,
            wasPlaying: playback.isPlaying
        )
        guard let target = resolveEpisode(citation.episodeID) else {
            // No local episode — leave audio alone. Caller can still see
            // metadata and quote text in the inner view.
            return
        }
        let startSeconds = TimeInterval(citation.startMS) / 1_000
        if playback.episode?.id == target.id {
            playback.seek(to: startSeconds)
        } else {
            playback.setEpisode(target)
            playback.seek(to: startSeconds)
        }
        playback.play()
    }

    private func endPeek() {
        // Commit path: user tapped "Play clip" — leave audio alone so the
        // cited timestamp keeps playing past the 12s peek.
        guard !didCommit else { return }
        guard let prior else { return }
        // Restore: switch back to the prior episode (if any), seek to the
        // prior playhead, and replay only if the user was actively playing
        // when the peek started.
        if let priorEpisode = prior.episode {
            if playback.episode?.id != priorEpisode.id {
                playback.setEpisode(priorEpisode)
            }
            playback.seek(to: prior.currentTime)
            if prior.wasPlaying {
                playback.play()
            } else {
                playback.pause()
            }
        } else {
            // No prior episode — best we can do is pause whatever the peek
            // started so it doesn't keep playing after dismissal.
            playback.pause()
        }
    }

    // MARK: - Snapshot

    /// Compact value capturing the playback context the peek hijacked so
    /// dismissal can restore it exactly.
    private struct PriorPlayback: Equatable {
        let episode: Episode?
        let currentTime: TimeInterval
        let wasPlaying: Bool
    }
}
