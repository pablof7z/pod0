import SwiftUI

// MARK: - AutoSnipBanner
//
// Small glass toast that remains visible for eight seconds after
// `AutoSnipController` captures a snip. Tapping the banner opens the existing
// clip share sheet.
//
// Designed to be installed once at the player root via `.overlay(...)`. The
// banner observes the controller's `captureGeneration` so back-to-back snips
// — even with identical payloads — re-trigger the animation cleanly.

struct AutoSnipBanner: View {

    @Bindable var controller: AutoSnipController

    @State private var visible: Bool = false
    @State private var dismissTask: Task<Void, Never>?
    @State private var lastSeenGeneration: Int = 0

    var body: some View {
        Group {
            if visible, let capture = controller.lastCapture {
                Button {
                    Haptics.selection()
                    controller.presentShare(for: capture.clip)
                } label: {
                    HStack(spacing: 10) {
                        Image(systemName: "bookmark.fill")
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.primary)
                        Text(capture.summary)
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.primary)
                            .lineLimit(1)
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
                .accessibilityLabel(capture.summary)
                .accessibilityHint("Open clip")
            }
        }
        .animation(.spring(response: 0.42, dampingFraction: 0.82), value: visible)
        .onChange(of: controller.captureGeneration) { _, newValue in
            guard newValue != lastSeenGeneration else { return }
            lastSeenGeneration = newValue
            showBanner()
        }
    }

    private func showBanner() {
        guard let capture = controller.lastCapture else { return }
        visible = true
        dismissTask?.cancel()
        let captureID = capture.id
        dismissTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(AutoSnipController.bannerVisibleSeconds))
            guard !Task.isCancelled else { return }
            visible = false
            // Give the fade-out time before clearing the source-of-truth so
            // the View's `if let` branch holds the content during transition.
            try? await Task.sleep(for: .milliseconds(360))
            guard !Task.isCancelled else { return }
            controller.dismissBanner(for: captureID)
        }
    }
}

private struct AutoSnipPresentationModifier: ViewModifier {
    @Environment(AppStateStore.self) private var store
    @Bindable var controller: AutoSnipController

    func body(content: Content) -> some View {
        content
            .sheet(item: $controller.pendingShareClip) { clip in
                if let episode = store.episode(id: clip.episodeID),
                   let podcast = store.podcast(id: clip.subscriptionID) {
                    ClipShareSheet(clip: clip, episode: episode, podcast: podcast)
                        .presentationDetents([.large])
                        .presentationDragIndicator(.visible)
                }
            }
            .overlay(alignment: .top) {
                VStack(spacing: AppTheme.Spacing.sm) {
                    AutoSnipBanner(controller: controller)
                    NoLLMKeyHintBanner(controller: controller)
                }
                .padding(.top, AppTheme.Spacing.lg)
            }
    }
}

extension View {
    func autoSnipPresentation(controller: AutoSnipController) -> some View {
        modifier(AutoSnipPresentationModifier(controller: controller))
    }
}
