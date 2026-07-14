import SwiftUI

// MARK: - EpisodeDetailHeroView

/// Magazine-cover layout for an episode in `.detail` mode (UX-03 §6.1):
/// hero artwork + title block, action row, italic summary lede, chapter
/// list, show-notes prose, and the "Read transcript" CTA.
///
/// Owns no state; all interactions bubble up via callbacks. The play button
/// label flips between Play / Resume based on `playbackPosition`.
struct EpisodeDetailHeroView: View {
    let episode: Episode
    let showName: String
    let showImageURL: URL?
    let isPlayed: Bool
    let onPlay: () -> Void
    let onPlayChapter: (Episode.Chapter) -> Void
    /// `true` when this episode is already queued in `PlaybackState.queue` —
    /// drives the "Queued" disabled state on the Add to Queue button.
    var isInQueue: Bool = false
    /// Tap handler for the new Add to Queue affordance. No-op default
    /// preserves call sites that don't yet wire it up.
    var onAddToQueue: () -> Void = {}
    /// Active chapter id when this episode is currently playing — drives
    /// the live "you are here" highlight in the chapters list. `nil` when
    /// playback is on a different episode (or no chapters); the list
    /// renders flat in that case.
    var activeChapterID: UUID? = nil
    /// Live download progress in `0...1`, observed from
    /// `EpisodeDownloadService`. Drives the inline progress pill on the
    /// action row so the user sees a smooth "Downloading 42%" badge while
    /// the file is in flight (the Episode's persisted `downloadState` only
    /// updates at coarse transitions to spare AppStateStore).
    var downloadProgress: Double? = nil
    /// Download / cancel / delete handler bound by the parent. The hero
    /// flips the affordance based on the episode's `downloadState`.
    var onToggleDownload: () -> Void = {}

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                hero
                actionRow
                if !descriptionPlain.isEmpty {
                    summarySection
                }
                if let chapters = navigableChapters, !chapters.isEmpty {
                    chaptersSection(chapters)
                }
                if !descriptionPlain.isEmpty {
                    showNotesSection
                }
                // Nostr-anchored comments section. Tightly coupled to the
                // hero for v1 — a clean injection slot is on the list when
                // we add comments to clip surfaces too. Hides itself when
                // the episode lacks a Podcasting 2.0 GUID (no global
                // address ⇒ no comments target).
                EpisodeCommentsSection(episode: episode)
                    .padding(.top, AppTheme.Spacing.md)
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
        HStack(spacing: AppTheme.Spacing.md) {
            Button(action: onPlay) {
                Label(playLabel, systemImage: "play.fill")
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: true)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)

            // Add to Queue / Queued — sits next to Play so the queue is
            // a one-tap action instead of buried inside a long-press
            // context menu. Flips to a disabled "Queued" state once the
            // episode is in `PlaybackState.queue`.
            Button(action: onAddToQueue) {
                Label(
                    isInQueue ? "Queued" : "Queue",
                    systemImage: isInQueue ? "checkmark" : "text.badge.plus"
                )
                .font(.system(.subheadline, design: .rounded).weight(.medium))
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: !isInQueue)
            }
            .buttonStyle(.plain)
            .foregroundStyle(isInQueue ? .secondary : .primary)
            .disabled(isInQueue)
            .accessibilityHint(isInQueue ? "Already in your Up Next queue" : "Add to Up Next queue")

            // Download pill — promoted from the menu so the user sees a
            // primary affordance and a live progress badge while bytes are
            // moving. Flips between Download / Cancel (with %) / Downloaded.
            downloadPill
        }
    }

    @ViewBuilder
    private var downloadPill: some View {
        switch episode.downloadState {
        case .notDownloaded, .queued:
            Button(action: onToggleDownload) {
                Label("Download", systemImage: "arrow.down.circle")
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: true)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)
            .accessibilityHint("Download episode for offline listening")
        case .downloading(let persistedProgress, _):
            Button(action: onToggleDownload) {
                let live = downloadProgress ?? persistedProgress
                let pct = Int((live.clamped01 * 100).rounded())
                Label("Downloading \(pct)%", systemImage: "arrow.down.circle.fill")
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: true)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)
            .accessibilityLabel("Downloading, \(Int(((downloadProgress ?? persistedProgress).clamped01 * 100).rounded())) percent")
            .accessibilityHint("Cancels the download")
        case .downloaded:
            Label("Downloaded", systemImage: "checkmark.circle.fill")
                .font(.system(.subheadline, design: .rounded).weight(.medium))
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: false)
                .foregroundStyle(.secondary)
                .accessibilityLabel("Downloaded")
        case .failed:
            Button(action: onToggleDownload) {
                Label("Retry", systemImage: "arrow.clockwise")
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .glassSurface(cornerRadius: AppTheme.Corner.pill, interactive: true)
            }
            .buttonStyle(.plain)
            .foregroundStyle(AppTheme.Tint.error)
            .accessibilityLabel("Download failed")
            .accessibilityHint("Retries the download")
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
        EpisodeShowNotesFormatter.plainText(from: episode.description)
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
