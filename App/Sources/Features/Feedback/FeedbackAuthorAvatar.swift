import SwiftUI

/// Small circular avatar for a feedback message bubble. Renders the
/// author's kind:0 `picture` URL via AsyncImage and falls back to a
/// tinted circle with the first letter of the display name when the
/// picture is missing or fails to load.
///
/// Used by `FeedbackBubble` when `showHeader == true`. Continuation
/// messages (same author within the burst window) reserve the same
/// width with a clear spacer so columns stay aligned.
struct FeedbackAuthorAvatar: View {
    let pictureURL: URL?
    let initial: String
    var size: CGFloat = Layout.defaultSize

    enum Layout {
        static let defaultSize: CGFloat = 28
        static let borderOpacity: Double = 0.08
        static let borderWidth: CGFloat = 1
        static let fontScale: CGFloat = 0.42
        static let backgroundOpacity: Double = 0.15
    }

    var body: some View {
        Group {
            if let pictureURL {
                AsyncImage(url: pictureURL) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        fallback
                    }
                }
            } else {
                fallback
            }
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .overlay(
            Circle().strokeBorder(
                Color.primary.opacity(Layout.borderOpacity),
                lineWidth: Layout.borderWidth
            )
        )
    }

    private var fallback: some View {
        ZStack {
            Color.accentColor.opacity(Layout.backgroundOpacity)
            Text(initial)
                .font(.system(size: size * Layout.fontScale, weight: .semibold))
                .foregroundStyle(Color.accentColor)
        }
    }
}
