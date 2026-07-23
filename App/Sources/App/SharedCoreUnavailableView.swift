import SwiftUI

/// Fail-closed recovery surface. The app does not render or mutate legacy
/// domain state while the authoritative Rust store is unavailable.
struct SharedCoreUnavailableView: View {
    var body: some View {
        ContentUnavailableView {
            Label(
                "Pod0 couldn’t finish updating",
                systemImage: "arrow.clockwise.circle"
            )
        } description: {
            Text("Your library is safe. Close and reopen Pod0 to try again.")
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemBackground))
        .accessibilityElement(children: .contain)
    }
}
