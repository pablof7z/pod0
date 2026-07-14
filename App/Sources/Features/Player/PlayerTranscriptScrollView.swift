import SwiftUI

// USAGE:
// Internal-only renderer. No longer referenced from `PlayerView` — the player
// always shows chapters now. Kept for the clip composer / quote share /
// ask-agent surfaces that still operate on transcript segments inside their
// own sheets. Transcripts are an extraction substrate, not user-visible
// content; do not re-add this as a primary player surface.

// MARK: - PlayerTranscriptScrollView

/// Inline synced transcript inside the full-screen `PlayerView`.
///
/// Loads the persisted `Transcript` from `TranscriptStore` for the currently
/// loaded `PlaybackState.episode`. While audio plays we follow `currentTime`,
/// tinting the active segment and auto-scrolling it toward the centre of the
/// scroll surface (teleprompter-style, restyled for the dark player chrome).
/// Tap a segment to scrub to its start — and, if the player is paused, start
/// playback so the user doesn't need a follow-up tap.
///
/// The load task is keyed on `(episodeID, isReady)` looked up against
/// `AppStateStore`, so a `transcriptState` flip happening while the user is
/// already inside the player surface (e.g. an ingest started from
/// `EpisodeDetailView` resolves) re-fires the load and the transcript appears
/// without re-opening the player. Collapsing the live state to a `Bool` (vs
/// keying on the full enum) avoids re-firing on every Scribe progress tick.
///
/// When no transcript exists we fall back to a compact prompt that fires
/// `TranscriptIngestService.ingest` — provided either the publisher exposes a
/// transcript URL or the user has an ElevenLabs Scribe key configured.
struct PlayerTranscriptScrollView: View {

    @Bindable var state: PlaybackState
    /// Toggles between hero glass card and the bare reading surface used in
    /// transcript-focus layout. Parent supplies whichever framing is live.
    let useGlassCard: Bool

    // MARK: - Environment

    /// Live store lookup so a `transcriptState` flip happening in
    /// `TranscriptIngestService` (e.g. while the user is already inside the
    /// full-screen player) re-fires the load task — `PlaybackState.episode` is
    /// a stored copy and won't reflect the change on its own.
    @Environment(AppStateStore.self) private var store

    // MARK: - Local state

    @State private var transcript: Transcript?
    @State private var activeSegmentID: UUID?
    @State private var isRequestingIngest: Bool = false

    /// Hashable identity used to re-key the load task. Collapses
    /// `transcriptState` down to "is the persisted file readable yet" so we
    /// fire exactly twice across the warming flow (initial appearance + the
    /// `.transcribing → .ready` transition) instead of thrashing once per
    /// Scribe progress tick.
    private struct LoadKey: Hashable {
        let episodeID: UUID?
        let isReady: Bool
    }

    private var liveEpisode: Episode? {
        guard let episodeID = state.episode?.id else { return nil }
        return store.episode(id: episodeID) ?? state.episode
    }

    private var loadKey: LoadKey {
        let episodeID = state.episode?.id
        let isReady: Bool
        if let episodeID, case .ready = store.episode(id: episodeID)?.transcriptState {
            isReady = true
        } else {
            isReady = false
        }
        return LoadKey(episodeID: episodeID, isReady: isReady)
    }

    var body: some View {
        Group {
            if let transcript, !transcript.segments.isEmpty {
                synced(transcript: transcript)
            } else if let episode = liveEpisode {
                fallback(episode: episode)
            } else {
                empty
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(AppTheme.Spacing.lg)
        .background(transcriptBackground)
        .task(id: loadKey) { reloadTranscript() }
    }

    // MARK: - Synced surface

    @ViewBuilder
    private func synced(transcript: Transcript) -> some View {
        ScrollViewReader { proxy in
            ScrollView(showsIndicators: false) {
                LazyVStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
                    ForEach(transcript.segments) { seg in
                        PlayerTranscriptRow(
                            segment: seg,
                            speaker: transcript.speaker(for: seg.speakerID),
                            isActive: seg.id == activeSegmentID,
                            onTap: { tapSegment(seg) },
                            onAskAgent: { askAgent(about: seg) }
                        )
                        .id(seg.id)
                    }
                }
                .padding(.vertical, AppTheme.Spacing.sm)
            }
            .onAppear { syncActiveSegment(transcript: transcript, proxy: proxy, animated: false) }
            .onChange(of: state.currentTime) { _, _ in
                syncActiveSegment(transcript: transcript, proxy: proxy, animated: true)
            }
        }
    }

    /// Resolve the currently-active segment for `state.currentTime` and, when
    /// it changes, auto-scroll so the line sits in the centre of the small
    /// player viewport — `.center` reads more like a follow-along teleprompter
    /// than `.top` (which glues the active line to the lid). Skipping the
    /// animation on initial appearance avoids a jarring jump when the player
    /// opens to an already-mid-episode position.
    private func syncActiveSegment(
        transcript: Transcript,
        proxy: ScrollViewProxy,
        animated: Bool
    ) {
        guard let active = transcript.segment(at: state.currentTime) else { return }
        guard active.id != activeSegmentID else { return }
        activeSegmentID = active.id
        if animated {
            withAnimation(.easeOut(duration: 0.35)) {
                proxy.scrollTo(active.id, anchor: .center)
            }
        } else {
            proxy.scrollTo(active.id, anchor: .center)
        }
    }

    /// Long-press → "Ask the agent." Forwards to the stateless dispatcher so
    /// the View doesn't need to expose `store`. See
    /// `PlayerTranscriptScrollView+AskAgent.swift` for the full flow rationale.
    private func askAgent(about segment: Segment) {
        AskAgentDispatcher.dispatch(segment: segment, episode: liveEpisode, store: store)
    }

    /// Tap-to-seek that respects the user's pause intent. Tapping a line
    /// ALWAYS seeks; auto-resume is gated on `currentTime == 0`, i.e. the
    /// user just opened a fresh episode. A user who paused mid-playback
    /// to read ahead would otherwise lose their pause every time they
    /// scrubbed via a transcript line.
    private func tapSegment(_ segment: Segment) {
        let isFreshSession = state.currentTime <= 0.5
        state.seek(to: segment.start)
        if !state.isPlaying && isFreshSession {
            state.play()
        }
    }

    // MARK: - Fallback (no transcript on disk)

    @ViewBuilder
    private func fallback(episode: Episode) -> some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "text.quote")
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(.secondary)
            Text("No transcript yet")
                .font(AppTheme.Typography.headline)
                .foregroundStyle(.primary)
            Text(fallbackSubtitle(for: episode))
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, AppTheme.Spacing.lg)

            if isIngestActionable(for: episode) {
                Button { requestIngest(for: episode) } label: {
                    Text(buttonLabel(for: episode))
                        .font(.system(.subheadline, design: .rounded).weight(.semibold))
                        .foregroundStyle(.primary)
                        .padding(.horizontal, AppTheme.Spacing.lg)
                        .padding(.vertical, 10)
                        .glassEffect(.regular.interactive(), in: .capsule)
                }
                .buttonStyle(.plain)
                .disabled(!isIngestEnabled(for: episode))
                .padding(.top, AppTheme.Spacing.xs)
            }
        }
    }

    private var empty: some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "text.quote")
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(.secondary)
            Text("Pick something to play")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Background

    @ViewBuilder
    private var transcriptBackground: some View {
        if useGlassCard {
            RoundedRectangle(cornerRadius: 28, style: .continuous)
                .fill(.ultraThinMaterial.opacity(0.55))
                .overlay(
                    RoundedRectangle(cornerRadius: 28, style: .continuous)
                        .stroke(.white.opacity(0.10), lineWidth: 0.5)
                )
        } else {
            Color.clear
        }
    }

    // MARK: - Loading

    private func reloadTranscript() {
        guard let episodeID = state.episode?.id else {
            transcript = nil
            activeSegmentID = nil
            return
        }
        // Only attempt the disk read once the live episode reports `.ready`.
        // Reading earlier would just return `nil` (the persisted file isn't
        // written until `TranscriptIngestService.persistAndIndex`) and trash
        // any transcript we already had loaded for the same episode.
        guard let live = store.episode(id: episodeID) ?? state.episode,
              case .ready = live.transcriptState else {
            transcript = nil
            activeSegmentID = nil
            return
        }
        // Disk read is fast and runs on the main actor — no actor hop needed
        // and no Task.detached, per the file's Swift 6 concurrency note.
        let loaded = TranscriptStore.shared.load(episodeID: episodeID)
        transcript = loaded
        activeSegmentID = loaded?.segment(at: state.currentTime)?.id
    }

    // MARK: - Ingest gating

    private func isIngestActionable(for episode: Episode) -> Bool {
        if episode.publisherTranscriptURL != nil { return true }
        return hasKey(for: store.state.settings.sttProvider, episode: episode)
    }

    private func isIngestEnabled(for episode: Episode) -> Bool {
        guard !isRequestingIngest else { return false }
        switch episode.transcriptState {
        case .none, .failed: return true
        case .queued, .fetchingPublisher, .transcribing, .ready: return false
        }
    }

    private func buttonLabel(for episode: Episode) -> String {
        switch episode.transcriptState {
        case .queued: return "Queued…"
        case .fetchingPublisher: return "Fetching…"
        case .transcribing: return "Transcribing…"
        case .failed: return "Try again"
        case .ready, .none: return "Fetch transcript"
        }
    }

    private func fallbackSubtitle(for episode: Episode) -> String {
        switch episode.transcriptState {
        case .transcribing(let p):
            return "Transcribing — \(Int((p * 100).rounded()))% complete."
        case .fetchingPublisher:
            return "Pulling the publisher's transcript…"
        case .queued:
            return "Queued — we'll start shortly."
        case .failed(let message):
            return message
        case .ready, .none:
            if episode.publisherTranscriptURL != nil {
                return "We can pull the publisher's transcript for this episode."
            }
            let provider = store.state.settings.sttProvider
            if hasKey(for: provider, episode: episode) {
                return "We'll transcribe with \(provider.displayName) using your stored key."
            }
            if provider == .appleNative {
                return "Download the episode to use Apple on-device transcription, or wait for a publisher transcript."
            }
            return "Add a \(provider.displayName) key in Settings, or wait for a publisher transcript."
        }
    }

    private func hasKey(for provider: STTProvider, episode: Episode) -> Bool {
        switch provider {
        case .elevenLabsScribe: return ElevenLabsCredentialStore.hasAPIKey()
        case .assemblyAI: return AssemblyAICredentialStore.hasAPIKey()
        case .openRouterWhisper: return OpenRouterCredentialStore.hasAPIKey()
        case .appleNative: return EpisodeDownloadStore.shared.exists(for: episode)
        }
    }

    private func requestIngest(for episode: Episode) {
        guard isIngestEnabled(for: episode) else { return }
        isRequestingIngest = true
        let episodeID = episode.id
        // The View body runs on MainActor, so the inherited Task is already
        // MainActor — `ingest` just hops off via `await` and resumes us back
        // on it. No explicit MainActor.run hop required.
        Task { @MainActor in
            await TranscriptIngestService.shared.ingest(episodeID: episodeID)
            isRequestingIngest = false
            reloadTranscript()
        }
    }
}
