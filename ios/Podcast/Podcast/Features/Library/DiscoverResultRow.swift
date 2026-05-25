import SwiftUI

// MARK: - DiscoverResultRow

/// Row in the iTunes search results list. Artwork left, title + author right,
/// subscribe button trailing.
///
/// - Tapping anywhere on the row body calls `onSubscribe`.
/// - When already subscribed, a checkmark replaces the ⊕ button.
/// - When a previous subscribe attempt failed, ⚠ replaces ⊕; tapping the
///   ⚠ toggles an inline error caption.
struct DiscoverResultRow: View {

    let result: PodcastSummary
    let isSubscribing: Bool
    let isAlreadySubscribed: Bool
    let rowError: String?
    let isErrorExpanded: Bool
    let onSubscribe: () -> Void
    let onToggleErrorExpansion: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            rowBody
            if rowError != nil, isErrorExpanded {
                errorCaption
                    .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
        .animation(AppTheme.Animation.springFast, value: isErrorExpanded)
    }

    // MARK: - Row body

    private var rowBody: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            artwork
            VStack(alignment: .leading, spacing: 2) {
                Text(result.title)
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                if let author = result.author, !author.isEmpty {
                    Text(author)
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            trailingControl
                .padding(.top, 2)
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .onTapGesture { handleRowTap() }
        .opacity(isRowDisabled ? 0.65 : 1)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(.isButton)
    }

    private var isRowDisabled: Bool { isSubscribing || isAlreadySubscribed }

    private var artwork: some View {
        Group {
            if let urlStr = result.artworkUrl, let url = URL(string: urlStr) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        artworkPlaceholder
                    }
                }
            } else {
                artworkPlaceholder
            }
        }
        .frame(width: 64, height: 64)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous))
    }

    private var artworkPlaceholder: some View {
        ZStack {
            Color(.tertiarySystemFill)
            Image(systemName: "waveform").foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var trailingControl: some View {
        if isSubscribing {
            ProgressView()
                .controlSize(.small)
                .frame(width: 32, height: 32)
        } else if isAlreadySubscribed {
            Image(systemName: "checkmark.circle.fill")
                .font(.title3)
                .foregroundStyle(.secondary)
                .frame(width: 32, height: 32)
        } else if rowError != nil {
            Button(action: onToggleErrorExpansion) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.title3)
                    .foregroundStyle(AppTheme.Tint.error)
                    .frame(width: 32, height: 32)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Show error for \(result.title)")
        } else {
            Image(systemName: "plus.circle.fill")
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 32, height: 32)
        }
    }

    private var errorCaption: some View {
        Label(rowError ?? "", systemImage: "exclamationmark.triangle.fill")
            .font(AppTheme.Typography.caption)
            .foregroundStyle(AppTheme.Tint.error)
            .padding(.leading, 64 + AppTheme.Spacing.md)
            .padding(.bottom, AppTheme.Spacing.sm)
            .accessibilityHint("Tap the warning icon again to dismiss")
    }

    // MARK: - Actions

    private func handleRowTap() {
        guard !isRowDisabled else { return }
        onSubscribe()
    }

    private var accessibilityLabel: String {
        if isAlreadySubscribed { return "Already subscribed to \(result.title)" }
        if isSubscribing { return "Subscribing to \(result.title)" }
        if let rowError { return "Subscribe to \(result.title). Last attempt failed: \(rowError)" }
        return "Subscribe to \(result.title)"
    }
}
