import SwiftUI

// MARK: - AvatarView
//
// Reusable circular avatar for the sidebar and toolbar. T0 paper, 1pt
// hairline ring. Falls back to the display-name initial when the picture
// URL is empty / fails to load.

struct AvatarView: View {

    let url: URL?
    let initial: Character?
    var size: CGFloat = 96
    var ringColor: Color = AppTheme.Tint.hairline

    var body: some View {
        ZStack {
            Circle()
                .fill(AppTheme.Tint.surfaceMuted)
            if let url {
                CachedAsyncImage(url: url, targetSize: CGSize(width: size, height: size)) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        initialView
                    }
                }
                .clipShape(Circle())
            } else {
                initialView
            }
        }
        .frame(width: size, height: size)
        .overlay(
            Circle()
                .strokeBorder(ringColor, lineWidth: 1)
        )
        .accessibilityHidden(true)
    }

    @ViewBuilder
    private var initialView: some View {
        if let initial {
            Text(String(initial).uppercased())
                .font(.system(size: size * 0.42, weight: .semibold, design: .rounded))
                .foregroundStyle(.secondary)
        } else {
            Image(systemName: "person.crop.circle")
                .font(.system(size: size * 0.6))
                .foregroundStyle(.tertiary)
        }
    }
}
