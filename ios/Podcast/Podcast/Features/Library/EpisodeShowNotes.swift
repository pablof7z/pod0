import SwiftUI

// MARK: - EpisodeShowNotes

/// Collapsible show-notes block for `EpisodeDetailView`. Renders nothing when
/// `notes` is `nil` or empty. Content beyond 300 characters gets a
/// "Show more / Show less" toggle so the detail screen stays scannable.
struct EpisodeShowNotes: View {
    let notes: String?

    @State private var expanded: Bool = false

    var body: some View {
        if let notes, !notes.isEmpty {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                Text("Show notes")
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)

                Text(notes)
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)
                    .lineLimit(expanded ? nil : 6)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)

                if notes.count > 300 {
                    Button {
                        withAnimation(.easeInOut(duration: 0.2)) {
                            expanded.toggle()
                        }
                    } label: {
                        Text(expanded ? "Show less" : "Show more")
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(Color.accentColor)
                    }
                    .buttonStyle(.plain)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
