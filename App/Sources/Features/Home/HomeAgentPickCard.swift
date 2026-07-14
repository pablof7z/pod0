import SwiftUI

// MARK: - HomeAgentPickCard

/// Featured-section "agent pick" card species. Visually distinct from the
/// `HomeResumeCard`: thinner aspect ratio, leading 2-pt accent bar, and a
/// one-line *because* rationale beneath the title. Tapping the chevron
/// expands to show the full rationale text.
struct HomeAgentPickCard: View {
    let pick: HomeAgentPick
    let episode: Episode
    let podcast: Podcast?
    let isHero: Bool
    let onPlay: () -> Void

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback
    @State private var narrator = RationaleNarrator.shared
    @State private var isExpanded: Bool = false
    @State private var pulse: Bool = false

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            accentBar
            content
                .padding(AppTheme.Spacing.sm)
        }
        .frame(width: cardWidth, alignment: .leading)
        .background(
            Color(.secondarySystemBackground),
            in: RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
        )
        .contentShape(RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous))
        .onTapGesture(perform: onPlay)
        .contextMenu {
            EpisodeRowContextMenu(
                episode: episode,
                store: store,
                openDetailsRoute: HomeEpisodeRoute(episodeID: episode.id)
            )
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityHint("Plays this episode")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction(named: Text("Play")) {
            onPlay()
        }
    }

    // MARK: - Layout

    private var cardWidth: CGFloat {
        isHero ? 320 : 240
    }

    private var accentBar: some View {
        // The 2-pt accent bar is the visual signature that distinguishes
        // an agent pick from a resume card. Hero gets a richer indigo
        // tone; secondaries fade slightly so the hero reads as primary.
        Rectangle()
            .fill(AppTheme.Tint.agentSurface.opacity(isHero ? 0.95 : 0.65))
            .frame(width: 2)
    }

    @ViewBuilder
    private var content: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
                artwork
                VStack(alignment: .leading, spacing: 2) {
                    if let showName = podcast?.title, !showName.isEmpty {
                        Text(showName)
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Text(episode.title)
                        .font(isHero ? AppTheme.Typography.title3 : AppTheme.Typography.headline)
                        .foregroundStyle(.primary)
                        .lineLimit(isHero ? 3 : 2)
                        .multilineTextAlignment(.leading)
                }
            }
            rationaleSection
        }
    }

    @ViewBuilder
    private var artwork: some View {
        let url = episode.imageURL ?? podcast?.imageURL
        let dim: CGFloat = isHero ? 64 : 44
        ZStack {
            Color.secondary.opacity(0.18)
            if let url {
                CachedAsyncImage(
                    url: url,
                    targetSize: CGSize(width: 128, height: 128)
                ) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default:
                        Image(systemName: "waveform")
                            .font(.system(size: dim * 0.45, weight: .light))
                            .foregroundStyle(.secondary)
                    }
                }
            } else {
                Image(systemName: "waveform")
                    .font(.system(size: dim * 0.45, weight: .light))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: dim, height: dim)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
    }

    @ViewBuilder
    private var rationaleSection: some View {
        if !pick.rationale.isEmpty {
            HStack(alignment: .top, spacing: AppTheme.Spacing.xs) {
                rationaleText
                Spacer(minLength: 0)
                speakerButton
                expandButton
            }
        }
    }

    /// Tap glyph → narrate via TTS (ElevenLabs if connected, else AVSpeech).
    /// Tap again while narrating → stop. The narrator pauses the active
    /// podcast for the duration of the narration and resumes after.
    private var speakerButton: some View {
        let isNarratingThis = narrator.narratingPickID == pick.episodeID
        return Button {
            Haptics.light()
            let text = pick.spokenRationale.isEmpty ? pick.rationale : pick.spokenRationale
            narrator.attach(playback: playback)
            Task {
                await narrator.speak(
                    pickID: pick.episodeID,
                    text: text,
                    voiceID: store.state.settings.elevenLabsVoiceID,
                    ttsModel: store.state.settings.elevenLabsTTSModel
                )
            }
        } label: {
            Image(systemName: isNarratingThis ? "speaker.wave.2.fill" : "speaker.wave.2")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(isNarratingThis ? AppTheme.Tint.agentSurface : .secondary)
                .padding(4)
                .scaleEffect(isNarratingThis && pulse ? 1.18 : 1.0)
                .animation(
                    isNarratingThis
                        ? .easeInOut(duration: 0.8).repeatForever(autoreverses: true)
                        : .default,
                    value: pulse
                )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(isNarratingThis ? "Stop reading rationale" : "Read rationale aloud")
        .onChange(of: narrator.narratingPickID == pick.episodeID) { _, narrating in
            pulse = narrating
        }
    }

    private var rationaleText: some View {
        Text(rationaleDisplay)
            .font(AppTheme.Typography.subheadline)
            .italic()
            .foregroundStyle(.secondary)
            .lineLimit(isExpanded ? nil : (isHero ? 2 : 1))
            .multilineTextAlignment(.leading)
            .fixedSize(horizontal: false, vertical: isExpanded)
    }

    /// "Because <rationale>" framing. The brief asks for the *because*
    /// preamble so the picks read as editorial recommendations, not as
    /// neutral metadata. We elide the preamble when the model already
    /// began its reason with "because" so we don't render "Because Because".
    private var rationaleDisplay: String {
        let trimmed = pick.rationale.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.lowercased().hasPrefix("because") {
            return trimmed
        }
        return "Because \(trimmed.first.map { String($0).lowercased() + trimmed.dropFirst() } ?? "")"
    }

    private var expandButton: some View {
        Button {
            Haptics.light()
            withAnimation(AppTheme.Animation.springFast) {
                isExpanded.toggle()
            }
        } label: {
            Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(4)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(isExpanded ? "Collapse rationale" : "Expand rationale")
    }

    private var accessibilityLabel: String {
        var parts: [String] = []
        if let s = podcast?.title, !s.isEmpty { parts.append(s) }
        parts.append(episode.title)
        if !pick.rationale.isEmpty { parts.append(pick.rationale) }
        parts.append(isHero ? "Hero pick" : "Pick")
        return parts.joined(separator: ", ")
    }
}
