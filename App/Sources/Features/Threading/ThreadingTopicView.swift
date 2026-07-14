import SwiftUI

// MARK: - Threading topic view

/// UX-09 §6.3 timeline detail. Vertical list of every recorded mention of a
/// `ThreadingTopic`, ordered newest-first, each row carrying the host show,
/// episode title, transcript snippet, a tappable timestamp chip, a
/// confidence dot, and an amber seam for contradictions.
///
/// Tap on a timestamp chip dispatches `play_episode` via `PlaybackState`
/// (set the episode + seek + play) — same contract the wiki citation chip
/// uses, so the threading and wiki surfaces feel like one fabric.
///
/// A glass capsule footer hands off to the wiki page for the same slug,
/// matching the §3 boundary: threading is *episodic recall*; wiki is
/// *synthesised knowledge*.
struct ThreadingTopicView: View {

    let topicID: UUID

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback
    @State private var service = ThreadingInferenceService.shared
    /// Cached wiki page for this topic's slug, looked up on appear. Drives
    /// whether the bottom handoff button renders at all — a styled "open
    /// wiki entry" pill that doesn't open anything is worse than no pill.
    @State private var existingWikiPage: WikiPage?
    /// Set when the user taps the wiki handoff; presents the page in a
    /// sheet so this view doesn't have to coordinate with the parent
    /// NavigationStack's `.navigationDestination` plumbing.
    @State private var presentedWikiPage: WikiPage?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                if let topic {
                    header(topic)
                    if let definition = topic.definition, !definition.isEmpty {
                        Text(definition)
                            .font(AppTheme.Typography.body)
                            .italic()
                            .foregroundStyle(.primary)
                            .lineSpacing(4)
                    }
                    Divider().overlay(Color.primary.opacity(0.18))
                    timeline
                    wikiHandoff(for: topic)
                } else {
                    missingTopic
                }
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollIndicators(.hidden)
        .background(paperBackground)
        .navigationBarTitleDisplayMode(.inline)
        .task { service.attach(store: store) }
        .task(id: topicID) { loadWikiPage() }
        .sheet(item: $presentedWikiPage) { page in
            NavigationStack {
                WikiPageView(
                    page: page,
                    storage: .shared,
                    onDeleted: { _ in
                        presentedWikiPage = nil
                        existingWikiPage = nil
                    },
                    onRegenerated: { newPage in
                        presentedWikiPage = newPage
                        existingWikiPage = newPage
                    }
                )
            }
        }
    }

    /// Tries to read the wiki page that matches this topic's slug. Treated
    /// as global scope — threading topics span the library, not a single
    /// show. Failure is silent: the handoff just doesn't render.
    private func loadWikiPage() {
        guard let topic else {
            existingWikiPage = nil
            return
        }
        existingWikiPage = try? WikiStorage.shared.read(
            slug: WikiPage.normalize(slug: topic.slug),
            scope: .global
        )
    }

    // MARK: - Sections

    private func header(_ topic: ThreadingTopic) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(topic.displayName)
                .font(AppTheme.Typography.largeTitle)
                .tracking(-0.4)
                .foregroundStyle(.primary)
            Text(metadataLine(for: topic))
                .font(.caption)
                .foregroundStyle(.secondary)
                .textCase(.uppercase)
                .tracking(0.5)
        }
        .padding(.top, 8)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(headerAccessibilityLabel(for: topic))
    }

    /// VoiceOver assembly for the header. Mirrors the visual metadata
    /// line — singular/plural-matched mention count, and contradictions
    /// only when present.
    private func headerAccessibilityLabel(for topic: ThreadingTopic) -> String {
        var parts: [String] = [topic.displayName]
        let mentions = topic.episodeMentionCount
        parts.append("\(mentions) episode\(mentions == 1 ? "" : "s")")
        if topic.contradictionCount > 0 {
            let n = topic.contradictionCount
            parts.append("\(n) contradiction\(n == 1 ? "" : "s")")
        }
        return parts.joined(separator: ", ")
    }

    private var timeline: some View {
        // Read through the service so the spec'd surface
        // (`ThreadingInferenceService.mentions(forTopic:)`) is the live read
        // path — the service is attached on `.task` above so the lookup
        // resolves to the running store rather than the empty fallback.
        let mentions = service.mentions(forTopic: topicID)
        return VStack(alignment: .leading, spacing: 18) {
            Text("Timeline")
                .font(.caption)
                .foregroundStyle(.secondary)
                .textCase(.uppercase)
                .tracking(0.6)
            if mentions.isEmpty {
                Text("No mentions recorded yet for this topic.")
                    .font(.callout)
                    .foregroundStyle(.tertiary)
            } else {
                ForEach(mentions) { mention in
                    ThreadingMentionRow(
                        mention: mention,
                        episode: store.episode(id: mention.episodeID),
                        subscriptionTitle: subscriptionTitle(for: mention),
                        onPlay: { play(mention: mention) }
                    )
                }
            }
        }
    }

    private var missingTopic: some View {
        VStack(alignment: .center, spacing: 12) {
            Image(systemName: "questionmark.bubble")
                .font(.system(size: 44, weight: .ultraLight))
                .foregroundStyle(.tertiary)
            Text("This topic is no longer in your library.")
                .font(.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 80)
    }

    @ViewBuilder
    private func wikiHandoff(for topic: ThreadingTopic) -> some View {
        // Only render when a wiki page actually exists. The pill *looked*
        // tappable before but was a styled `Text` with a glass background —
        // tapping did nothing, and it always rendered even when the page
        // hadn't been compiled. Two paper cuts in one. Now: real Button,
        // visible only when there's somewhere to go.
        if let page = existingWikiPage {
            Divider().overlay(Color.primary.opacity(0.10))
            Button {
                Haptics.selection()
                presentedWikiPage = page
            } label: {
                Text("Open the wiki entry for \u{201C}\(topic.displayName)\u{201D} \u{2192}")
                    .font(AppTheme.Typography.caption)
                    .italic()
                    .foregroundStyle(AppTheme.Tint.editorialAmber)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(
                        RoundedRectangle(cornerRadius: 16, style: .continuous)
                            .fill(Color.clear)
                            .glassEffect(.regular.interactive(), in: .rect(cornerRadius: 16))
                    )
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Open the wiki entry for \(topic.displayName)")
        }
    }

    // MARK: - Actions

    private func play(mention: ThreadingMention) {
        Haptics.selection()
        guard let episode = store.episode(id: mention.episodeID) else { return }
        let startSeconds = TimeInterval(mention.startMS) / 1_000
        if playback.episode?.id == episode.id {
            playback.seek(to: startSeconds)
        } else {
            playback.setEpisode(episode)
            playback.seek(to: startSeconds)
        }
        playback.play()
    }

    // MARK: - Helpers

    private var topic: ThreadingTopic? {
        store.threadingTopic(id: topicID)
    }

    private func metadataLine(for topic: ThreadingTopic) -> String {
        let mentions = topic.episodeMentionCount
        var parts: [String] = ["\(mentions) episode\(mentions == 1 ? "" : "s")"]
        if topic.contradictionCount > 0 {
            let n = topic.contradictionCount
            parts.append("\(n) contradiction\(n == 1 ? "" : "s")")
        }
        return parts.joined(separator: " \u{00B7} ")
    }

    private func subscriptionTitle(for mention: ThreadingMention) -> String? {
        guard let episode = store.episode(id: mention.episodeID) else { return nil }
        return store.podcast(id: episode.podcastID)?.title
    }

    private var paperBackground: some View {
        Color(uiColor: UIColor { traits in
            traits.userInterfaceStyle == .dark
                ? UIColor(red: 0.055, green: 0.059, blue: 0.071, alpha: 1)
                : UIColor(red: 0.965, green: 0.949, blue: 0.914, alpha: 1)
        })
        .ignoresSafeArea()
    }
}

