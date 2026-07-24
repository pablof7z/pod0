import SwiftUI

/// Twitter-style slide-in sidebar. Triggered by tapping the user avatar in the
/// navigation bar. Shows a left-anchored panel with the user's identity and
/// navigation shortcuts. Tap the darkened overlay to dismiss.
struct AppSidebarView: View {
    @Binding var selectedTab: RootTab
    @Binding var isPresented: Bool

    @Environment(AppStateStore.self) private var store

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            navSection
                .padding(.top, AppTheme.Spacing.sm)
            Spacer()
        }
        .safeAreaPadding(.top)
        .safeAreaPadding(.bottom)
        .background(Color(.systemBackground).ignoresSafeArea())
    }

    // MARK: - Header

    private var header: some View {
        let settings = store.state.settings
        let name = settings.agentDisplayName.trimmed
        return VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            AvatarView(
                url: URL(string: settings.agentAvatarURLString.trimmed),
                initial: name.first,
                size: 72
            )
            Text(name.isEmpty ? "Welcome" : name)
                .font(AppTheme.Typography.title3)
                .foregroundStyle(.primary)
                .lineLimit(1)
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.top, AppTheme.Spacing.lg)
        .padding(.bottom, AppTheme.Spacing.lg)
    }

    // MARK: - Main navigation

    private var navSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            Divider()
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.bottom, AppTheme.Spacing.xs)

            navRow("Home", icon: "house.fill", isActive: selectedTab == .home) {
                selectedTab = .home
                dismiss()
            }
            navRow("Library", icon: "tray.fill", isActive: selectedTab == .library) {
                selectedTab = .library
                dismiss()
            }
            navRow("Clips", icon: "scissors", isActive: selectedTab == .clips) {
                selectedTab = .clips
                dismiss()
            }
            navRow("Settings", icon: "gearshape", isActive: selectedTab == .settings) {
                selectedTab = .settings
                dismiss()
            }
        }
    }

    // MARK: - Row

    private func navRow(
        _ title: String,
        icon: String,
        isActive: Bool,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: AppTheme.Spacing.md) {
                Image(systemName: icon)
                    .font(.system(size: 19, weight: .medium))
                    .foregroundStyle(isActive ? Color.accentColor : .primary)
                    .frame(width: 26, alignment: .center)
                Text(title)
                    .font(AppTheme.Typography.title3)
                    .fontWeight(isActive ? .bold : .semibold)
                    .foregroundStyle(isActive ? Color.accentColor : .primary)
                Spacer()
            }
            .padding(.horizontal, AppTheme.Spacing.lg)
            .frame(height: 52)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background {
            if isActive {
                RoundedRectangle(cornerRadius: AppTheme.Corner.sm)
                    .fill(Color.accentColor.opacity(0.08))
                    .padding(.horizontal, AppTheme.Spacing.sm)
            }
        }
    }

    // MARK: - Dismiss

    private func dismiss() {
        Haptics.selection()
        withAnimation(AppTheme.Animation.spring) {
            isPresented = false
        }
    }
}
