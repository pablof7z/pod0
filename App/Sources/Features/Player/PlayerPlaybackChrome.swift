import SwiftUI

/// Bottom transport cluster attached through the player's safe-area inset.
///
/// Controls remain visually uncontained while retaining generous invisible
/// tap targets and press feedback.
struct PlayerPlaybackChrome: View {
    @Bindable var state: PlaybackState
    let sourceEpisodeID: String?
    let episode: Episode?
    let chapters: [Episode.Chapter]
    @Binding var showVoiceNoteSheet: Bool

    var body: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            if let sourceEpisodeID {
                PlayerClipSourceChip(sourceEpisodeID: sourceEpisodeID)
                    .animation(.easeInOut(duration: 0.25), value: sourceEpisodeID)
            }
            PlayerPrerollSkipButton(state: state, episode: episode)
                .animation(AppTheme.Animation.spring, value: state.currentTime)
            PlayerControlsView(
                state: state,
                chapters: chapters,
                showVoiceNoteSheet: $showVoiceNoteSheet
            )
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.bottom, AppTheme.Spacing.md)
    }
}
