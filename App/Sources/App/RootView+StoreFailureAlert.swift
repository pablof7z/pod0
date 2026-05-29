import SwiftUI

// MARK: - Store-open-failure alert

/// Presents the mandatory NMP v0.1.0 `store_open_failure` diagnostic (V-67) as a
/// user-facing alert. The kernel sets the diagnostic when it could not open its
/// on-disk LMDB store and fell back to in-memory — meaning the current session's
/// data will not persist — and the host is required to surface it.
///
/// Extracted into its own modifier (rather than inlined on `RootView`) to keep
/// `RootView.swift` under the 500-line hard limit and to isolate the alert's
/// dismiss/re-present gating.
struct StoreFailureAlert: ViewModifier {
    @Environment(KernelModel.self) private var kernelModel

    /// Suppresses re-presentation after the user taps OK. `store_open_failure`
    /// is emitted on every tick while the store is down, so without this the
    /// alert would re-present on each frame.
    @State private var dismissed = false

    func body(content: Content) -> some View {
        // Read the observable property in `body` (not only inside the Binding
        // closure) so SwiftUI tracks it and re-evaluates the alert when the
        // kernel raises or clears the failure.
        let failure = kernelModel.storeOpenFailure
        content
            .alert(
                "Storage Unavailable",
                isPresented: Binding(
                    get: { failure != nil && !dismissed },
                    set: { if !$0 { dismissed = true } }
                ),
                presenting: failure
            ) { _ in
                Button("OK", role: .cancel) { dismissed = true }
            } message: { reason in
                Text(
                    """
                    On-device storage couldn't be opened, so anything you do this \
                    session won't be saved. Restarting the app usually fixes it.

                    \(reason)
                    """
                )
            }
            // A new failure (or recovery → re-failure) re-arms the alert; repeated
            // identical ticks while down do not, because `dismissed` stays true
            // until the condition actually clears.
            .onChange(of: kernelModel.storeOpenFailure) { _, newValue in
                if newValue == nil { dismissed = false }
            }
    }
}

extension View {
    /// Attach the mandatory store-open-failure alert, driven by `KernelModel`.
    func storeFailureAlert() -> some View {
        modifier(StoreFailureAlert())
    }
}
