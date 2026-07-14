import SwiftUI

// MARK: - ChapterRailView

/// Vertical Liquid Glass chapter rail per UX-03 §4 + UX-15 §5. Each chapter
/// renders as a small glass capsule; the **active** chapter morphs via
/// `glassEffectID` into a labeled pill — reads as one liquid bead tracking
/// scroll position.
///
/// Tapping a chapter calls `onTap`. The parent owns the playhead.
struct ChapterRailView: View {
    let chapters: [Episode.Chapter]
    let activeID: UUID?
    let onTap: (Episode.Chapter) -> Void

    /// Shared namespace required by `glassEffectID` for morph continuity.
    @Namespace private var glassNamespace

    var body: some View {
        GlassEffectContainer(spacing: 12) {
            VStack(spacing: 12) {
                ForEach(chapters) { chapter in
                    chapterRow(chapter)
                }
            }
            .padding(.vertical, AppTheme.Spacing.md)
            .padding(.horizontal, AppTheme.Spacing.sm)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Chapter rail")
    }

    @ViewBuilder
    private func chapterRow(_ chapter: Episode.Chapter) -> some View {
        let isActive = chapter.id == activeID
        Button {
            onTap(chapter)
        } label: {
            HStack(spacing: 6) {
                Circle()
                    .fill(Color.accentColor.opacity(isActive ? 0.9 : 0.5))
                    .frame(width: 6, height: 6)
                if isActive {
                    Text(chapter.title)
                        .font(.system(.caption, design: .rounded).weight(.medium))
                        .lineLimit(1)
                        .transition(.opacity.combined(with: .scale))
                }
            }
            .padding(.horizontal, isActive ? 10 : 6)
            .padding(.vertical, 6)
            .glassEffect(
                .regular.interactive(),
                in: .capsule
            )
            .glassEffectID(chapter.id, in: glassNamespace)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(chapter.title)
        .accessibilityValue(isActive ? "Active" : "")
    }
}

// MARK: - Preview

#Preview {
    let chapters: [Episode.Chapter] = [
        .init(startTime: 0, title: "Cold open"),
        .init(startTime: 252, title: "Why ketones matter"),
        .init(startTime: 1720, title: "The Inuit objection"),
        .init(startTime: 4810, title: "Practical protocols")
    ]
    return HStack {
        Spacer()
        ChapterRailView(
            chapters: chapters,
            activeID: chapters[1].id,
            onTap: { _ in }
        )
        .padding(.trailing, AppTheme.Spacing.md)
    }
    .frame(maxHeight: .infinity)
    .background(Color(.systemBackground))
}
