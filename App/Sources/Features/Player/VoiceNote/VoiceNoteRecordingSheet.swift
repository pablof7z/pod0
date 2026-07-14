import SwiftUI

/// Half-sheet for recording a time-anchored voice note in the player.
///
/// Tap the mic button → recording starts (auto-started on appear).
/// Tap again or press Send to finalize. The transcribed utterance plus
/// the current playback position and chapter context are packaged into
/// `VoiceNoteAgentContext`, written to the store, and dispatched to the
/// agent chat via `.askAgentRequested`. Playback is paused on open and
/// resumed when the sheet dismisses (unless something else owns playback).
struct VoiceNoteRecordingSheet: View {

    @Environment(AppStateStore.self) private var store
    @Bindable var state: PlaybackState
    @Environment(\.dismiss) private var dismiss

    /// Playback position captured the instant the sheet appeared.
    @State private var timestampOnOpen: TimeInterval = 0
    @State private var wasPlayingOnOpen = false
    @State private var stt = VoiceNoteRealtimeSTT()
    @State private var startError: String?
    @State private var isSending = false

    var body: some View {
        VStack(spacing: 0) {
            header
                .padding(.top, AppTheme.Spacing.lg)
                .padding(.horizontal, AppTheme.Spacing.lg)

            Spacer(minLength: AppTheme.Spacing.md)

            micButton
                .padding(.vertical, AppTheme.Spacing.lg)

            transcriptArea
                .padding(.horizontal, AppTheme.Spacing.lg)

            Spacer(minLength: AppTheme.Spacing.md)

            if let err = startError ?? stt.errorMessage {
                errorHint(err)
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    .padding(.bottom, AppTheme.Spacing.sm)
            }

            actionRow
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.bottom, AppTheme.Spacing.lg)
        }
        .frame(maxWidth: .infinity)
        .presentationDetents([.fraction(0.45), .medium])
        .presentationDragIndicator(.visible)
        .onAppear(perform: handleAppear)
        .onDisappear(perform: handleDisappear)
    }

    // MARK: - Header

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 4) {
                Label("Voice note", systemImage: "waveform.badge.mic")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.tint)

                if let episode = state.episode {
                    Text(episode.title)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.primary)
                        .lineLimit(1)
                }

                HStack(spacing: 6) {
                    Image(systemName: "clock")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    Text("At \(VoiceNoteAgentContext.formatStamp(timestampOnOpen))")
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    if let chapter = activeChapterTitle {
                        Text("·")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                        Text(chapter)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }

            Spacer()
        }
    }

    // MARK: - Mic button

    private var micButton: some View {
        Button(action: micTapped) {
            ZStack {
                // Ripple ring driven by audio level
                Circle()
                    .stroke(Color.accentColor.opacity(0.3), lineWidth: 2)
                    .scaleEffect(1 + CGFloat(stt.level) * 0.6)
                    .opacity(stt.isRecording ? 1 : 0)
                    .animation(.easeOut(duration: 0.08), value: stt.level)

                Circle()
                    .fill(Color.accentColor.opacity(stt.isRecording ? 0.15 : 0.08))
                    .frame(width: 88, height: 88)

                Image(systemName: micIcon)
                    .font(.system(size: 36, weight: .semibold))
                    .foregroundStyle(.tint)
                    .scaleEffect(stt.isStarting ? 0.85 : 1)
                    .animation(.spring(response: 0.3), value: stt.isStarting)
            }
            .frame(width: 110, height: 110)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(micAccessibilityLabel)
    }

    private var micIcon: String {
        if stt.isStarting { return "mic.circle" }
        if stt.isRecording { return "mic.fill" }
        return "mic"
    }

    private var micAccessibilityLabel: String {
        if stt.isStarting { return "Connecting microphone" }
        if stt.isRecording { return "Tap to send voice note" }
        return "Tap to start recording"
    }

    // MARK: - Transcript area

    private var transcriptArea: some View {
        Group {
            if !stt.transcript.isEmpty {
                ScrollView {
                    Text(stt.transcript)
                        .font(.body)
                        .foregroundStyle(.primary)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                }
                .frame(maxHeight: 100)
            } else {
                Text(statusHint)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity)
                    .animation(.easeInOut, value: stt.isRecording)
            }
        }
    }

    private var statusHint: String {
        if stt.isStarting { return "Starting…" }
        if stt.isRecording { return "Listening…" }
        return "Tap the mic to record"
    }

    // MARK: - Error hint

    private func errorHint(_ message: String) -> some View {
        Label(message, systemImage: "exclamationmark.triangle.fill")
            .font(.caption)
            .foregroundStyle(.red)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
    }

    // MARK: - Action row

    private var actionRow: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            Button(role: .destructive, action: cancelTapped) {
                Text("Cancel")
                    .font(.subheadline.weight(.semibold))
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glass)

            Button(action: sendTapped) {
                Label(isSending ? "Sending…" : "Send", systemImage: "arrow.up.circle.fill")
                    .font(.subheadline.weight(.semibold))
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glass)
            .disabled(isSending || (!stt.isRecording && stt.transcript.isEmpty))
        }
    }

    // MARK: - Active chapter resolution

    private var activeChapterTitle: String? {
        guard let episode = state.episode,
              let chapters = (store.episode(id: episode.id) ?? episode).chapters?
                  .filter(\.includeInTableOfContents),
              !chapters.isEmpty
        else { return nil }
        return chapters.active(at: timestampOnOpen)?.title
    }

    private var activeChapterBounds: (start: TimeInterval, end: TimeInterval?)? {
        guard let episode = state.episode,
              let chapters = (store.episode(id: episode.id) ?? episode).chapters?
                  .filter(\.includeInTableOfContents),
              !chapters.isEmpty,
              let active = chapters.active(at: timestampOnOpen)
        else { return nil }

        let idx = chapters.firstIndex(where: { $0.id == active.id })
        let resolvedEnd: TimeInterval? = active.endTime
            ?? idx.flatMap { i in
                let next = chapters.index(after: i)
                return next < chapters.endIndex ? chapters[next].startTime : nil
            }
        return (active.startTime, resolvedEnd)
    }

    // MARK: - Actions

    private func handleAppear() {
        timestampOnOpen = state.currentTime
        wasPlayingOnOpen = state.isPlaying
        if state.isPlaying { state.pause() }
        Task { await startRecording() }
    }

    private func handleDisappear() {
        if stt.isRecording || stt.isStarting { stt.cancel() }
        if wasPlayingOnOpen { state.play() }
    }

    private func startRecording() async {
        startError = nil
        do {
            try await stt.start(modelID: store.state.settings.elevenLabsSTTModel)
        } catch {
            startError = error.localizedDescription
        }
    }

    private func micTapped() {
        Haptics.selection()
        if stt.isRecording {
            sendTapped()
        } else if !stt.isStarting {
            Task { await startRecording() }
        }
    }

    private func cancelTapped() {
        Haptics.selection()
        stt.cancel()
        dismiss()
    }

    private func sendTapped() {
        guard !isSending else { return }
        let hasTranscript = !stt.transcript.isEmpty

        if !hasTranscript && !stt.isRecording {
            startError = "Nothing recorded yet. Tap the mic to start."
            return
        }

        isSending = true
        Haptics.success()

        Task {
            let utterance = await stt.stop()
            let trimmed = utterance.trimmingCharacters(in: .whitespacesAndNewlines)

            if trimmed.isEmpty {
                startError = "We didn't catch anything. Tap the mic to try again."
                isSending = false
                return
            }

            guard let episode = state.episode else {
                isSending = false
                dismiss()
                return
            }

            let subTitle = store.podcast(id: episode.podcastID)?.title ?? ""
            let bounds = activeChapterBounds

            let context = VoiceNoteAgentContext(
                episodeID: episode.id,
                subscriptionTitle: subTitle,
                episodeTitle: episode.title,
                timestamp: timestampOnOpen,
                activeChapterTitle: activeChapterTitle,
                chapterStartTime: bounds?.start,
                chapterEndTime: bounds?.end,
                userUtterance: trimmed
            )

            store.pendingVoiceNoteAgentContext = context
            NotificationCenter.default.post(name: .askAgentRequested, object: nil)
            dismiss()
        }
    }
}
