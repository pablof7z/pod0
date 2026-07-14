import SwiftUI

/// Half-sheet that records a spoken feedback note and publishes it as a
/// new thread on send. Pattern mirrors the player's voice-note sheet but
/// the send action goes through `FeedbackStore.publishThread` instead of
/// dispatching to the agent.
///
/// Identity is auto-provisioned by `UserIdentityStore.publishFeedbackNote`
/// (it generates a local key on first use), so this sheet does not gate
/// on `hasIdentity` — the user can always speak and ship.
struct FeedbackVoiceRecordingSheet: View {

    let store: FeedbackStore
    @Bindable var workflow: FeedbackWorkflow
    @Environment(AppStateStore.self) private var appStore
    @Environment(UserIdentityStore.self) private var userIdentity
    @Environment(\.dismiss) private var dismiss

    @StateObject private var stt = VoiceNoteRealtimeSTT()
    @State private var startError: String?
    @State private var publishError: String?
    @State private var isSending = false

    private enum Layout {
        static let micOuter: CGFloat = 110
        static let micInner: CGFloat = 88
        static let micIconSize: CGFloat = 36
        static let ringScaleMultiplier: CGFloat = 0.6
        static let transcriptMaxHeight: CGFloat = 120
    }

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

            if let err = publishError ?? startError ?? stt.errorMessage {
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
        .onAppear { Task { await startRecording() } }
        .onDisappear {
            if stt.isRecording || stt.isStarting { stt.cancel() }
        }
    }

    // MARK: - Header

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Label("Voice feedback", systemImage: "waveform.badge.mic")
                    .font(AppTheme.Typography.caption.weight(.semibold))
                    .foregroundStyle(.tint)
                Text(workflow.selectedCategory.rawValue)
                    .font(AppTheme.Typography.subheadline.weight(.semibold))
                    .foregroundStyle(.primary)
            }
            Spacer()
        }
    }

    // MARK: - Mic

    private var micButton: some View {
        Button(action: micTapped) {
            ZStack {
                Circle()
                    .stroke(Color.accentColor.opacity(0.3), lineWidth: 2)
                    .scaleEffect(1 + CGFloat(stt.level) * Layout.ringScaleMultiplier)
                    .opacity(stt.isRecording ? 1 : 0)
                    .animation(.easeOut(duration: 0.08), value: stt.level)

                Circle()
                    .fill(Color.accentColor.opacity(stt.isRecording ? 0.15 : 0.08))
                    .frame(width: Layout.micInner, height: Layout.micInner)

                Image(systemName: micIcon)
                    .font(.system(size: Layout.micIconSize, weight: .semibold))
                    .foregroundStyle(.tint)
                    .scaleEffect(stt.isStarting ? 0.85 : 1)
                    .animation(.spring(response: 0.3), value: stt.isStarting)
            }
            .frame(width: Layout.micOuter, height: Layout.micOuter)
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
        if stt.isRecording { return "Tap to publish voice feedback" }
        return "Tap to start recording"
    }

    // MARK: - Transcript / status

    private var transcriptArea: some View {
        Group {
            if !stt.transcript.isEmpty {
                ScrollView {
                    Text(stt.transcript)
                        .font(AppTheme.Typography.body)
                        .foregroundStyle(.primary)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                }
                .frame(maxHeight: Layout.transcriptMaxHeight)
            } else {
                Text(statusHint)
                    .font(AppTheme.Typography.subheadline)
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

    private func errorHint(_ message: String) -> some View {
        Label(message, systemImage: "exclamationmark.triangle.fill")
            .font(AppTheme.Typography.caption)
            .foregroundStyle(AppTheme.Tint.error)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
    }

    // MARK: - Actions

    private var actionRow: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            Button(role: .destructive, action: cancelTapped) {
                Text("Cancel")
                    .font(AppTheme.Typography.subheadline.weight(.semibold))
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glass)

            Button(action: sendTapped) {
                Label(isSending ? "Sending…" : "Send", systemImage: "arrow.up.circle.fill")
                    .font(AppTheme.Typography.subheadline.weight(.semibold))
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.glass)
            .disabled(isSending || (!stt.isRecording && stt.transcript.isEmpty))
        }
    }

    private func startRecording() async {
        startError = nil
        publishError = nil
        do {
            try await stt.start(modelID: appStore.state.settings.elevenLabsSTTModel)
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
        publishError = nil

        Task {
            let transcript = await stt.stop()
            let trimmed = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.isEmpty {
                startError = "We didn't catch anything. Tap the mic to try again."
                isSending = false
                return
            }
            do {
                _ = try await store.publishThread(
                    category: workflow.selectedCategory,
                    content: trimmed,
                    image: nil,
                    identity: userIdentity
                )
                Haptics.success()
                dismiss()
            } catch {
                publishError = error.localizedDescription
                Haptics.error()
                isSending = false
            }
        }
    }
}
