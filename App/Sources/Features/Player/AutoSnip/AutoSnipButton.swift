import SwiftUI

// MARK: - AutoSnipButton
//
// Compact glass button that captures a snip via `AutoSnipController.shared`.
// Designed to drop into the player controls row as a single-line modifier
// — see the call site in `PlayerControlsView`.
//
// The button is the universal trigger: lock-screen / Control Center route
// through `MPRemoteCommandCenter.bookmarkCommand` (wired by the controller),
// but iOS does not surface AirPods double-tap or wired headphone middle-press
// as a discrete remote command, so this button is the only path on those
// hardware setups.

struct AutoSnipButton: View {

    var body: some View {
        Button {
            AutoSnipController.shared.captureSnip(source: .touch)
        } label: {
            Image(systemName: "bookmark.fill")
                .font(.title3.weight(.semibold))
                .foregroundStyle(.primary)
                .frame(width: 44, height: 44)
                .glassEffect(.regular.interactive(), in: .circle)
        }
        .buttonStyle(.pressable)
        .accessibilityLabel("Snip last 30 seconds")
        .accessibilityHint("Saves a 30-second clip ending at the current moment")
    }
}
