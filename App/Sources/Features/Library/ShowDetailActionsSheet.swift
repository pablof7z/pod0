import SwiftUI

// MARK: - ShowDetailActionsSheet

/// App-owned replacement for the Show detail toolbar overflow actions.
///
/// The player already uses this pattern because simulator UI automation can
/// stall waiting for system pull-down menus to become idle. Keeping show-level
/// actions in a sheet gives users the same options with clearer tap targets
/// and gives tests a stable, visible accessibility tree.
struct ShowDetailActionsSheet: View {
    let podcast: Podcast
    let hasEpisodes: Bool
    let isFollowed: Bool
    let isApplyingFollowChange: Bool
    let sharePreviewTitle: String
    let onSettings: () -> Void
    let onDownloadAll: () -> Void
    let onFollow: () -> Void
    let onUnsubscribe: () -> Void
    let onDelete: () -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if isFollowed {
                        optionButton("Settings for this show", systemImage: "slider.horizontal.3") {
                            onSettings()
                        }
                    }

                    if hasEpisodes {
                        optionButton("Download all episodes", systemImage: "arrow.down.circle") {
                            onDownloadAll()
                        }
                    }

                    if !isFollowed, podcast.feedURL != nil {
                        optionButton(
                            "Follow",
                            systemImage: "plus.circle",
                            disabled: isApplyingFollowChange
                        ) {
                            onFollow()
                        }
                    }

                    if podcast.feedURL != nil {
                        shareButton
                    }

                    Divider()
                        .padding(.vertical, AppTheme.Spacing.xs)

                    if isFollowed {
                        optionButton(
                            "Unsubscribe",
                            systemImage: "minus.circle",
                            role: .destructive,
                            isDestructive: true,
                            disabled: isApplyingFollowChange
                        ) {
                            onUnsubscribe()
                        }
                    } else {
                        optionButton(
                            "Delete podcast",
                            systemImage: "trash",
                            role: .destructive,
                            isDestructive: true
                        ) {
                            onDelete()
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.bottom, AppTheme.Spacing.lg)
            }
            .navigationTitle("Show Options")
            .toolbarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }

    @ViewBuilder
    private var shareButton: some View {
        if let feedURL = podcast.feedURL {
            ShareLink(
                item: feedURL,
                preview: SharePreview(
                    sharePreviewTitle,
                    image: Image(systemName: "antenna.radiowaves.left.and.right")
                )
            ) {
                Label("Share show", systemImage: "square.and.arrow.up")
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(.primary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    .padding(.vertical, AppTheme.Spacing.md)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.pressable(scale: 0.98, opacity: 0.85))
        }
    }

    private func optionButton(
        _ title: String,
        systemImage: String,
        role: ButtonRole? = nil,
        isDestructive: Bool = false,
        disabled: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        Button(role: role, action: action) {
            Label(title, systemImage: systemImage)
                .font(AppTheme.Typography.body)
                .foregroundStyle(isDestructive ? Color.red : Color.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.vertical, AppTheme.Spacing.md)
                .contentShape(Rectangle())
        }
        .buttonStyle(.pressable(scale: 0.98, opacity: 0.85))
        .disabled(disabled)
    }
}
