import SwiftUI

@MainActor
private struct SharedEpisodeImportPresenter: ViewModifier {
    @Environment(AppStateStore.self) private var store

    let coordinator: SharedEpisodeImportCoordinator
    let onImported: @MainActor (UUID) -> Void

    func body(content: Content) -> some View {
        content
            .task { await consumePending() }
            .onReceive(
                NotificationCenter.default.publisher(
                    for: UIApplication.willEnterForegroundNotification
                )
            ) { _ in
                Task { await consumePending() }
            }
            .overlay(alignment: .top) {
                if let label = bannerLabel {
                    Label(label.text, systemImage: label.systemImage)
                        .font(.system(.subheadline, weight: .semibold))
                        .foregroundStyle(.primary)
                        .padding(.horizontal, 16)
                        .padding(.vertical, 10)
                        .background(.thinMaterial, in: Capsule())
                        .padding(.top, 8)
                        .transition(.move(edge: .top).combined(with: .opacity))
                        .accessibilityIdentifier("shared-episode-import-status")
                }
            }
            .animation(.snappy, value: coordinator.phase)
            .alert(
                "Couldn’t add episode",
                isPresented: Binding(
                    get: {
                        if case .failed = coordinator.phase { return true }
                        return false
                    },
                    set: { presented in
                        if !presented { coordinator.dismissFailure() }
                    }
                )
            ) {
                Button("OK") { coordinator.dismissFailure() }
            } message: {
                if case .failed(let message) = coordinator.phase {
                    Text(message)
                }
            }
    }

    private var bannerLabel: (text: String, systemImage: String)? {
        switch coordinator.phase {
        case .importing:
            ("Adding episode…", "arrow.down.circle")
        case .downloadStarted(let title):
            ("Downloading \(title)", "checkmark.circle.fill")
        case .failed, nil:
            nil
        }
    }

    private func consumePending() async {
        await coordinator.consumePending(
            store: store,
            onImported: onImported
        )
    }
}

extension View {
    @MainActor
    func sharedEpisodeImportPresenter(
        coordinator: SharedEpisodeImportCoordinator,
        onImported: @escaping @MainActor (UUID) -> Void
    ) -> some View {
        modifier(SharedEpisodeImportPresenter(
            coordinator: coordinator,
            onImported: onImported
        ))
    }
}
