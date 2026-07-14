import SwiftUI

// MARK: - ClippingsCard

/// Full-width card for a single user clip in the Clippings feed.
/// Tapping plays the clip from its start position; long-press opens a context
/// menu with play / share / open-episode / delete actions.
struct ClippingsCard: View {

    let clip: Clip
    let episode: Episode?
    let podcast: Podcast?
    let onPlay: () -> Void
    let onOpenEpisode: () -> Void
    let onDelete: () -> Void

    @State private var shareClip: Clip?

    var body: some View {
        Button(action: { Haptics.selection(); onPlay() }) {
            cardContent
        }
        .buttonStyle(.pressable(scale: 0.98))
        .contextMenu { contextMenuContent }
        .sheet(item: $shareClip) { c in
            if let ep = episode, let pod = podcast {
                ClipShareSheet(clip: c, episode: ep, podcast: pod)
            }
        }
    }

    // MARK: - Layout

    private var cardContent: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            headerRow
            quoteBlock
            footerRow
        }
        .padding(AppTheme.Spacing.md)
        .background(
            Color(.secondarySystemBackground),
            in: RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
        )
    }

    private var headerRow: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            artwork
            VStack(alignment: .leading, spacing: 2) {
                if let showName = podcast?.title {
                    Text(showName)
                        .font(AppTheme.Typography.caption)
                        .tracking(0.8)
                        .textCase(.uppercase)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                if let title = episode?.title {
                    Text(title)
                        .font(AppTheme.Typography.headline)
                        .foregroundStyle(.primary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
            if clip.source != .touch {
                sourceBadge
            }
        }
    }

    private var artwork: some View {
        let url = episode?.imageURL ?? podcast?.imageURL
        return Group {
            if let url {
                CachedAsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let img): img.resizable().scaledToFill()
                    default: artworkPlaceholder
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
        ZStack {
            Color(.tertiarySystemFill)
            Image(systemName: "headphones")
                .font(.system(size: 16, weight: .light))
                .foregroundStyle(.secondary)
        }
    }

    private var quoteBlock: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            if let caption = clip.caption, !caption.isEmpty {
                Text(caption)
                    .font(.system(.caption, design: .rounded).weight(.semibold))
                    .tracking(0.4)
                    .textCase(.uppercase)
                    .foregroundStyle(.tertiary)
            }
            Text("\u{201C}\(displayText)\u{201D}")
                .font(.system(.body, design: .default))
                .foregroundStyle(.primary)
                .lineLimit(4)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(AppTheme.Spacing.md)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color(.systemBackground))
        )
        .overlay(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .strokeBorder(Color.secondary.opacity(0.12), lineWidth: 0.5)
        )
    }

    private var footerRow: some View {
        HStack(spacing: 4) {
            Text(timestampLabel)
                .font(AppTheme.Typography.mono)
                .foregroundStyle(.secondary)
                .monospacedDigit()
            Text("·")
                .font(AppTheme.Typography.mono)
                .foregroundStyle(.tertiary)
            Text(durationLabel)
                .font(AppTheme.Typography.mono)
                .foregroundStyle(.secondary)
            Spacer()
            Text(relativeDate)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.tertiary)
        }
    }

    @ViewBuilder
    private var sourceBadge: some View {
        let (icon, label) = sourceInfo
        Label(label, systemImage: icon)
            .font(.system(.caption2, design: .rounded).weight(.semibold))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(Capsule().fill(Color(.tertiarySystemFill)))
    }

    // MARK: - Context menu

    @ViewBuilder
    private var contextMenuContent: some View {
        Button { Haptics.selection(); onPlay() } label: {
            Label("Play Clip", systemImage: "play.circle")
        }

        if episode != nil, podcast != nil {
            Button { Haptics.selection(); shareClip = clip } label: {
                Label("Share…", systemImage: "square.and.arrow.up")
            }
        }

        Button { Haptics.selection(); onOpenEpisode() } label: {
            Label("Open Episode", systemImage: "doc.text")
        }
        .disabled(episode == nil)

        Divider()

        Button(role: .destructive) { Haptics.delete(); onDelete() } label: {
            Label("Delete", systemImage: "trash")
        }
    }

    // MARK: - Helpers

    private var displayText: String {
        if !clip.transcriptText.isEmpty { return clip.transcriptText }
        if let title = episode?.title { return "Clip from \(title)" }
        return "Audio clip"
    }

    private var sourceInfo: (String, String) {
        switch clip.source {
        case .touch:     return ("hand.tap", "Manual")
        case .auto:      return ("sparkles", "Auto")
        case .headphone: return ("airpodspro", "AirPods")
        case .carplay:   return ("car", "CarPlay")
        case .watch:     return ("applewatch", "Watch")
        case .siri:      return ("waveform", "Siri")
        case .agent:     return ("sparkles", "Agent")
        }
    }

    private var timestampLabel: String {
        "\(format(ms: clip.startMs)) \u{2192} \(format(ms: clip.endMs))"
    }

    private var durationLabel: String {
        let total = max(0, Int(clip.durationSeconds.rounded()))
        let m = total / 60
        let s = total % 60
        return m > 0 ? "\(m):\(String(format: "%02d", s))" : "\(s)s"
    }

    private var relativeDate: String {
        let interval = Date().timeIntervalSince(clip.createdAt)
        if interval < 60 { return "just now" }
        if interval < 3_600 { return "\(Int(interval / 60))m ago" }
        if interval < 86_400 { return "\(Int(interval / 3_600))h ago" }
        let days = Int(interval / 86_400)
        if days == 1 { return "yesterday" }
        if days < 7 { return "\(days)d ago" }
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .none
        return f.string(from: clip.createdAt)
    }

    private func format(ms: Int) -> String {
        let total = ms / 1000
        let h = total / 3_600
        let m = (total % 3_600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%d:%02d:%02d", h, m, s)
            : String(format: "%d:%02d", m, s)
    }
}

// MARK: - Preview

#Preview {
    let podID = UUID()
    let epID = UUID()
    let clip = Clip(
        episodeID: epID,
        subscriptionID: podID,
        startMs: 14 * 60_000 + 31_000,
        endMs: 14 * 60_000 + 58_000,
        caption: "On metabolism",
        transcriptText: "Metabolic flexibility isn't a diet — it's a property of the mitochondria and how well it can switch between fat and glucose oxidation."
    )
    let episode = Episode(
        podcastID: podID,
        guid: "preview",
        title: "How to Think About Keto",
        pubDate: Date(),
        enclosureURL: URL(string: "https://example.com/x.mp3")!
    )
    let podcast = Podcast(
        id: podID,
        feedURL: URL(string: "https://example.com/feed")!,
        title: "The Peter Attia Drive"
    )
    return ClippingsCard(
        clip: clip,
        episode: episode,
        podcast: podcast,
        onPlay: {},
        onOpenEpisode: {},
        onDelete: {}
    )
    .padding()
    .background(Color(.systemGroupedBackground))
}
