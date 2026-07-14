import SwiftUI

// MARK: - NoLLMKeyHintBanner
//
// One-time hint shown when a snip / Share Quote action ran but no LLM API
// key was configured, so we couldn't refine boundaries. The banner exists
// because the LLM refinement is silent (no spinner on the snip path, since
// the user already got an instant mechanical capture), and a user can't tell
// "the LLM ran and didn't help" from "no LLM key was set." This banner
// disambiguates — once, ever.
//
// Suppression persists in `UserDefaults.standard` so the hint doesn't
// re-fire across launches. Tapping the banner clears it immediately; the
// auto-fade also clears it after a few seconds.

struct NoLLMKeyHintBanner: View {

    @Bindable var controller: AutoSnipController

    /// Suppression marker — once we've shown the hint, we never show it again
    /// on this device unless the user resets storage.
    static let suppressionDefaultsKey = "NoLLMKeyHintBanner.shown.v1"

    @State private var visible: Bool = false
    @State private var dismissTask: Task<Void, Never>?

    var body: some View {
        Group {
            if visible {
                Button(action: dismiss) {
                    HStack(spacing: 10) {
                        Image(systemName: "key.fill")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.primary)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Smarter clips need an AI key")
                                .font(.subheadline.weight(.semibold))
                                .foregroundStyle(.primary)
                            Text("Settings → AI keys")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .glassEffect(.regular.interactive(), in: .capsule)
                }
                .buttonStyle(.plain)
                .transition(
                    .move(edge: .top)
                        .combined(with: .opacity)
                )
                .accessibilityLabel("Smarter clips need an AI key. Open Settings, then AI keys.")
            }
        }
        .animation(.spring(response: 0.42, dampingFraction: 0.82), value: visible)
        .onChange(of: controller.noLLMKeyHintPending) { _, pending in
            guard pending else { return }
            controller.noLLMKeyHintPending = false
            showOnce()
        }
    }

    private func showOnce() {
        let defaults = UserDefaults.standard
        guard !defaults.bool(forKey: Self.suppressionDefaultsKey) else { return }
        defaults.set(true, forKey: Self.suppressionDefaultsKey)
        visible = true
        dismissTask?.cancel()
        dismissTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(4))
            guard !Task.isCancelled else { return }
            visible = false
        }
    }

    private func dismiss() {
        dismissTask?.cancel()
        visible = false
    }
}
