import SwiftUI

// MARK: - ClipImageCardView
//
// Editorial 1080×1080 share card per UX-03 §6.5. Mirrors `QuoteShareView`'s
// composition (artwork, show name, italic pull-quote, speaker chip,
// timestamp, deep-link footer) but rendered at the larger square size used
// by `ClipExporter.exportImage`. The card is intentionally self-contained:
// callers hand in fully-resolved strings + pre-fetched artwork so the
// `ImageRenderer` snapshot is deterministic.
struct ClipImageCardView: View {
    let showName: String
    let episodeTitle: String
    let artwork: UIImage?
    let pullQuote: String
    let speakerName: String?
    let timestamp: String
    let deepLink: String
    let style: ClipExporter.SubtitleStyle

    /// Soft truncation for the pull-quote to keep the layout readable.
    private static let quoteCharLimit = 140

    var body: some View {
        ZStack {
            background
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                header
                Spacer(minLength: AppTheme.Spacing.md)
                quote
                Spacer(minLength: AppTheme.Spacing.md)
                footer
            }
            .padding(AppTheme.Spacing.xl)
        }
        .frame(width: 1080, height: 1080)
    }

    // MARK: - Sections

    private var header: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            artworkView
            VStack(alignment: .leading, spacing: 4) {
                Text(showName)
                    .font(.system(size: 32, design: .rounded).weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                Text(episodeTitle)
                    .font(.system(size: 22))
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            Spacer(minLength: 0)
        }
    }

    private var quote: some View {
        Text("\u{201C}\(displayQuote)\u{201D}")
            .font(quoteFont)
            .foregroundStyle(.primary)
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: true)
            .lineSpacing(8)
    }

    private var footer: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            HStack(spacing: AppTheme.Spacing.sm) {
                if let name = speakerName, !name.isEmpty {
                    Text("\u{2014} \(name)")
                        .font(.system(size: 26, design: .rounded).weight(.medium))
                        .foregroundStyle(.secondary)
                }
                if speakerName != nil { Text("\u{00B7}").foregroundStyle(.tertiary) }
                Text(timestamp)
                    .font(.system(size: 26, design: .monospaced))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
            }
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "waveform")
                    .font(.system(size: 22, weight: .semibold))
                    .foregroundStyle(.tint)
                Text(deepLink)
                    .font(.system(size: 22, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 0)
            }
        }
    }

    private var background: some View {
        LinearGradient(
            colors: [
                Color(.systemBackground),
                Color(.secondarySystemBackground)
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
    }

    private var artworkView: some View {
        Group {
            if let artwork {
                Image(uiImage: artwork)
                    .resizable()
                    .scaledToFill()
            } else {
                LinearGradient(
                    colors: [Color.orange.opacity(0.7), Color.purple.opacity(0.6)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
                .overlay(
                    Text(String(showName.prefix(1)))
                        .font(.system(size: 60, design: .rounded).weight(.bold))
                        .foregroundStyle(.white)
                )
            }
        }
        .frame(width: 140, height: 140)
        .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
    }

    // MARK: - Helpers

    private var displayQuote: String {
        let trimmed = pullQuote.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count > Self.quoteCharLimit else { return trimmed }
        // Truncate on the last word boundary that fits inside the limit so
        // the card never ends mid-word. Falls back to a hard cut when the
        // text has no whitespace (rare; e.g. a single long URL).
        let cap = trimmed.index(trimmed.startIndex, offsetBy: Self.quoteCharLimit)
        let head = trimmed[..<cap]
        if let lastSpace = head.lastIndex(where: { $0.isWhitespace }) {
            return String(head[..<lastSpace]) + "\u{2026}"
        }
        return String(head) + "\u{2026}"
    }

    private var quoteFont: Font {
        switch style {
        case .editorial:
            return AppTheme.Typography.title.italic()
        case .bold:
            return .system(size: 64).weight(.semibold)
        }
    }
}

// MARK: - Preview

#Preview {
    ClipImageCardView(
        showName: "The Tim Ferriss Show",
        episodeTitle: "How to Think About Keto",
        artwork: nil,
        pullQuote: "Metabolic flexibility isn't a diet — it's a property of the mitochondria.",
        speakerName: "Peter Attia",
        timestamp: "14:31",
        deepLink: "podcastr://clip/8E2C0E1A",
        style: .editorial
    )
    .scaleEffect(0.3)
}
