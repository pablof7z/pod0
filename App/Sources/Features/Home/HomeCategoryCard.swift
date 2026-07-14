import SwiftUI

// MARK: - HomeCategoryCard
//
// Rich card for a single PodcastCategory in the HomeCategoryPickerSheet.
// Artwork strip (up to 3 thumbnails) gives instant visual recognition;
// the LLM description and stats row give semantic + practical context
// so the user can choose a category without having to recall which shows
// belong where.

struct HomeCategoryCard: View {
    let category: PodcastCategory
    let subscriptions: [Podcast]
    let unplayedTotal: Int
    let isSelected: Bool
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                artworkStrip
                infoBlock
                statsRow
            }
            .padding(AppTheme.Spacing.md)
            .background(cardBackground)
        }
        .buttonStyle(.plain)
        .animation(.easeInOut(duration: 0.2), value: isSelected)
    }

    // MARK: - Artwork strip

    private var artworkStrip: some View {
        HStack(spacing: AppTheme.Spacing.xs) {
            ForEach(Array(subscriptions.prefix(3).enumerated()), id: \.offset) { _, sub in
                subscriptionThumbnail(sub)
            }
            if subscriptions.count > 3 {
                overflowBadge
            }
            Spacer(minLength: 0)
        }
    }

    private func subscriptionThumbnail(_ sub: Podcast) -> some View {
        ZStack {
            RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
                .fill(LinearGradient(
                    colors: [sub.accentColor.opacity(0.9), sub.accentColor.opacity(0.5)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                ))
            if let url = sub.imageURL {
                CachedAsyncImage(url: url, targetSize: CGSize(width: 88, height: 88)) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        placeholderIcon(sub)
                    }
                }
            } else {
                placeholderIcon(sub)
            }
        }
        .frame(width: 44, height: 44)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
    }

    private func placeholderIcon(_ sub: Podcast) -> some View {
        Image(systemName: sub.artworkSymbol)
            .font(.system(size: 14, weight: .light))
            .foregroundStyle(.white.opacity(0.9))
    }

    private var overflowBadge: some View {
        Text("+\(subscriptions.count - 3)")
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.secondary)
            .frame(width: 36, height: 44)
            .background(AppTheme.Tint.surfaceMuted,
                        in: RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
    }

    // MARK: - Info block

    private var infoBlock: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(category.name.isEmpty ? category.slug : category.name)
                .font(AppTheme.Typography.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
                .lineLimit(1)
            if !category.description.isEmpty {
                Text(category.description)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
        }
    }

    // MARK: - Stats row

    private var statsRow: some View {
        HStack {
            Text(statsLabel)
                .font(AppTheme.Typography.caption2)
                .foregroundStyle(.tertiary)
            Spacer(minLength: 0)
            if isSelected {
                Image(systemName: "checkmark.circle.fill")
                    .font(.system(size: 18, weight: .medium))
                    .foregroundStyle(Color.accentColor)
                    .transition(.scale.combined(with: .opacity))
            }
        }
    }

    // MARK: - Card background

    private var cardBackground: some View {
        RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
            .fill(Color(.secondarySystemGroupedBackground))
            .overlay(
                RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                    .strokeBorder(
                        isSelected ? Color.accentColor.opacity(0.45) : Color.clear,
                        lineWidth: 1.5
                    )
            )
    }

    // MARK: - Derived

    private var statsLabel: String {
        let n = subscriptions.count
        let showPart = n == 1 ? "1 show" : "\(n) shows"
        guard unplayedTotal > 0 else { return showPart }
        return "\(showPart) · \(unplayedTotal) unplayed"
    }
}
