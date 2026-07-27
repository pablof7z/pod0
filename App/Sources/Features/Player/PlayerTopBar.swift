import AVKit
import SwiftUI

// MARK: - PlayerTopBar
//
// Top bar for the full-screen `PlayerView`. Holds the dismiss button on
// the leading edge, the share / AirPlay / more cluster on the trailing
// edge, and a middle slot that crossfades between the show name and a
// compact artwork+title once the hero header has scrolled offscreen.
//
// All state lives in `PlayerView`; this view is a pure layout container
// driven by the bindings/closures the parent passes in.

struct PlayerTopBar: View {
    @Bindable var state: PlaybackState
    let podcast: Podcast?
    let showName: String
    let artworkURL: URL?
    let titleCollapsed: Bool

    let onDismiss: () -> Void
    let onShare: () -> Void
    let onShowSleepTimer: () -> Void
    let onShowSpeed: () -> Void

    @Environment(AppStateStore.self) private var store

    var body: some View {
        HStack(spacing: AppTheme.Spacing.xs) {
            dismissButton
            historyButton
                .animation(AppTheme.Animation.spring, value: historyButtonState)

            Spacer(minLength: 0)

            HStack(spacing: AppTheme.Spacing.xs) {
                if state.episode != nil {
                    Button(action: onShare) {
                        Image(systemName: "square.and.arrow.up")
                            .font(.body.weight(.semibold))
                            .foregroundStyle(.primary)
                            .frame(width: 44, height: 44)
                            .contentShape(Circle())
                    }
                    .buttonStyle(.pressable)
                    .accessibilityLabel("Share episode")

                    routePicker
                }

                if let episode = state.episode {
                    PlayerMoreMenu(
                        episode: episode,
                        podcast: podcast,
                        speedLabel: state.rate.label,
                        onMarkPlayed: { store.markEpisodePlayed(episode.id) },
                        onMarkUnplayed: { store.markEpisodeUnplayed(episode.id) },
                        onShowSleepTimer: onShowSleepTimer,
                        onShowSpeed: onShowSpeed
                    )
                }
            }
        }
        .overlay {
            ZStack {
                if titleCollapsed, let episode = state.episode {
                    PlayerCompactTitleView(
                        artworkURL: artworkURL,
                        episodeTitle: episode.title,
                        showName: showName
                    )
                    .transition(.opacity)
                } else if !showName.isEmpty {
                    Text(showName)
                        .font(AppTheme.Typography.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .transition(.opacity)
                }
            }
            .animation(.easeInOut(duration: 0.2), value: titleCollapsed)
            .padding(.horizontal, 140)
            .frame(maxWidth: .infinity)
            .allowsHitTesting(false)
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.top, AppTheme.Spacing.sm)
        .padding(.bottom, AppTheme.Spacing.xs)
    }

    private var dismissButton: some View {
        Button {
            Haptics.selection()
            onDismiss()
        } label: {
            Image(systemName: "xmark")
                .font(.body.weight(.semibold))
                .foregroundStyle(.primary)
                .frame(width: 44, height: 44)
                .contentShape(Circle())
        }
        .buttonStyle(.pressable)
        .accessibilityLabel("Close player")
    }

    /// Uses one fixed-size slot for back and the temporary forward recovery
    /// action, so neither state shifts the centred title.
    @ViewBuilder
    private var historyButton: some View {
        if state.canJumpForward {
            Button {
                state.jumpForward()
                Haptics.selection()
            } label: {
                Image(systemName: "chevron.right")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.primary)
                    .frame(width: 44, height: 44)
                    .contentShape(Circle())
            }
            .buttonStyle(.pressable)
            .accessibilityLabel("Return to listening position")
            .accessibilityHint("Returns to the position before the last jump back")
            .transition(.opacity.combined(with: .scale(scale: 0.85, anchor: .leading)))
        } else if state.canJumpBack {
            Button {
                state.jumpBack()
                Haptics.selection()
            } label: {
                Image(systemName: "chevron.left")
                    .font(.body.weight(.semibold))
                    .foregroundStyle(.primary)
                    .frame(width: 44, height: 44)
                    .contentShape(Circle())
            }
            .buttonStyle(.pressable)
            .accessibilityLabel("Jump back")
            .accessibilityHint("Returns to the previous playback position")
            .transition(.opacity.combined(with: .scale(scale: 0.85, anchor: .leading)))
        } else {
            Color.clear
                .frame(width: 44, height: 44)
        }
    }

    private var historyButtonState: Int {
        state.canJumpForward ? 2 : (state.canJumpBack ? 1 : 0)
    }

    private var routePicker: some View {
        ZStack {
            Image(systemName: "airplayaudio")
                .font(.body.weight(.semibold))
                .foregroundStyle(.primary)
                .frame(width: 44, height: 44)
                .contentShape(Circle())
                .accessibilityHidden(true)
            RoutePickerView(activeTintColor: .clear, tintColor: .clear)
                .allowsHitTesting(true)
                .accessibilityHidden(true)
        }
        .frame(width: 44, height: 44)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Audio output")
        .accessibilityHint("Opens system output picker")
    }
}
