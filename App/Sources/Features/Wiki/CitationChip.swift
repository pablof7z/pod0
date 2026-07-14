import SwiftUI

// MARK: - Citation chip

/// Small glass capsule rendering a single `WikiCitation`'s timestamp.
///
/// Two gestures (peek-first, per the llm-wiki ethos — provenance comes
/// before commitment):
///   - **Tap** presents a `CitationPeekSheet` so the user can audition the
///     cited ~12 seconds without losing their place.
///   - **Long-press** dispatches `play_episode` via `PlaybackState` (set
///     the episode, seek to `startMS`, play) for the user who already
///     knows they want to commit to the full clip.
///
/// Visually mirrors UX-04's amber chip palette but renders with the Liquid
/// Glass material so it morphs alongside the rest of the floating wiki
/// chrome.
struct CitationChip: View {

    let citation: WikiCitation
    /// Episode resolver — the chip needs an `Episode` instance to hand off
    /// to the player, but doesn't want to depend on the store's concrete
    /// type. The host view injects `store.episode(id:)`.
    let resolveEpisode: (UUID) -> Episode?

    @Environment(PlaybackState.self) private var playback
    @State private var peeking = false

    var body: some View {
        Button {
            Haptics.selection()
            peeking = true
        } label: {
            HStack(spacing: 4) {
                Image(systemName: "quote.bubble")
                    .font(.caption2)
                Text(citation.formattedTimestamp)
                    .font(.system(.caption, design: .monospaced))
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(
                Capsule()
                    .fill(Color.clear)
                    .glassEffect(.regular.interactive(), in: .capsule)
            )
            .overlay(
                Capsule()
                    .strokeBorder(CitationChip.amber.opacity(0.35), lineWidth: 0.5)
            )
            .foregroundStyle(CitationChip.amber)
        }
        .buttonStyle(.plain)
        .simultaneousGesture(
            LongPressGesture(minimumDuration: 0.4)
                .onEnded { _ in
                    playClipImmediate()
                }
        )
        .sheet(isPresented: $peeking) {
            CitationPeekSheet(
                citation: citation,
                resolveEpisode: resolveEpisode
            )
            .presentationDetents([.fraction(0.42), .medium])
            .presentationDragIndicator(.visible)
            .presentationBackground(.regularMaterial)
        }
        .accessibilityLabel("Citation at \(citation.formattedTimestamp)")
        // Hints describe gesture effects; VoiceOver already announces
        // "double-tap to activate" via the button trait, so the label
        // shouldn't repeat that. The non-default long-press jump-to-play
        // gesture lives in the hint accordingly.
        .accessibilityHint("Peeks the cited moment. Long-press to play the full clip.")
    }

    // MARK: - Actions

    /// Fires the wiki-citation deep-link contract: jump the player straight
    /// to the cited timestamp and start playback. Falls back to a
    /// `NotificationCenter` post when the episode can't be resolved locally
    /// — preserves backward compatibility with the original peek view's
    /// stub contract.
    private func playClipImmediate() {
        Haptics.selection()
        if let episode = resolveEpisode(citation.episodeID) {
            playback.setEpisode(episode)
            playback.seek(to: TimeInterval(citation.startMS) / 1_000)
            playback.play()
            return
        }
        NotificationCenter.default.post(
            name: CitationPeekView.playClipNotification,
            object: nil,
            userInfo: [
                "episodeID": citation.episodeID.uuidString,
                "startMS": citation.startMS,
                "endMS": citation.endMS,
            ]
        )
    }

    // MARK: - Palette

    /// Editorial amber, shared with `ThreadingMentionRow` and wiki
    /// contradiction surfaces. Lives once in `AppTheme.Tint.editorialAmber`.
    static let amber = AppTheme.Tint.editorialAmber
}
