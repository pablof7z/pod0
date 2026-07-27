import SwiftUI

// MARK: - ClipTranscriptView

/// The clip, read in place.
///
/// Not a clip page — the reader's marked-up copy of the episode, opened at the
/// passage they kept. The clip sits at full presence, the conversation either
/// side recedes, everything else folds, and their *other* clips in this episode
/// never fold at all.
///
/// This is the app's first surface that renders transcript text for a human;
/// until now the transcript was an internal extraction layer feeding retrieval,
/// clipping and search.
struct ClipTranscriptView: View {

    let clipID: UUID

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback
    @Environment(\.dismiss) private var dismiss

    @State private var expandedFolds: Set<String> = []
    @State private var noteClipID: UUID?
    @State private var didAnchor = false

    var body: some View {
        Group {
            if let clip = store.clip(id: clipID) {
                reader(for: clip)
            } else {
                ContentUnavailableView(
                    "This clip is gone",
                    systemImage: "scissors",
                    description: Text("It was deleted from another device.")
                )
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { principalTitle }
        .sheet(item: Binding(
            get: { noteClipID.flatMap { store.clip(id: $0) }.map(IdentifiedClip.init) },
            set: { noteClipID = $0?.clip.id }
        )) { wrapper in
            ClipNoteSheet(
                clip: wrapper.clip,
                notes: notes(inside: wrapper.clip),
                episode: store.episode(id: wrapper.clip.episodeID),
                podcast: store.podcast(id: wrapper.clip.subscriptionID),
                onPlay: { play(wrapper.clip) }
            )
        }
    }

    // MARK: - Reader

    @ViewBuilder
    private func reader(for clip: Clip) -> some View {
        let rows = rows(for: clip)
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
                    ForEach(rows) { row in
                        switch row {
                        case .fold(let id, let weight):
                            ClipTranscriptFoldView(weight: weight) {
                                withAnimation(AppTheme.Animation.spring) {
                                    _ = expandedFolds.insert(id)
                                }
                            }
                            .id(id)
                        case .turn(let turn):
                            ClipTranscriptTurnView(turn: turn) { tapped in
                                noteClipID = tapped
                            }
                            .id(turn.id)
                        }
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.vertical, AppTheme.Spacing.xl)
            }
            .safeAreaInset(edge: .bottom) { playBar(clip) }
            .onAppear { anchor(rows: rows, using: proxy) }
        }
    }

    /// Opens on the clip rather than at the top of the episode — the passage
    /// you kept is why the screen exists.
    private func anchor(rows: [ClipTranscriptRow], using proxy: ScrollViewProxy) {
        guard !didAnchor else { return }
        didAnchor = true
        guard let target = rows.first(where: {
            if case .turn(let turn) = $0 { return turn.presence == .focus }
            return false
        })?.id else { return }
        DispatchQueue.main.async {
            proxy.scrollTo(target, anchor: .center)
        }
    }

    // MARK: - Rows

    private func rows(for clip: Clip) -> [ClipTranscriptRow] {
        let clips = store.clips(forEpisode: clip.episodeID)
        let annotated = Set(clips.filter { !notes(inside: $0).isEmpty }.map(\.id))

        guard let transcript = store.transcriptReader.load(episodeID: clip.episodeID),
              !transcript.segments.isEmpty
        else {
            return [frozenFallback(clip, annotated: annotated)]
        }

        return ClipTranscriptComposer.rows(
            transcript: transcript,
            clips: clips,
            focusedClipID: clip.id,
            annotatedClipIDs: annotated,
            expandedFolds: expandedFolds
        )
    }

    /// Episodes without an ingested transcript still have the prose frozen into
    /// the clip at capture. One turn, no context, no folds — the clip is all
    /// there is to show, and showing nothing would be worse.
    private func frozenFallback(_ clip: Clip, annotated: Set<UUID>) -> ClipTranscriptRow {
        .turn(ClipTranscriptTurn(
            id: "frozen-\(clip.id.uuidString)",
            presence: .focus,
            speakerName: nil,
            text: clip.transcriptText.isEmpty
                ? "This clip was captured without a transcript."
                : clip.transcriptText,
            start: clip.startSeconds,
            end: clip.endSeconds,
            clipID: clip.id,
            isAnnotated: annotated.contains(clip.id)
        ))
    }

    // MARK: - Notes

    /// The margin for this clip.
    ///
    /// `notes(forClip:)` returns newest first; the sheet stacks oldest first so
    /// returning to a clip months later reads as strata rather than a feed.
    private func notes(inside clip: Clip) -> [Note] {
        store.notes(forClip: clip.id).reversed()
    }

    // MARK: - Chrome

    @ToolbarContentBuilder
    private var principalTitle: some ToolbarContent {
        ToolbarItem(placement: .principal) {
            if let clip = store.clip(id: clipID),
               let episode = store.episode(id: clip.episodeID) {
                VStack(spacing: 0) {
                    Text(episode.title)
                        .font(.system(.subheadline).weight(.semibold))
                        .lineLimit(1)
                    if let show = store.podcast(id: clip.subscriptionID)?.title {
                        Text(show)
                            .font(.system(.caption2, design: .monospaced))
                            .tracking(0.8)
                            .textCase(.uppercase)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
    }

    /// The one affordance on the screen that is not text. Playing is a separate
    /// act — tapping a passage opens its note, it never seeks.
    private func playBar(_ clip: Clip) -> some View {
        GlassEffectContainer(spacing: 12) {
            HStack(spacing: AppTheme.Spacing.md) {
                Button {
                    Haptics.selection()
                    play(clip)
                } label: {
                    Image(systemName: "play.fill")
                        .font(.system(size: 15))
                        .frame(width: 32, height: 32)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(.primary.opacity(0.75))
                .accessibilityLabel("Play in player")

                Spacer(minLength: 0)

                Text(Self.timecode(clip.startSeconds))
                    .font(.system(.caption2, design: .monospaced))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, AppTheme.Spacing.lg)
            .padding(.vertical, AppTheme.Spacing.sm)
        }
        .background(.bar)
    }

    // MARK: - Actions

    private func play(_ clip: Clip) {
        guard let episode = store.episode(id: clip.episodeID) else { return }
        playback.setEpisode(episode)
        playback.seek(to: clip.startSeconds)
        playback.play()
        NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
    }

    private static func timecode(_ seconds: TimeInterval) -> String {
        let total = max(0, Int(seconds))
        let hours = total / 3_600
        let minutes = (total % 3_600) / 60
        let secs = total % 60
        return hours > 0
            ? String(format: "%d:%02d:%02d", hours, minutes, secs)
            : String(format: "%d:%02d", minutes, secs)
    }
}

// MARK: - Sheet identity

/// `sheet(item:)` needs an `Identifiable` payload; `Clip` is identifiable but
/// the binding round-trips through the id so the sheet always reads live state.
private struct IdentifiedClip: Identifiable {
    let clip: Clip
    var id: UUID { clip.id }
}
