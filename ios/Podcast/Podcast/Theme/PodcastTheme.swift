import SwiftUI

enum PodcastColor {
    static let accent = Color.accentColor
    static let accentSoft = accent.opacity(0.12)
    static let bg = Color(.systemBackground)
    static let surface = Color(.secondarySystemBackground)
    static let surfaceElevated = Color(.tertiarySystemBackground)
    static let hairline = Color(.separator)
    static let hairlineSoft = hairline.opacity(0.35)
    static let transparent = Color.clear
    static let textPrimary = Color.primary
    static let textSecondary = Color.secondary
    static let textTertiary = Color(.tertiaryLabel)
    static let link = Color(.link)
    static let success = Color(.systemGreen)
    static let warning = Color(.systemOrange)
    static let danger = Color(.systemRed)
    static let network = Color(.systemCyan)
    static let positive = success
    static let systemFill = Color(.systemFill)
    static let secondaryFill = Color(.secondarySystemFill)
    static let emphasisForeground = Color(.white)
    static let errorBannerBackground = danger.opacity(0.9)

    /// Deterministic avatar gradient from a hex color string the kernel
    /// supplies (`avatarColor`). Falls back to the accent.
    static func avatar(from hex: String) -> LinearGradient {
        avatarGradient(base: avatarBase(from: hex))
    }

    static func avatarBase(from hex: String?) -> Color {
        guard let hex, let color = Color(hex: hex) else { return accent }
        return color
    }

    private static func avatarGradient(base: Color) -> LinearGradient {
        LinearGradient(
            colors: [base, base.opacity(0.65)],
            startPoint: .topLeading, endPoint: .bottomTrailing)
    }
}

enum PodcastFont {
    static let largeTitle = Font.largeTitle.weight(.bold)
    static let title = Font.title2.weight(.semibold)
    static let headline = Font.headline
    static let body = Font.body
    static let callout = Font.callout
    static let caption = Font.caption
    static let mono = Font.footnote.monospaced()
}

enum PodcastSpace {
    static let xs: CGFloat = 4
    static let s: CGFloat = 8
    static let m: CGFloat = 12
    static let l: CGFloat = 16
    static let xl: CGFloat = 24
    static let xxl: CGFloat = 36
    static let radius: CGFloat = 20
    static let radiusSmall: CGFloat = 12
}

struct PodcastBackdrop: View {
    var body: some View {
        Rectangle().fill(.background)
            .ignoresSafeArea()
    }
}

extension View {
    func podcastScreenBackground() -> some View {
        background(PodcastBackdrop())
    }
}

/// Standard empty / loading placeholder.
struct PodcastPlaceholder: View {
    let systemImage: String
    let title: String
    var subtitle: String? = nil
    var body: some View {
        VStack(spacing: PodcastSpace.m) {
            Image(systemName: systemImage)
                .font(.system(size: 44, weight: .light))
                .symbolRenderingMode(.hierarchical)
            Text(title)
                .font(.headline)
            if let subtitle {
                Text(subtitle)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
        }
        .padding(PodcastSpace.xl)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

extension Color {
    /// Parse "#RRGGBB" / "RRGGBB" — used for kernel-supplied avatar colors.
    init?(hex: String) {
        var s = hex.trimmingCharacters(in: .whitespaces)
        if s.hasPrefix("#") { s.removeFirst() }
        guard s.count == 6, let v = UInt64(s, radix: 16) else { return nil }
        self = Color(
            red: Double((v >> 16) & 0xFF) / 255,
            green: Double((v >> 8) & 0xFF) / 255,
            blue: Double(v & 0xFF) / 255)
    }
}
