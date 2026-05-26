import SwiftUI

// MARK: - ClipComposerView
//
// Sheet for creating a user-defined clip out of the active episode. Owned
// by the kernel (D7 — Rust decides what a clip *is*; we only emit
// `podcast.clip.create` / `auto_snip` and re-render from the snapshot).
//
// Layout:
//   - Episode header (artwork + episode title + podcast name)
//   - Title text field
//   - Range sliders (two `Slider` controls keyed by min/max of the other)
//   - Preview play / stop button (seeks + plays, stops via timer at end)
//   - "Auto Snip" button (dispatches `podcast.clip.auto_snip` with current pos)
//   - "Save Clip" button (dispatches `podcast.clip.create`)
//
// The preview timer is the only piece of local state with side effects:
// when the user releases the preview, or the sheet dismisses, we pause
// playback. Everything else is fire-and-forget.

struct ClipComposerView: View {
    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    /// PlayerState snapshot captured at sheet-presentation time. The
    /// composer reads duration / position from this; once dismissed the
    /// snapshot tick will catch up.
    let player: PlayerState
    /// Episode resolved by the parent (so the composer doesn't have to
    /// re-scan the library on every render).
    let episode: EpisodeSummary?
    let podcastTitle: String?

    @State private var startSecs: Double
    @State private var endSecs: Double
    @State private var clipTitle: String = ""
    @State private var isPreviewing: Bool = false
    @State private var previewTask: Task<Void, Never>?

    init(player: PlayerState, episode: EpisodeSummary?, podcastTitle: String?) {
        self.player = player
        self.episode = episode
        self.podcastTitle = podcastTitle
        let dur = player.durationSecs ?? episode?.durationSecs ?? 0
        let pos = player.positionSecs
        let initialStart = max(0, pos - 15)
        let initialEnd = max(initialStart + 1, min(dur > 0 ? dur : pos + 45, pos + 30))
        _startSecs = State(initialValue: initialStart)
        _endSecs = State(initialValue: initialEnd)
    }

    private var duration: Double {
        let d = player.durationSecs ?? episode?.durationSecs ?? 0
        return d > 0 ? d : max(endSecs + 30, 60)
    }

    private var clipLength: Double { max(0, endSecs - startSecs) }
    private var episodeId: String? { player.episodeId ?? episode?.id }
    private var canSave: Bool { episodeId != nil && clipLength >= 1 }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: PodcastSpace.l) {
                    header
                    titleField
                    rangeSection
                    previewButton
                    actionButtons
                }
                .padding(PodcastSpace.l)
            }
            .navigationTitle("New Clip")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Cancel") {
                        stopPreview()
                        dismiss()
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink {
                        ClipsListView()
                            .environment(model)
                    } label: {
                        Image(systemName: "list.bullet")
                            .accessibilityLabel("All clips")
                    }
                }
            }
            .onDisappear { stopPreview() }
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: PodcastSpace.m) {
            artwork
                .frame(width: 56, height: 56)
                .clipShape(RoundedRectangle(cornerRadius: PodcastSpace.radiusSmall, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(episode?.title ?? "Now Playing")
                    .font(PodcastFont.headline)
                    .lineLimit(2)
                if let podcastTitle = podcastTitle ?? episode?.podcastTitle {
                    Text(podcastTitle)
                        .font(PodcastFont.caption)
                        .foregroundStyle(PodcastColor.textSecondary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 0)
        }
    }

    @ViewBuilder
    private var artwork: some View {
        if let str = episode?.artworkUrl, let url = URL(string: str) {
            AsyncImage(url: url) { image in
                image.resizable().scaledToFill()
            } placeholder: {
                artworkPlaceholder
            }
        } else {
            artworkPlaceholder
        }
    }

    private var artworkPlaceholder: some View {
        RoundedRectangle(cornerRadius: PodcastSpace.radiusSmall, style: .continuous)
            .fill(PodcastColor.surface)
            .overlay {
                Image(systemName: "scissors")
                    .foregroundStyle(PodcastColor.textTertiary)
                    .font(.system(size: 18))
            }
    }

    // MARK: - Title field

    private var titleField: some View {
        VStack(alignment: .leading, spacing: PodcastSpace.xs) {
            Text("Title").font(PodcastFont.caption).foregroundStyle(PodcastColor.textSecondary)
            TextField("Optional clip title", text: $clipTitle)
                .textFieldStyle(.roundedBorder)
        }
    }

    // MARK: - Range sliders

    private var rangeSection: some View {
        VStack(alignment: .leading, spacing: PodcastSpace.s) {
            HStack {
                Text("Range").font(PodcastFont.caption).foregroundStyle(PodcastColor.textSecondary)
                Spacer()
                Text("\(formatDuration(startSecs)) – \(formatDuration(endSecs)) (\(formatDuration(clipLength)))")
                    .font(PodcastFont.caption.monospacedDigit())
                    .foregroundStyle(PodcastColor.textSecondary)
            }
            VStack(alignment: .leading, spacing: PodcastSpace.xs) {
                Text("Start").font(PodcastFont.caption)
                Slider(value: Binding(
                    get: { startSecs },
                    set: { newValue in
                        let upper = max(0, endSecs - 1)
                        startSecs = min(max(0, newValue), upper)
                    }
                ), in: 0...max(1, duration))
                Text("End").font(PodcastFont.caption)
                Slider(value: Binding(
                    get: { endSecs },
                    set: { newValue in
                        let lower = startSecs + 1
                        endSecs = max(lower, min(duration, newValue))
                    }
                ), in: 0...max(1, duration))
            }
        }
        .padding(PodcastSpace.m)
        .background(PodcastColor.surface, in: RoundedRectangle(cornerRadius: PodcastSpace.radiusSmall, style: .continuous))
    }

    // MARK: - Preview

    private var previewButton: some View {
        Button {
            if isPreviewing { stopPreview() } else { startPreview() }
        } label: {
            HStack(spacing: PodcastSpace.s) {
                Image(systemName: isPreviewing ? "stop.fill" : "play.fill")
                Text(isPreviewing ? "Stop Preview" : "Preview Clip")
                    .font(PodcastFont.callout.weight(.semibold))
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, PodcastSpace.m)
            .background(PodcastColor.accentSoft, in: RoundedRectangle(cornerRadius: PodcastSpace.radiusSmall, style: .continuous))
            .foregroundStyle(PodcastColor.accent)
        }
        .buttonStyle(.plain)
        .disabled(episodeId == nil || clipLength < 1)
    }

    private func startPreview() {
        guard let epId = episodeId else { return }
        previewTask?.cancel()
        model.dispatch(namespace: "podcast.player", body: [
            "op": "seek",
            "position_secs": startSecs,
        ])
        model.dispatch(namespace: "podcast.player", body: [
            "op": "play",
            "episode_id": epId,
        ])
        isPreviewing = true
        let duration = clipLength
        previewTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(duration))
            guard !Task.isCancelled else { return }
            stopPreview()
        }
    }

    private func stopPreview() {
        previewTask?.cancel()
        previewTask = nil
        if isPreviewing {
            model.dispatch(namespace: "podcast.player", body: ["op": "pause"])
        }
        isPreviewing = false
    }

    // MARK: - Actions

    private var actionButtons: some View {
        VStack(spacing: PodcastSpace.s) {
            Button(action: autoSnip) {
                actionLabel("Auto Snip", systemImage: "sparkles", filled: false)
            }
            .buttonStyle(.plain)
            .disabled(episodeId == nil)
            Button(action: save) {
                actionLabel("Save Clip", systemImage: "checkmark", filled: true)
            }
            .buttonStyle(.plain)
            .disabled(!canSave)
        }
    }

    @ViewBuilder
    private func actionLabel(_ text: String, systemImage: String, filled: Bool) -> some View {
        HStack(spacing: PodcastSpace.s) {
            Image(systemName: systemImage)
            Text(text).font(PodcastFont.callout.weight(.semibold))
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, PodcastSpace.m)
        .background(
            filled ? AnyShapeStyle(PodcastColor.accent) : AnyShapeStyle(PodcastColor.surface),
            in: RoundedRectangle(cornerRadius: PodcastSpace.radiusSmall, style: .continuous)
        )
        .foregroundStyle(filled ? Color.white : PodcastColor.textPrimary)
    }

    private func autoSnip() {
        guard let epId = episodeId else { return }
        let pos = player.positionSecs
        model.dispatch(namespace: "podcast.clip", body: [
            "op": "auto_snip",
            "episode_id": epId,
            "position_secs": pos,
        ])
        stopPreview()
        dismiss()
    }

    private func save() {
        guard let epId = episodeId else { return }
        var body: [String: Any] = [
            "op": "create",
            "episode_id": epId,
            "start_secs": startSecs,
            "end_secs": endSecs,
        ]
        let trimmed = clipTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
            body["title"] = trimmed
        }
        model.dispatch(namespace: "podcast.clip", body: body)
        stopPreview()
        dismiss()
    }

}
