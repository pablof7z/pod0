import SwiftUI

/// Persistent mini-player presented as a `tabViewBottomAccessory` (iOS 26).
///
/// Reads `\.tabViewBottomAccessoryPlacement` from the environment and
/// renders one of two layouts:
///   - `.expanded` — full mini-bar above the tab bar with the episode title.
///   - `.inline`   — compact pill that slots between the active-tab capsule
///     and the trailing toolbar controls when the tab bar collapses on
///     scroll-down (Apple Music pattern).
///
/// The expanded UI is two glass bodies inside one `GlassEffectContainer`: a
/// metadata card (artwork, episode title, show name + clock) and a circular
/// play/pause orb beside it. The inline pill drops to artwork + play/pause
/// only, unsplit, since the tab bar's own glass shell hosts it.
struct MiniPlayerView: View {

    @Environment(AppStateStore.self) private var store
    @Bindable var state: PlaybackState
    let onTap: () -> Void
    let glassNamespace: Namespace.ID

    @Environment(\.tabViewBottomAccessoryPlacement) private var placement

    private var showName: String {
        guard let subID = state.episode?.podcastID,
              let sub = store.podcast(id: subID) else { return "" }
        return sub.title
    }

    /// Title of the chapter containing the playhead, when the live episode
    /// has navigable chapters. Returns `nil` for chapter-less episodes so
    /// the metadata line falls back to the show name. Reads from
    /// `AppStateStore` rather than the cached `state.episode` so chapters
    /// selected by durable artifact reconciliation after playback started
    /// show up here without a re-load.
    private var activeChapterTitle: String? {
        guard let stateEpisode = state.episode else { return nil }
        let live = store.episode(id: stateEpisode.id) ?? stateEpisode
        let navigable = live.chapters?.filter(\.includeInTableOfContents) ?? []
        guard !navigable.isEmpty else { return nil }
        return navigable.active(at: state.currentTime)?.title
    }

    var body: some View {
        Group {
            switch placement {
            case .inline:
                inlineBody
            default:
                expandedBody
            }
        }
        .animation(AppTheme.Animation.spring, value: placement)
    }

    // MARK: - Expanded (regular) layout

    /// One bar, and no glass of our own.
    ///
    /// `tabViewBottomAccessory` already draws a Liquid Glass capsule around
    /// whatever it hosts, and there is no API to opt out of it — there is no
    /// `ContainerBackgroundPlacement` for it. This view used to paint a second
    /// `glassEffect` on top of that shell, which stacked two blurs: each one
    /// lightens what's behind it, so over light content the pair went nearly
    /// opaque and lost the translucency and specular edge that make the
    /// material read as glass at all.
    ///
    /// So the content sits directly on the accessory's own glass. Nothing
    /// inside carries a background either — a glass control on a glass shell
    /// just reads as a button bolted onto a toolbar.
    ///
    /// Wrapping this in `Button(action: onTap)` would collapse the nested
    /// play/pause Button into the parent's tap target, so tapping the visible
    /// pause icon would *expand* the player instead of pausing. Use a
    /// non-Button tap surface so the transport Button keeps its own gesture.
    private var expandedBody: some View {
        content
            .contentShape(.rect)
            .onTapGesture {
                Haptics.light()
                onTap()
            }
            .accessibilityElement(children: .contain)
            .accessibilityAction(named: "Open player") {
                onTap()
            }
    }

    // MARK: - Inline (compact) layout

    /// The collapsed pill that sits inline with the tab bar. No surrounding
    /// glass surface — the toolbar's own glass shell hosts it.
    ///
    /// Same Button-inside-Button trap as `expandedBody`: the play/pause icon
    /// has to remain an independent, separately-tappable Button. Use a non-Button
    /// tap surface for the expand-on-tap action.
    ///
    /// Title is included alongside the artwork — without it, the pill reads
    /// as a generic glass slab and the underlying scroll content shows
    /// through the translucent background, making it look broken. Apple
    /// Music's pill omits the title because their artwork conveys identity
    /// strongly; podcast covers don't, so we need text.
    private var inlineBody: some View {
        HStack(spacing: AppTheme.Spacing.xs) {
            // Combine artwork + title + clock into a single tap-to-expand
            // surface. Each child is `accessibilityHidden` so VoiceOver
            // hears one labeled "Now Playing" element, not three. The
            // tap-to-expand was previously unreachable for VO users.
            HStack(spacing: AppTheme.Spacing.xs) {
                inlineArtwork
                    .glassEffectID("player.artwork", in: glassNamespace)
                    .accessibilityHidden(true)

                VStack(alignment: .leading, spacing: 0) {
                    inlineTitle
                    inlineClock
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityHidden(true)
            }
            .contentShape(Rectangle())
            .onTapGesture {
                Haptics.light()
                onTap()
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(accessibilityLabel)
            .accessibilityHint("Opens the full player")
            .accessibilityAddTraits(.isButton)

            inlineDownloadBadge

            // Real Button kept as a sibling so its 44pt hit area never
            // gets eaten by the expand-tap surface. `.frame(28)` keeps the
            // visible glyph compact; the outer `.frame(44)` + .contentShape
            // expands the actual tap target to Apple's HIG minimum.
            Button {
                state.togglePlayPause()
            } label: {
                Image(systemName: state.isPlaying ? "pause.fill" : "play.fill")
                    .font(.subheadline.weight(.bold))
                    .foregroundStyle(.primary)
                    .frame(width: 28, height: 28)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.pressable)
            .accessibilityLabel(state.isPlaying ? "Pause" : "Play")
        }
        .padding(.horizontal, AppTheme.Spacing.sm)
    }

    /// Re-resolves `state.episode` through the store so coarse download
    /// transitions (`.downloading → .downloaded`) reach the badge without
    /// requiring `PlaybackState` to refresh its cached snapshot.
    private var liveDownloadEpisode: Episode? {
        guard let id = state.episode?.id else { return nil }
        return store.episode(id: id) ?? state.episode
    }

    @ViewBuilder
    private var inlineTitle: some View {
        if let episode = state.episode {
            Text(episode.title)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }

    /// Compact mono-digit playhead surfaced inline so the collapsed pill
    /// keeps a glanceable cue without the full metadata line. Hidden when
    /// no episode is loaded (the artwork+title row already conveys "no
    /// playback") to avoid an empty 0:00 leaking into the layout.
    @ViewBuilder
    private var inlineClock: some View {
        if state.episode != nil {
            Text(PlayerTimeFormat.clock(state.currentTime))
                .font(.system(size: 11, weight: .regular, design: .monospaced))
                .foregroundStyle(.secondary)
                .monospacedDigit()
                .lineLimit(1)
        }
    }

    /// Inline-only download surface — narrow visibility rule per spec:
    /// only render when the live episode is actively downloading or has
    /// failed. The collapsed pill has no horizontal slack for `.queued`
    /// or terminal `.downloaded` states, and they'd add visual noise to
    /// the tab bar without informing an in-flight action.
    @ViewBuilder
    private var inlineDownloadBadge: some View {
        if let resolved = liveDownloadEpisode {
            if store.sharedLibrary?.downloadProgress(episodeID: resolved.id) != nil {
                DownloadProgressBadge(
                    episode: resolved,
                    liveProgress: store.sharedLibrary?.downloadProgress(episodeID: resolved.id)
                )
            }
        }
    }

    private var inlineArtwork: some View {
        artworkSurface(
            size: 26,
            cornerRadius: AppTheme.Corner.sm,
            placeholderGlyphSize: 10
        )
    }

    // MARK: - Subviews

    private var content: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            artwork
                .glassEffectID("player.artwork", in: glassNamespace)

            // Three lines inside a 48pt capsule leaves no room for gaps —
            // spacing 0 and the tight caption sizes are what make them fit.
            VStack(alignment: .leading, spacing: 0) {
                titleLine
                metadataLine
                showLine
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .animation(AppTheme.Animation.spring, value: activeChapterTitle)

            playPauseButton
        }
        // Fill whatever vertical slot the accessory hands us, then centre in it.
        // Sizing to the content instead left the artwork 3pt from the capsule's
        // top edge and 15pt from the bottom — visibly off-centre and touching.
        .frame(maxHeight: .infinity)
        // Vertical padding cannot make this bar taller. `tabViewBottomAccessory`
        // draws its glass capsule at a height it owns — measured at ~60pt on a
        // 17 Pro Max — and ignores the content's own height: padding this out to
        // 24pt grew the content box to 92pt while the drawn capsule stayed 60pt,
        // so the only visible effect was tap targets spilling outside the glass.
        // Breathing room has to come from smaller content, not more padding.
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.vertical, AppTheme.Spacing.sm)
    }

    /// Sized against the accessory's fixed 48pt capsule, not chosen freely.
    /// Inside that height the trade is zero-sum, measured on a 17 Pro Max:
    /// 42pt artwork leaves 3pt above and below and reads as wedged in; 34pt
    /// leaves 7pt but the cover reads as undersized. 38pt splits it at 5pt.
    /// Overcast gets both because its pill is ~58pt — height the accessory
    /// does not let us have.
    private var artwork: some View {
        artworkSurface(
            size: 38,
            cornerRadius: AppTheme.Corner.md,
            placeholderGlyphSize: 15
        )
    }

    /// Resolved artwork URL — episode override first, then the show-level
    /// fallback via `PlaybackState.resolveShowImage` (the same closure the
    /// full Player uses, wired in `RootView`).
    private var artworkURL: URL? {
        guard let episode = state.episode else { return nil }
        return episode.imageURL ?? state.resolveShowImage(episode)
    }

    /// Shared artwork rendering for both the expanded (44pt) and inline
    /// (26pt) layouts. Loading state is glyph-free so the user doesn't read
    /// it as "no artwork"; failure state shows a subtle waveform indicator.
    @ViewBuilder
    private func artworkSurface(
        size: CGFloat,
        cornerRadius: CGFloat,
        placeholderGlyphSize: CGFloat
    ) -> some View {
        ZStack {
            if let url = artworkURL {
                CachedAsyncImage(url: url, targetSize: CGSize(width: 64, height: 64)) { phase in
                    switch phase {
                    case .success(let image):
                        image
                            .resizable()
                            .scaledToFill()
                    case .failure:
                        miniArtworkFailureFallback(glyphSize: placeholderGlyphSize)
                    case .empty:
                        Color.secondary.opacity(0.18)
                    @unknown default:
                        Color.secondary.opacity(0.18)
                    }
                }
            } else {
                miniArtworkFailureFallback(glyphSize: placeholderGlyphSize)
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    }

    private func miniArtworkFailureFallback(glyphSize: CGFloat) -> some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: glyphSize, weight: .semibold))
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var titleLine: some View {
        if let episode = state.episode {
            Text(episode.title)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }

    /// Chapter line — bare text. The clock and the `list.bullet.rectangle`
    /// glyph that used to flank it are gone: the glyph had no tap behaviour
    /// and read as an unexplained blue mark, and the playhead is already one
    /// tap away in the full player.
    @ViewBuilder
    private var metadataLine: some View {
        if state.episode != nil, let chapterTitle = activeChapterTitle {
            Text(chapterTitle)
                .font(AppTheme.Typography.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.tail)
                .transition(.opacity)
                .id(chapterTitle)
        }
    }

    /// Show name, muted and semibold. Third line, so it needs to be the
    /// quietest thing that still reads as a distinct field — weight carries it
    /// rather than size, since there is no vertical room to spend.
    @ViewBuilder
    private var showLine: some View {
        if !showName.isEmpty {
            Text(showName)
                .font(AppTheme.Typography.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.tail)
        }
    }

    /// Sole transport control in the expanded bar. Skip-forward and dismiss
    /// were removed so play/pause reads as the one obvious target: the full
    /// player (a tap away on the bar itself) already owns skipping, and
    /// dismissal is a destructive action that doesn't belong one stray thumb
    /// away from the tab bar.
    ///
    /// Bare glyph, no background of its own — the accessory's glass is the
    /// only surface here, and a second one around the glyph turns the bar into
    /// a toolbar with a button screwed to it.
    private var playPauseButton: some View {
        Button {
            state.togglePlayPause()
        } label: {
            Image(systemName: state.isPlaying ? "pause.fill" : "play.fill")
                .font(.title.weight(.bold))
                .foregroundStyle(.primary)
                .frame(width: 44, height: 44)
                .contentShape(.rect)
                .glassEffectID("player.play", in: glassNamespace)
        }
        .buttonStyle(.pressable)
        .accessibilityLabel(state.isPlaying ? "Pause" : "Play")
    }

    private var accessibilityLabel: String {
        let title = state.episode?.title ?? "Now playing"
        var parts: [String] = [title]
        // Both, not either/or — the expanded bar now shows the chapter and the
        // show name on separate lines, so VoiceOver should hear what is there.
        if let chapter = activeChapterTitle {
            parts.append("Chapter: \(chapter)")
        }
        if !showName.isEmpty {
            parts.append(showName)
        }
        return parts.joined(separator: ", ")
    }
}
