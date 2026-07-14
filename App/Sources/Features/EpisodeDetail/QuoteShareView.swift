import SwiftUI

// MARK: - QuoteShareView

/// Exportable quote card per UX-03 §6.5. Used inside a SwiftUI sheet; the
/// rendered card can be passed to `ImageRenderer` for image export.
///
/// Three actions: image, audio+sub, link. Wiring lives one level up — this
/// view is just the visual.
struct QuoteShareView: View {
    let episode: Episode
    let showName: String
    let showImageURL: URL?
    let segment: Segment
    let speaker: Speaker?
    let deepLink: String
    /// Hook the parent surface up to its audio-clip-with-subtitles
    /// pipeline. When `nil` the "Audio + Sub" button is hidden because
    /// rendering an audio clip with burned-in subtitles is heavy
    /// (video composition) and the parent doesn't have it wired —
    /// better to hide than to look like a dead button.
    var onShareAudioWithSubtitles: (() -> Void)? = nil

    @Environment(\.displayScale) private var displayScale

    var body: some View {
        VStack(spacing: AppTheme.Spacing.lg) {
            card
                .frame(maxWidth: 360)
            actionRow
        }
        .padding(AppTheme.Spacing.lg)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
    }

    // MARK: - Card

    private var card: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            HStack(spacing: AppTheme.Spacing.sm) {
                artwork
                VStack(alignment: .leading, spacing: 2) {
                    Text(showName)
                        .font(.system(.subheadline, design: .rounded).weight(.semibold))
                        .foregroundStyle(.primary)
                    Text(formattedDate)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }

            Text("\u{201C}\(segment.text)\u{201D}")
                .font(AppTheme.Typography.title)
                .foregroundStyle(.primary)
                .fixedSize(horizontal: false, vertical: true)

            HStack {
                Text("\u{2014} \(speaker?.displayName ?? speaker?.label ?? "Unknown"), \(formattedTimestamp)")
                    .font(.system(.footnote, design: .rounded).weight(.medium))
                    .foregroundStyle(.secondary)
                Spacer()
            }

            Text(deepLink)
                .font(.system(.caption2, design: .monospaced))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .padding(AppTheme.Spacing.lg)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                .fill(Color(.systemBackground))
        )
        .overlay(
            RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                .strokeBorder(Color.secondary.opacity(0.18), lineWidth: 0.5)
        )
        .shadow(color: Color.black.opacity(0.08), radius: 24, y: 8)
    }

    private var artwork: some View {
        let url = episode.imageURL ?? showImageURL
        return Group {
            if let url {
                CachedAsyncImage(url: url) { phase in
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
        .frame(width: 44, height: 44)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
    }

    private var artworkPlaceholder: some View {
        RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
            .fill(LinearGradient(
                colors: [Color.orange.opacity(0.7), Color.purple.opacity(0.6)],
                startPoint: .topLeading, endPoint: .bottomTrailing
            ))
            .overlay(
                Text(String(showName.prefix(1)))
                    .font(.system(.headline, design: .rounded).weight(.bold))
                    .foregroundStyle(.white)
            )
    }

    private var formattedDate: String {
        let f = DateFormatter()
        f.dateFormat = "MMM d, yyyy"
        return f.string(from: episode.pubDate)
    }

    private var formattedTimestamp: String {
        let total = Int(segment.start)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, s)
            : String(format: "%d:%02d", m, s)
    }

    // MARK: - Actions

    private var actionRow: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            actionButton(label: "Image", systemImage: "photo", action: shareImage)
            if let handler = onShareAudioWithSubtitles {
                actionButton(label: "Audio + Sub", systemImage: "waveform") {
                    Haptics.light()
                    handler()
                }
            }
            actionButton(label: "Link", systemImage: "link", action: shareLink)
        }
    }

    private func actionButton(label: String, systemImage: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 6) {
                Image(systemName: systemImage)
                    .font(.title3)
                Text(label)
                    .font(.caption)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
            .background(
                RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                    .fill(Color(.secondarySystemBackground))
            )
        }
        .buttonStyle(.plain)
        .foregroundStyle(.primary)
    }

    // MARK: - Share handlers

    @MainActor
    private func shareImage() {
        // Render the same card view at the same logical width the user
        // sees in the sheet. Without an explicit frame the renderer
        // would size the view to its intrinsic content, which wraps
        // weirdly for long quotes.
        let renderer = ImageRenderer(content: card.frame(width: 360))
        renderer.scale = displayScale
        guard let image = renderer.uiImage else { return }
        Haptics.light()
        // Image attaches the deep link as a second activity item so the
        // recipient can long-press the share sheet preview and still
        // get back to the quote — image-only would lose all context.
        var items: [Any] = [image]
        if let url = URL(string: deepLink) {
            items.append(url)
        }
        SystemShareSheet.present(items: items)
    }

    @MainActor
    private func shareLink() {
        Haptics.light()
        if let url = URL(string: deepLink) {
            SystemShareSheet.present(items: [url])
        } else {
            SystemShareSheet.present(items: [deepLink])
        }
    }
}

// MARK: - Preview

#Preview {
    let subID = UUID()
    let episode = Episode(
        podcastID: subID,
        guid: "preview-1",
        title: "How to Think About Keto",
        pubDate: Date(timeIntervalSince1970: 1_714_780_800),
        duration: 60 * 60,
        enclosureURL: URL(string: "https://traffic.megaphone.fm/HSW1234567890.mp3")!
    )
    let peter = Speaker(label: "Peter Attia", displayName: "Peter Attia")
    let segment = Segment(
        start: 252,
        end: 262,
        speakerID: peter.id,
        text: "We're measuring the body's ability to switch substrate utilization on demand."
    )
    return QuoteShareView(
        episode: episode,
        showName: "The Tim Ferriss Show",
        showImageURL: nil,
        segment: segment,
        speaker: peter,
        deepLink: "podcast.app/e/\(episode.id.uuidString.prefix(8))?t=\(Int(segment.start))"
    )
}
