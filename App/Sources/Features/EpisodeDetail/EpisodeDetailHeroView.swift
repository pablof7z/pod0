import SwiftUI
// MARK: - EpisodeDetailHeroView

/// Magazine-cover layout for an episode in `.detail` mode (UX-03 §6.1):
/// hero artwork + title block, action row, italic summary lede, chapter
/// list, show-notes prose, and the "Read transcript" CTA.
///
/// Owns no state; all interactions bubble up via callbacks. The play button
/// label flips between Play / Resume based on `playbackPosition`.
struct EpisodeDetailHeroView: View {
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    let episode: Episode
    let showName: String
    let showImageURL: URL?
    let isPlayed: Bool
    let onPlay: () -> Void
    let onPlayChapter: (Episode.Chapter) -> Void
    var isInQueue: Bool = false
    var onAddToQueue: () -> Void = {}
    var activeChapterID: UUID? = nil
    var downloadProgress: Double? = nil
    var downloadJobState: WorkJobState? = nil
    var preparationStatus: EpisodePreparationStatus? = nil
    var onPreparationAction: (EpisodePreparationActionKind, WorkflowJobProjection?) -> Void = { _, _ in }
    var onToggleDownload: () -> Void = {}

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                hero
                actionRow
                if let preparationStatus {
                    EpisodePreparationStatusView(status: preparationStatus, onAction: onPreparationAction)
                }
                if !descriptionPlain.isEmpty {
                    summarySection
                }
                if let chapters = navigableChapters, !chapters.isEmpty {
                    chaptersSection(chapters)
                }
                if !descriptionPlain.isEmpty {
                    showNotesSection
                }
                Spacer(minLength: 80)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.top, AppTheme.Spacing.md)
        }
    }

    // MARK: Hero

    private var hero: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            artwork
            VStack(alignment: .leading, spacing: 6) {
                Text(episode.title)
                    .font(AppTheme.Typography.title)
                    .foregroundStyle(.primary)
                Text(showName)
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .foregroundStyle(.secondary)
                Text(metadataLine)
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private var artwork: some View {
        let url = episode.imageURL ?? showImageURL
        return Group {
            if let url {
                CachedAsyncImage(
                    url: url,
                    targetSize: CGSize(width: 220, height: 220)
                ) { phase in
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
        .frame(width: 110, height: 110)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
    }

    private var artworkPlaceholder: some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(.secondary)
        }
    }

    private var metadataLine: String {
        let f = DateFormatter()
        f.dateFormat = "MMM d, yyyy"
        let date = f.string(from: episode.pubDate)
        if let duration = episode.duration {
            let mins = Int(duration / 60)
            let h = mins / 60
            let m = mins % 60
            let durString = h > 0 ? "\(h)h \(m)m" : "\(m)m"
            return "\(date) · \(durString)"
        }
        return date
    }

    // MARK: Sections

    private var actionRow: some View {
        actionLayout {
            Button(action: onPlay) {
                Label(playLabel, systemImage: "play.fill")
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .frame(maxWidth: dynamicTypeSize.isAccessibilitySize ? .infinity : nil, alignment: .leading)
                    .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: true)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)

            Button(action: onAddToQueue) {
                Label(
                    isInQueue ? "Queued" : "Queue",
                    systemImage: isInQueue ? "checkmark" : "text.badge.plus"
                )
                .font(.system(.subheadline, design: .rounded).weight(.medium))
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .frame(maxWidth: dynamicTypeSize.isAccessibilitySize ? .infinity : nil, alignment: .leading)
                .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: !isInQueue)
            }
            .buttonStyle(.plain)
            .foregroundStyle(isInQueue ? .secondary : .primary)
            .disabled(isInQueue)
            .accessibilityHint(isInQueue ? "Already in your Up Next queue" : "Add to Up Next queue")

            downloadPill
        }
    }

    private var actionLayout: AnyLayout {
        dynamicTypeSize.isAccessibilitySize
            ? AnyLayout(VStackLayout(alignment: .leading, spacing: AppTheme.Spacing.md))
            : AnyLayout(HStackLayout(spacing: AppTheme.Spacing.md))
    }

    @ViewBuilder
    private var downloadPill: some View {
        if case .downloaded = episode.downloadState {
            Label("Downloaded", systemImage: "checkmark.circle.fill")
                .font(.system(.subheadline, design: .rounded).weight(.medium))
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .frame(maxWidth: dynamicTypeSize.isAccessibilitySize ? .infinity : nil, alignment: .leading)
                .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: false)
                .foregroundStyle(.secondary)
                .accessibilityLabel("Downloaded")
        } else if let downloadProgress {
            Button(action: onToggleDownload) {
                let pct = Int((downloadProgress.clamped01 * 100).rounded())
                Label("Downloading \(pct)%", systemImage: "arrow.down.circle.fill")
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .frame(maxWidth: dynamicTypeSize.isAccessibilitySize ? .infinity : nil, alignment: .leading)
                    .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: true)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)
            .accessibilityLabel("Downloading, \(Int((downloadProgress.clamped01 * 100).rounded())) percent")
            .accessibilityHint("Cancels the download")
        } else if downloadJobState == .pending || downloadJobState == .leased ||
                    downloadJobState == .running || downloadJobState == .retryScheduled {
            Button(action: onToggleDownload) {
                Label("Queued", systemImage: "clock")
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .frame(maxWidth: dynamicTypeSize.isAccessibilitySize ? .infinity : nil, alignment: .leading)
                    .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: true)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .accessibilityHint("Cancels the download")
        } else if downloadJobState == .blocked || downloadJobState == .failedPermanent {
            Button(action: onToggleDownload) {
                Label("Retry", systemImage: "arrow.clockwise")
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .frame(maxWidth: dynamicTypeSize.isAccessibilitySize ? .infinity : nil, alignment: .leading)
                    .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: true)
            }
            .buttonStyle(.plain)
            .foregroundStyle(AppTheme.Tint.error)
            .accessibilityLabel("Download failed")
            .accessibilityHint("Retries the download")
        } else {
            Button(action: onToggleDownload) {
                Label("Download", systemImage: "arrow.down.circle")
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .frame(maxWidth: dynamicTypeSize.isAccessibilitySize ? .infinity : nil, alignment: .leading)
                    .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: true)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)
            .accessibilityHint("Download episode for offline listening")
        }
    }

    private var playLabel: String {
        if isPlayed { return "Play again" }
        return episode.playbackPosition > 0 ? "Resume" : "Play"
    }

    private var summarySection: some View {
        VStack(alignment: .leading, spacing: 6) {
            sectionDivider("Summary")
            Text("\u{201C}\(summaryLede)\u{201D}")
                .font(AppTheme.Typography.title3.italic())
                .lineSpacing(8)
                .foregroundStyle(.primary)
                .lineLimit(4)
        }
    }

    private var summaryLede: String {
        let trimmed = descriptionPlain.trimmingCharacters(in: .whitespacesAndNewlines)
        let sentence = trimmed.split(whereSeparator: { ".!?".contains($0) }).first.map(String.init) ?? trimmed
        return sentence.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func chaptersSection(_ chapters: [Episode.Chapter]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            sectionDivider("Chapters")
            ForEach(chapters) { chapter in
                let isActive = chapter.id == activeChapterID
                Button {
                    onPlayChapter(chapter)
                } label: {
                    HStack(alignment: .firstTextBaseline) {
                        Text(formatTimestamp(chapter.startTime))
                            .font(.system(.footnote, design: .monospaced).weight(.medium))
                            .foregroundStyle(isActive ? Color.accentColor : .secondary)
                            .frame(width: 64, alignment: .leading)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(chapter.title)
                                .font(AppTheme.Typography.body)
                                .foregroundStyle(isActive ? Color.accentColor : .primary)
                                .multilineTextAlignment(.leading)
                            if let summary = chapter.summary?.trimmingCharacters(in: .whitespacesAndNewlines),
                               !summary.isEmpty {
                                Text(summary)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .multilineTextAlignment(.leading)
                                    .lineLimit(isActive ? 4 : 2)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                        Spacer()
                        if isActive {
                            Image(systemName: "waveform")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(Color.accentColor)
                                .symbolEffect(.variableColor.iterative, options: .repeating)
                                .accessibilityLabel("Now playing")
                        }
                    }
                    .padding(.vertical, 4)
                }
                .buttonStyle(.plain)
            }
        }
    }

    private var navigableChapters: [Episode.Chapter]? {
        episode.chapters?.filter(\.includeInTableOfContents)
    }

    private var showNotesSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            sectionDivider("Show notes")
            Text(descriptionPlain)
                .font(AppTheme.Typography.body)
                .lineSpacing(7)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: Helpers

    private func sectionDivider(_ label: String) -> some View {
        HStack(spacing: 8) {
            Rectangle().fill(AppTheme.Tint.dimmed).frame(width: 18, height: 1)
            Text(label)
                .font(.system(.caption, design: .rounded).weight(.semibold))
                .tracking(0.6)
                .foregroundStyle(.secondary)
            Rectangle().fill(AppTheme.Tint.hairline).frame(height: 1)
        }
        .padding(.top, 8)
    }

    private var descriptionPlain: String {
        episode.plainTextDescription
    }

    private func formatTimestamp(_ t: TimeInterval) -> String {
        let total = Int(t)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%02d:%02d:%02d", h, m, s)
            : String(format: "%02d:%02d", m, s)
    }
}

// `Double.clamped01` lives in `Design/NumberExtensions.swift`.

// MARK: - Preview

#Preview {
    let subID = UUID()
    let episode = Episode(
        podcastID: subID,
        guid: "preview-1",
        title: "How to Think About Keto",
        description: "<p>Tim sits down with <b>Peter Attia, MD</b> to revisit a topic the show has circled for years: ketones and metabolic flexibility.</p>",
        pubDate: Date(timeIntervalSince1970: 1_714_780_800),
        duration: 60 * 60 * 2 + 14 * 60,
        enclosureURL: URL(string: "https://traffic.megaphone.fm/HSW1234567890.mp3")!,
        chapters: [
            .init(startTime: 0, title: "Cold open"),
            .init(startTime: 252, title: "Why ketones matter"),
            .init(startTime: 1720, title: "The Inuit objection"),
            .init(startTime: 4810, title: "Practical protocols")
        ]
    )
    return NavigationStack {
        EpisodeDetailHeroView(
            episode: episode,
            showName: "The Tim Ferriss Show",
            showImageURL: nil,
            isPlayed: false,
            onPlay: {},
            onPlayChapter: { _ in }
        )
    }
}
