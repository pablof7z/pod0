import SwiftUI

/// Fail-closed recovery surface. The app does not render or mutate legacy
/// domain state while the authoritative Rust store is unavailable.
///
/// The copy is per-reason because the remedies differ. A single message meant
/// telling everyone to close and reopen, which is false for every *blocked*
/// store: a downgrade is refused, a failed migration does not retry itself,
/// and a corrupt store does not repair on launch. An error surface with no
/// honest next step still beats one with a fake one.
struct SharedCoreUnavailableView: View {
    /// Raw value of `SharedLibraryBootstrapFailureCode`, or any other reason
    /// string recorded at startup. Unrecognised values fall back to the
    /// retry-safe wording, which is correct for the transient cases.
    let reason: String?

    var body: some View {
        ContentUnavailableView {
            Label(copy.title, systemImage: copy.symbol)
        } description: {
            Text(copy.message)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemBackground))
        .accessibilityElement(children: .contain)
    }

    private struct Copy {
        let title: String
        let message: String
        let symbol: String
    }

    private var copy: Copy {
        switch reason.flatMap(SharedLibraryBootstrapFailureCode.init(rawValue:)) {
        case .storeNewerThanApp:
            // Relaunching this binary can never succeed, so do not suggest it.
            // Updating is the whole remedy and it is worth saying plainly.
            Copy(
                title: "Pod0 needs updating",
                message: "This version of Pod0 is older than your library. "
                    + "Your library is safe — update Pod0 to open it.",
                symbol: "arrow.up.circle"
            )
        case .migrationFailed:
            // No user gesture completes a failed migration. Say what is true —
            // the data and its backup are intact — rather than inventing one.
            Copy(
                title: "An update didn’t finish",
                message: "Your library is safe, and Pod0 backed it up before "
                    + "starting. Reopening won’t finish the update on its own.",
                symbol: "exclamationmark.triangle"
            )
        case .storeUnreadable:
            Copy(
                title: "Pod0 can’t read your library",
                message: "The library file is unreadable or belongs to another "
                    + "app. Your original data hasn’t been changed.",
                symbol: "questionmark.folder"
            )
        default:
            // Transient and recovery states — retrying is genuinely meaningful.
            Copy(
                title: "Pod0 couldn’t finish updating",
                message: "Your library is safe. Close and reopen Pod0 to try again.",
                symbol: "arrow.clockwise.circle"
            )
        }
    }

    /// Test seam: the rendered description, without reaching into SwiftUI.
    var messageForTesting: String { copy.message }
}
