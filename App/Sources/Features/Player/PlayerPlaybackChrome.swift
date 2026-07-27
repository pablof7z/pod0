import SwiftUI

/// Bottom transport cluster attached through the player's safe-area inset.
///
/// Each interactive control owns its glass body; there is deliberately no
/// outer glass card, which keeps the cluster translucent instead of stacking
/// multiple material layers.
struct PlayerPlaybackChrome: View {
    @Bindable var state: PlaybackState
    let glassNamespace: Namespace.ID
    let sourceEpisodeID: String?
    let episode: Episode?
    let chapters: [Episode.Chapter]
    @Binding var showVoiceNoteSheet: Bool

    var body: some View {
        GlassEffectContainer(spacing: AppTheme.Spacing.md) {
            VStack(spacing: AppTheme.Spacing.md) {
                if let sourceEpisodeID {
                    PlayerClipSourceChip(sourceEpisodeID: sourceEpisodeID)
                        .animation(.easeInOut(duration: 0.25), value: sourceEpisodeID)
                }
                PlayerPrerollSkipButton(state: state, episode: episode)
                    .animation(AppTheme.Animation.spring, value: state.currentTime)
                PlayerControlsView(
                    state: state,
                    glassNamespace: glassNamespace,
                    chapters: chapters,
                    showVoiceNoteSheet: $showVoiceNoteSheet
                )
            }
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.bottom, AppTheme.Spacing.md)
    }
}
