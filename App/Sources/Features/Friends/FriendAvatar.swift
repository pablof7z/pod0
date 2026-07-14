import SwiftUI

// MARK: - FriendAvatar

/// Circular avatar for a Friend. Shows a URL-based photo when available,
/// falling through to a gradient-initial monogram derived from the identifier.
///
/// The color is computed continuously from the identifier hash (HSB space)
/// so every friend gets a unique shade — not a slot from a fixed palette.
///
/// Pass a non-zero `pendingCount` to display a red badge in the top-trailing
/// corner of the avatar. The badge is rendered outside the clipped circle so
/// it visually "lifts" off the avatar edge.
struct FriendAvatar: View {
    let friend: Friend
    var size: CGFloat = Layout.defaultSize
    /// Number of pending items from this friend. When > 0, a count badge is
    /// shown in the top-trailing corner. Defaults to 0 (no badge).
    var pendingCount: Int = 0

    // MARK: - Layout constants

    private enum Layout {
        /// Default avatar diameter when no explicit size is provided.
        static let defaultSize: CGFloat = 40
        /// Ratio of font size to avatar diameter — keeps the initial visually centered.
        static let fontSizeRatio: CGFloat = 0.38
        /// Diameter of the pending-count badge circle.
        static let badgeSize: CGFloat = 18
        /// Point size of the count label inside the pending badge.
        static let badgeFontSize: CGFloat = 10
        /// Stroke width of the white ring that separates the badge from the avatar.
        static let badgeStrokeWidth: CGFloat = 2
        /// Maximum count displayed literally; anything higher shows "99+".
        static let badgeMaxCount: Int = 99
    }

    var body: some View {
        ZStack {
            gradientBackground
            Text(String(friend.displayName.prefix(1)).uppercased())
                .font(.system(size: size * Layout.fontSizeRatio, weight: .semibold, design: .rounded))
                .foregroundStyle(.white)
            if let url = validAvatarURL {
                CachedAsyncImage(url: url, targetSize: CGSize(width: 64, height: 64)) { phase in
                    if case .success(let image) = phase {
                        image.resizable().scaledToFill()
                    }
                }
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        // Badge is overlaid on the clipped view so it renders outside the circle.
        .overlay(alignment: .topTrailing) {
            if pendingCount > 0 {
                pendingBadge
                    // Shift the badge so it straddles the avatar edge.
                    .offset(x: Layout.badgeSize * 0.3, y: -Layout.badgeSize * 0.3)
            }
        }
        .accessibilityLabel(
            pendingCount > 0
                ? "\(friend.displayName), \(pendingCount) pending item\(pendingCount == 1 ? "" : "s")"
                : friend.displayName
        )
        .accessibilityAddTraits(.isImage)
    }

    // MARK: - Badge

    private var badgeLabel: String {
        pendingCount > Layout.badgeMaxCount ? "99+" : "\(pendingCount)"
    }

    private var pendingBadge: some View {
        Text(badgeLabel)
            .font(.system(size: Layout.badgeFontSize, weight: .bold, design: .rounded))
            .foregroundStyle(.white)
            .padding(.horizontal, 4)
            .frame(minWidth: Layout.badgeSize, minHeight: Layout.badgeSize)
            .background(.red, in: Capsule())
            .overlay(Capsule().strokeBorder(.background, lineWidth: Layout.badgeStrokeWidth))
    }

    // MARK: - Helpers

    private var validAvatarURL: URL? {
        guard let urlString = friend.avatarURL,
              let url = URL(string: urlString),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else { return nil }
        return url
    }

    private var gradientBackground: some View {
        let hue = Double(abs(friend.identifier.hashValue) % 360) / 360.0
        let base = Color(hue: hue, saturation: 0.65, brightness: 0.82)
        let dim  = Color(hue: hue, saturation: 0.65, brightness: 0.65)
        return Circle().fill(
            LinearGradient(colors: [base, dim], startPoint: .topLeading, endPoint: .bottomTrailing)
        )
    }
}
