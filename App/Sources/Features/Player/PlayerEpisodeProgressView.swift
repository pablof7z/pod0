import SwiftUI

/// A quiet episode-level progress track beneath the player metadata.
///
/// The visible track stays two points high while the surrounding height is
/// interactive, making precise taps unnecessary without adding more chrome.
struct PlayerEpisodeProgressView: View {
    @Bindable var state: PlaybackState

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.primary.opacity(0.10))
                    .frame(height: 2)

                Capsule()
                    .fill(Color.accentColor.opacity(0.72))
                    .frame(width: proxy.size.width * progress, height: 2)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .local)
                    .onEnded { value in
                        seek(toHorizontalPosition: value.location.x, width: proxy.size.width)
                    }
            )
        }
        .frame(height: 36)
        .accessibilityElement()
        .accessibilityLabel("Episode progress")
        .accessibilityValue(PlayerTimeFormat.progress(state.currentTime, state.duration))
        .accessibilityHint("Tap to seek")
        .accessibilityAdjustableAction { direction in
            guard state.duration.isFinite, state.duration > 0 else { return }
            let delta: TimeInterval = direction == .increment ? 30 : -30
            let destination = min(state.duration, max(0, state.currentTime + delta))
            state.navigationalSeek(to: destination)
        }
    }

    private var progress: CGFloat {
        guard state.duration.isFinite, state.duration > 0 else { return 0 }
        return CGFloat((state.currentTime / state.duration).clamped01)
    }

    private func seek(toHorizontalPosition x: CGFloat, width: CGFloat) {
        guard width > 0, state.duration.isFinite, state.duration > 0 else { return }
        let fraction = (Double(x / width)).clamped01
        state.navigationalSeek(to: fraction * state.duration)
        Haptics.selection()
    }
}
