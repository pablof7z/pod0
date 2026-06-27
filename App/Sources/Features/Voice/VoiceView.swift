import SwiftUI

// MARK: - VoiceView

/// Full-screen voice conversation experience, driven entirely by the kernel
/// voice engine (`podcast.voice` actions + the `voice` snapshot projection).
///
/// The kernel owns the STT→LLM→TTS loop: `VoiceConversationManager` (Rust)
/// runs the model on each final transcript and dispatches the spoken reply,
/// while the iOS `VoiceCapability` executor performs recognition and ElevenLabs/
/// AVSpeech playback. This view is a *thin shell* over that engine — it
/// dispatches `activate`/`deactivate`/`stop` and renders the projected state
/// (`isListening`, `isSpeaking`, `partialTranscript`, `lastResponse`). It owns
/// no audio, no agent session, and no state machine of its own. This mirrors
/// the Android `VoiceScreen` design so both platforms share one canonical
/// voice path.
///
/// Presented by `RootView` as a `fullScreenCover` on `.voiceModeRequested`.
struct VoiceView: View {

    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss

    /// Caller-supplied closure to pivot back to the Ask (text) chat.
    var onSwitchToText: (() -> Void)? = nil

    /// True between the user finishing speaking and the assistant starting to
    /// speak — the kernel exposes no explicit "thinking" flag, so we infer it
    /// from the `listening → (silence) → speaking` gap to drive the orb.
    @State private var awaitingResponse = false

    /// Set when the user is dismissing voice mode so the speaking→idle
    /// transition does not re-arm listening for another turn.
    @State private var isClosing = false

    /// True once a non-empty partial transcript has been seen during the
    /// current listening turn. Gates the inferred "thinking" state so a
    /// no-speech stop (or a permission/activation failure) does not show
    /// "Thinking…" with nothing coming.
    @State private var heardSpeech = false

    /// Set right before a user-initiated `stop` so the resulting
    /// speaking→idle transition does NOT auto-re-arm the mic — tapping stop
    /// should end the assistant's turn, not silently reopen listening.
    @State private var suppressRearm = false

    // MARK: - Kernel state accessors

    private var voice: VoiceSnapshot? { store.kernel?.podcastSnapshot?.voice }
    private var isListening: Bool { voice?.isListening ?? false }
    private var isSpeaking: Bool { voice?.isSpeaking ?? false }
    private var partialTranscript: String? { voice?.partialTranscript }
    private var lastResponse: String? { voice?.lastResponse }

    // MARK: - Body

    var body: some View {
        ZStack {
            background
            VStack(spacing: AppTheme.Spacing.lg) {
                stateBadge
                Spacer()
                VoiceOrbView(state: orbState, inputRMS: 0)
                Spacer()
                captionRail
                actionRow
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.bottom, AppTheme.Spacing.lg)
            .padding(.top, AppTheme.Spacing.md)
        }
        .preferredColorScheme(.dark)
        .accessibilityIdentifier("voice.view")
        .onAppear { activate() }
        .onDisappear {
            // Always release the mic on teardown. `deactivate` (StopListening)
            // is idempotent kernel-side, so dispatching it unconditionally is
            // safe and closes the window where an external dismissal would
            // otherwise leave recognition running.
            dispatchVoice("deactivate")
        }
        .onChange(of: partialTranscript) { _, partial in
            if let partial, !partial.isEmpty { heardSpeech = true }
        }
        .onChange(of: isListening) { _, listening in
            if listening {
                heardSpeech = false
            } else if heardSpeech && !isSpeaking && !isClosing {
                // Listening stopped after the user actually spoke → a turn
                // was submitted; show the thinking orb until speech begins.
                awaitingResponse = true
            }
        }
        .onChange(of: isSpeaking) { _, speaking in
            if speaking {
                awaitingResponse = false
            } else if isClosing || suppressRearm {
                // User-initiated stop or dismissal — do not reopen the mic.
                suppressRearm = false
            } else {
                // Assistant finished naturally — re-arm listening so the
                // conversation continues hands-free (kernel STT is
                // single-utterance).
                activate()
            }
        }
    }

    // MARK: - Background

    private var background: some View {
        AppTheme.Gradients.onboardingNebula
            .ignoresSafeArea()
            .overlay(Color.black.opacity(0.18).ignoresSafeArea())
    }

    // MARK: - State badge

    private var stateBadge: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Circle()
                .fill(badgeColor)
                .frame(width: 8, height: 8)
                .opacity(badgePulses ? 0.5 : 1.0)
                .animation(
                    badgePulses
                        ? .easeInOut(duration: 0.85).repeatForever(autoreverses: true)
                        : .default,
                    value: badgePulses)
            Text(stateLabel)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.vertical, AppTheme.Spacing.sm)
        .glassSurface(cornerRadius: AppTheme.Corner.pill)
    }

    private var stateLabel: String {
        switch orbState {
        case .listening: return "Listening…"
        case .thinking: return "Thinking…"
        case .speaking: return "Speaking"
        default: return "Tap to talk"
        }
    }

    private var badgeColor: Color {
        switch orbState {
        case .listening: return AppTheme.Tint.voiceListening
        case .thinking: return AppTheme.Tint.voiceThinking
        case .speaking: return AppTheme.Tint.voiceSpeaking
        default: return .white.opacity(0.7)
        }
    }

    private var badgePulses: Bool {
        switch orbState {
        case .listening, .thinking, .speaking: return true
        default: return false
        }
    }

    // MARK: - Orb

    /// Map the projected kernel state onto the orb's visual state.
    private var orbState: VoiceOrbState {
        if isSpeaking { return .speaking }
        if isListening { return .listening }
        if awaitingResponse { return .thinking() }
        return .idle
    }

    // MARK: - Captions

    private var captionRail: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            if isListening, let partial = partialTranscript, !partial.isEmpty {
                captionRow(speaker: "You", text: partial, dimmed: true)
            }
            if let response = lastResponse, !response.isEmpty {
                captionRow(speaker: isSpeaking ? "Agent" : "•", text: response, dimmed: false)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(minHeight: 120, alignment: .bottom)
        .padding(.horizontal, AppTheme.Spacing.md)
        .animation(.easeInOut(duration: 0.25), value: lastResponse)
        .animation(.easeInOut(duration: 0.25), value: partialTranscript)
    }

    private func captionRow(speaker: String, text: String, dimmed: Bool) -> some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
            Text(speaker)
                .font(.caption2.weight(.semibold))
                .foregroundStyle(dimmed ? AppTheme.Tint.voiceListening : AppTheme.Tint.voiceThinking)
                .frame(width: 44, alignment: .leading)
            Text(text)
                .font(.callout)
                .foregroundStyle(dimmed ? Color.white.opacity(0.65) : .white)
                .frame(maxWidth: .infinity, alignment: .leading)
                .multilineTextAlignment(.leading)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(speaker) said \(text)")
    }

    // MARK: - Actions

    private var actionRow: some View {
        HStack(spacing: AppTheme.Spacing.lg) {
            closeButton
            talkButton
            switchToTextButton
        }
        .padding(.top, AppTheme.Spacing.md)
    }

    /// Primary affordance. Idle → start listening; speaking → interrupt;
    /// listening → finish the turn early (flush the transcript).
    private var talkButton: some View {
        Button {
            if isSpeaking {
                // Interrupt the assistant without reopening the mic.
                suppressRearm = true
                dispatchVoice("stop")
            } else if isListening {
                dispatchVoice("deactivate")
            } else {
                activate()
            }
        } label: {
            ZStack {
                Circle().fill(.white.opacity(0.12)).frame(width: 96, height: 96)
                Image(systemName: talkIcon)
                    .font(.system(size: 36, weight: .semibold))
                    .foregroundStyle(.white)
            }
        }
        .buttonStyle(.plain)
        .glassSurface(cornerRadius: 48, interactive: true)
        .accessibilityLabel(talkAccessibilityLabel)
    }

    private var talkIcon: String {
        switch orbState {
        case .listening: return "waveform"
        case .speaking: return "stop.fill"
        case .thinking: return "ellipsis"
        default: return "mic.fill"
        }
    }

    private var talkAccessibilityLabel: String {
        switch orbState {
        case .listening: return "Listening — tap to send"
        case .speaking: return "Tap to interrupt"
        case .thinking: return "Thinking"
        default: return "Tap to talk"
        }
    }

    private var closeButton: some View {
        Button {
            close()
        } label: {
            actionLabel(icon: "xmark", title: "Close")
        }
        .buttonStyle(.plain)
        .glassSurface(cornerRadius: AppTheme.Corner.lg, interactive: true)
        .accessibilityLabel("Close voice mode")
    }

    private var switchToTextButton: some View {
        Button {
            // `onDisappear` releases the mic; flag close so we don't re-arm
            // during the transition, and always dismiss so the cover never
            // lingers if the caller's handler doesn't dismiss synchronously.
            isClosing = true
            onSwitchToText?()
            dismiss()
        } label: {
            actionLabel(icon: "keyboard", title: "Text")
        }
        .buttonStyle(.plain)
        .glassSurface(cornerRadius: AppTheme.Corner.lg, interactive: true)
        .accessibilityLabel("Switch to text chat")
    }

    private func actionLabel(icon: String, title: String) -> some View {
        VStack(spacing: AppTheme.Spacing.xs) {
            Image(systemName: icon).font(.title2)
            Text(title).font(.caption2.weight(.medium))
        }
        .foregroundStyle(.white)
        .frame(width: 70, height: 70)
    }

    // MARK: - Dispatch helpers

    private func activate() {
        awaitingResponse = false
        dispatchVoice("activate")
    }

    private func close() {
        isClosing = true
        dispatchVoice("deactivate")
        dismiss()
    }

    private func dispatchVoice(_ op: String) {
        store.kernel?.dispatch(namespace: "podcast.voice", body: ["op": op])
    }
}
