import AVFoundation
import Speech
import SwiftUI

// MARK: - BriefingPlayerView

/// W2 — the briefing player surface. Distinctive chrome per UX-08 §4: warm
/// brass-amber glass, editorial serif title, segment rail with glassEffectID
/// morphing pills, *deeper into this* / *skip* per-segment actions, and a
/// live transcript pane.
///
/// The view binds to a `BriefingPlayerEngine` instance for transport state.
/// In dev / preview builds the engine is backed by `FakeBriefingPlayerHost`;
/// production wiring will hand in Lane 1's `AudioEngine`.
struct BriefingPlayerView: View {

    // MARK: Inputs

    let context: BriefingPlaybackContext
    /// When `true`, `prepareEngine()` calls `engine.play(stitchedURL:)`
    /// immediately after `engine.load(...)` so the surface starts speaking
    /// without a user tap. Default `false` preserves the existing
    /// tap-one-to-play library entry behaviour. The lean-back river
    /// (`BriefingRiverView`) passes `true`. A missing asset URL surfaces an
    /// inline error and returns before reaching the engine, so no silent
    /// playback of a placeholder can occur.
    var autoplay: Bool = false

    // MARK: State

    @State private var engine = BriefingPlayerEngine()
    @State private var mic = BriefingMicCaptureController()
    @State private var promptDraft: String = ""
    @State private var isShowingBranchPrompt = false
    /// Whether the user is currently holding the mic glyph. Drives chrome
    /// (the *listening* glow on the transcript pane — UX-08 §4) and the
    /// `engine.beginBranch` / `endBranch` lifecycle.
    @State private var isHoldingMic = false
    @State private var assetMissingMessage: String?

    // MARK: Body

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                if let msg = assetMissingMessage {
                    HStack(spacing: AppTheme.Spacing.sm) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                        Text(msg)
                            .font(AppTheme.Typography.body)
                            .foregroundStyle(.primary)
                        Spacer()
                    }
                    .padding(AppTheme.Spacing.md)
                    .glassSurface(
                        cornerRadius: AppTheme.Corner.md,
                        tint: Color.orange.opacity(0.15)
                    )
                }
                editorialHeader
                transcriptPane
                transportControls
                actionsRow
                segmentRail
                attributionStrip
            }
            .padding()
        }
        .background(background)
        .navigationBarTitleDisplayMode(.inline)
        // No toolbar items yet — the previous "share-card" placeholder
        // was dead, see `toolbar` for context.
        .sheet(isPresented: $isShowingBranchPrompt) {
            BriefingBranchPromptSheet(
                promptDraft: $promptDraft,
                onSubmit: {
                    Task {
                        await engine.beginBranch(prompt: promptDraft)
                        isShowingBranchPrompt = false
                    }
                }
            )
        }
        .task { await prepareEngine() }
    }

    // MARK: Header (W2 lines 1-3)

    private var editorialHeader: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Rectangle()  // editorial hairline
                .frame(height: 1)
                .foregroundStyle(.tertiary)
            Text(context.script.title)
                .font(AppTheme.Typography.largeTitle)
                .padding(.top, AppTheme.Spacing.sm)
            Text(context.script.subtitle.uppercased())
                .font(.caption.weight(.medium))
                .tracking(2)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: Transcript pane

    private var transcriptPane: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            if isHoldingMic {
                // Live transcript of the user's question while the mic is
                // held (UX-08 §4 — *listening* glow + frozen segment text).
                if !mic.liveTranscript.isEmpty {
                    Text(mic.liveTranscript)
                        .font(AppTheme.Typography.body.italic())
                        .lineSpacing(6)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                } else {
                    Text("Listening…")
                        .font(AppTheme.Typography.body.italic())
                        .foregroundStyle(.secondary)
                }
            } else if let segment = currentSegment {
                Text(segment.bodyText)
                    .font(AppTheme.Typography.body)
                    .lineSpacing(6)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                Text("No segment loaded.")
                    .font(.body)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(AppTheme.Spacing.md)
        .glassSurface(
            cornerRadius: AppTheme.Corner.lg,
            tint: BriefingsView.brassAmber.opacity(isHoldingMic ? 0.34 : 0.18)
        )
        .animation(.easeInOut(duration: 0.18), value: isHoldingMic)
    }

    // MARK: Transport (W2 line 8)

    private var transportControls: some View {
        HStack(spacing: AppTheme.Spacing.lg) {
            // The previous transport row had a "backward" placeholder
            // wired to `/* prev */` — the engine has no `previousSegment`
            // (the segment rail below handles jumping to any segment,
            // including earlier ones via `jump(toSegment:)`), so the
            // button was a silent no-op. Dropped rather than leave a
            // visibly broken affordance.
            Button {
                Task { engine.isPlaying ? await engine.pause() : await engine.resume() }
            } label: {
                Image(systemName: engine.isPlaying ? "pause.fill" : "play.fill")
                    .font(.largeTitle)
            }
            .accessibilityLabel(engine.isPlaying ? "Pause briefing" : "Resume briefing")
            Button {
                Task { await engine.skipCurrentSegment() }
            } label: {
                Image(systemName: "forward.fill")
            }
            .accessibilityLabel("Skip to next segment")
            Spacer()
            Text(formatProgress())
                .font(.system(.caption, design: .monospaced).weight(.medium))
                .foregroundStyle(.secondary)
        }
        .buttonStyle(.plain)
    }

    // MARK: Per-segment actions (W2 line 9)

    private var actionsRow: some View {
        HStack(spacing: AppTheme.Spacing.lg) {
            actionButton(label: "deeper", icon: "arrow.turn.down.right") {
                isShowingBranchPrompt = true
            }
            actionButton(label: "skip", icon: "arrow.down.to.line") {
                Task { await engine.skipCurrentSegment() }
            }
            Spacer()
            micButton
        }
    }

    /// Hold-to-talk mic. UX-08 §5 *Hold-to-pause-and-ask*: pressing ducks
    /// briefing audio, recording starts; releasing finalises the question
    /// and hands it to `engine.endBranch`. Permission denial collapses the
    /// glyph into a typed-sheet fallback so the surface is never gated.
    @ViewBuilder
    private var micButton: some View {
        if mic.phase == .denied {
            // Fallback per spec: UI-only "Pause + ask in chat" path.
            actionButton(label: "ask", icon: "text.bubble") {
                Task { await engine.pause() }
                isShowingBranchPrompt = true
            }
        } else {
            VStack(spacing: 2) {
                Image(systemName: isHoldingMic ? "mic.fill" : "mic")
                    .foregroundStyle(isHoldingMic ? BriefingsView.brassAmber : .primary)
                Text(isHoldingMic ? "listening" : "hold")
                    .font(.caption2.weight(.medium))
                    .tracking(1.5)
            }
            .padding(.horizontal, AppTheme.Spacing.sm)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        guard !isHoldingMic else { return }
                        isHoldingMic = true
                        Task { await beginMicCapture() }
                    }
                    .onEnded { _ in
                        guard isHoldingMic else { return }
                        isHoldingMic = false
                        Task { await endMicCapture() }
                    }
            )
        }
    }

    @MainActor
    private func beginMicCapture() async {
        // Pause the briefing so the user's question starts a clean branch.
        await engine.beginBranch(prompt: "")
        await mic.start { _ in
            // Live transcript already mirrored on `mic.liveTranscript`; the
            // closure exists so SwiftUI body recomputes via @Observable.
        }
    }

    @MainActor
    private func endMicCapture() async {
        let transcript = mic.stop()
        // For now, the agent answer pipeline (Lane 6/8) is not wired to this
        // surface. We close the branch with a placeholder echo so the engine's
        // pause-and-resume contract still holds end-to-end. When the agent
        // surface lands, swap this for the real answer text.
        let answer = transcript.isEmpty
            ? "I didn't catch that — try again."
            : "You asked: \(transcript)"
        await engine.endBranch(prompt: transcript, answerText: answer)
    }

    private func actionButton(
        label: String,
        icon: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            VStack(spacing: 2) {
                Image(systemName: icon)
                Text(label)
                    .font(.caption2.weight(.medium))
                    .tracking(1.5)
            }
        }
        .buttonStyle(.plain)
    }

    // MARK: Segment rail (W2 lines 13-16, W3)

    private var segmentRail: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text("SEGMENTS")
                .font(.caption2.weight(.semibold))
                .tracking(2)
                .foregroundStyle(.secondary)
            ForEach(context.script.segments) { segment in
                Button {
                    Task { await engine.jump(toSegment: segment.id) }
                } label: {
                    railRow(segment: segment)
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func railRow(segment: BriefingSegment) -> some View {
        let isActive = engine.activeSegmentID == segment.id
        return HStack {
            Text("\(segment.index + 1).")
                .font(.system(.body, design: .monospaced))
                .foregroundStyle(.secondary)
                .frame(width: 28, alignment: .trailing)
            Text(segment.title)
                .font(.body.weight(isActive ? .semibold : .regular))
            Spacer()
            Text(formatTimestamp(cumulativeStartFor(segment)))
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
        }
        .padding(AppTheme.Spacing.sm)
        .glassSurface(
            cornerRadius: AppTheme.Corner.md,
            tint: isActive ? BriefingsView.brassAmber.opacity(0.22) : Color.clear.opacity(0.0)
        )
    }

    // MARK: Attribution strip

    private var attributionStrip: some View {
        Group {
            if let chip = currentSegment?.attributions.first {
                HStack {
                    Image(systemName: "quote.bubble.fill")
                        .foregroundStyle(.secondary)
                    Text(chip.displayLabel)
                        .font(.caption.weight(.medium))
                    Spacer()
                    Image(systemName: "arrow.up.forward")
                        .foregroundStyle(.secondary)
                }
                .padding(AppTheme.Spacing.sm)
                .glassSurface(
                    cornerRadius: AppTheme.Corner.md,
                    tint: BriefingsView.brassAmber.opacity(0.10)
                )
            }
        }
    }

    // The toolbar used to carry a "share-card" Button wired to a
    // `/* share-card */` placeholder — tapping did nothing because
    // briefings don't yet have a share representation (no deep link,
    // no image-renderer card). Visible-but-dead is worse than absent,
    // so the toolbar block is gone until a real share path exists.

    // MARK: Helpers

    private var currentSegment: BriefingSegment? {
        guard let id = engine.activeSegmentID else {
            return context.script.segments.first
        }
        return context.script.segments.first { $0.id == id }
    }

    private func cumulativeStartFor(_ segment: BriefingSegment) -> TimeInterval {
        var total: TimeInterval = 0
        for s in context.script.segments {
            if s.id == segment.id { return total }
            total += s.targetSeconds
        }
        return total
    }

    private func formatTimestamp(_ seconds: TimeInterval) -> String {
        let mm = Int(seconds) / 60
        let ss = Int(seconds) % 60
        return String(format: "%d:%02d", mm, ss)
    }

    private func formatProgress() -> String {
        let total = context.script.totalDurationSeconds
        let current = engine.tracks
            .prefix(engine.currentTrackIndex)
            .reduce(0.0) { $0 + $1.durationSeconds }
        return "\(formatTimestamp(current)) / \(formatTimestamp(total))"
    }

    private var background: some View {
        LinearGradient(
            colors: [
                BriefingsView.brassAmber.opacity(0.10),
                BriefingsView.brassAmber.opacity(0.04),
                Color(.systemBackground),
            ],
            startPoint: .top, endPoint: .bottom
        )
        .ignoresSafeArea()
    }

    // MARK: Engine setup

    @MainActor
    private func prepareEngine() async {
        // Reflect existing mic / speech authorization into `mic.phase` so the
        // mic button renders the correct state on first appear (without
        // prompting). Only an explicit hold actually requests permission.
        let micStatus = AVAudioApplication.shared.recordPermission
        let srStatus = SFSpeechRecognizer.authorizationStatus()
        if micStatus == .denied || (srStatus != .notDetermined && srStatus != .authorized) {
            // Force the denied chrome so the typed-sheet fallback is exposed.
            _ = await mic.requestPermission()
        }

        // Build minimal `BriefingTrack`s from segments so the engine can
        // navigate without re-running the composer. Real builds will pass the
        // composer's `tracks` through — for saved-on-disk briefings (which
        // the lane spec persists at `<id>.m4a`), we synthesise a single
        // track-per-segment placeholder pointing at the stitched audio.
        let storage = (try? BriefingStorage())
        guard let assetURL = storage?.audioURL(id: context.script.id) else {
            assetMissingMessage = "Briefing audio is missing. Try recomposing this briefing."
            return
        }
        var tracks: [BriefingTrack] = []
        var cursor: TimeInterval = 0
        for segment in context.script.segments {
            let duration = segment.targetSeconds
            tracks.append(BriefingTrack(
                segmentID: segment.id,
                indexInSegment: 0,
                kind: .tts,
                audioURL: assetURL,
                startInTrackSeconds: cursor,
                endInTrackSeconds: cursor + duration,
                transcriptText: segment.bodyText,
                attribution: segment.attributions.first
            ))
            cursor += duration
        }
        engine.load(context.script, tracks: tracks, host: FakeBriefingPlayerHost())

        // Lean-back river opt-in: auto-play once tracks are loaded.
        if autoplay {
            await engine.play(stitchedURL: assetURL)
        }
    }
}
