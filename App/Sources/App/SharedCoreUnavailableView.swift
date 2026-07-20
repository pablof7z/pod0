import SwiftUI

/// Fail-closed recovery surface. The app does not render or mutate legacy
/// domain state while the authoritative Rust store is unavailable.
struct SharedCoreUnavailableView: View {
    let reason: String

    var body: some View {
        ContentUnavailableView {
            Label("Pod0 needs to recover its library", systemImage: "externaldrive.badge.exclamationmark")
        } description: {
            Text(
                "Your existing data has been left untouched. Close and reopen Pod0 to retry recovery."
            )
        } actions: {
            Text("Diagnostic: \(reason)")
                .font(.footnote.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemBackground))
        .accessibilityElement(children: .contain)
    }
}
