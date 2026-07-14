import SwiftUI

// MARK: - VoiceView

/// Full-screen voice conversation experience.
///
/// Layout (top → bottom):
///   • State badge ("Listening", "Thinking", …)
///   • Animated orb (center, ~60% of screen)
///   • Caption rail (last 3-4 captions, scrolling)
///   • Bottom action row: PTT button (large), ambient toggle, "switch to text"
///
/// The orchestrator (`RootView`) is expected to mount this as the contents
/// of a Voice tab. We don't add ourselves to `RootTab` here — that's
/// handled at merge time per the lane spec.
struct VoiceView: View {

    @State private var manager = AudioConversationManager()
    @Environment(\.dismiss) private var dismiss

    /// Caller-supplied closure to pivot back to the Ask (text) chat tab.
    var onSwitchToText: (() -> Void)? = nil

    /// PTT press state — used to drive `start/end PushToTalk` from a
    /// `LongPressGesture`-style interaction without race conditions.
    @State private var isPressing = false

    var body: some View {
        ZStack {
            background
            VStack(spacing: AppTheme.Spacing.lg) {
                stateBadge
                Spacer()
                orb
                Spacer()
                captionRail
                actionRow
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.bottom, AppTheme.Spacing.lg)
            .padding(.top, AppTheme.Spacing.md)
        }
        .preferredColorScheme(.dark)
    }

    // MARK: - Background

    private var background: some View {
        AppTheme.Gradients.onboardingNebula
            .ignoresSafeArea()
            .overlay(
                Color.black.opacity(0.18).ignoresSafeArea()
            )
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
                    value: badgePulses
                )
            Text(stateLabel)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.white)
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.vertical, AppTheme.Spacing.sm)
        .glassSurface(cornerRadius: AppTheme.Corner.pill)
    }

    private var stateLabel: String {
        switch manager.state {
        case .idle: return manager.isAmbient ? "Ambient · waiting" : "Tap and hold to talk"
        case .listening: return "Listening…"
        case .thinking: return "Thinking…"
        case .speaking: return "Speaking"
        case .duckedWhileBriefing: return "Briefing"
        case .error(let err): return errorLabel(err)
        }
    }

    private var badgeColor: Color {
        switch manager.state {
        case .listening: return AppTheme.Tint.voiceListening
        case .thinking: return AppTheme.Tint.voiceThinking
        case .speaking: return AppTheme.Tint.voiceSpeaking
        case .duckedWhileBriefing: return .gray
        case .error: return .red
        case .idle: return .white.opacity(0.7)
        }
    }

    private var badgePulses: Bool {
        switch manager.state {
        case .listening, .thinking, .speaking: return true
        default: return false
        }
    }

    private func errorLabel(_ error: VoiceError) -> String {
        switch error {
        case .permissionDenied: return "Microphone or speech access denied"
        case .recognizerUnavailable: return "Speech recogniser unavailable"
        case .ttsFailed: return "TTS error — using local voice"
        case .agentFailed: return "Agent error"
        case .audioRouteFailed: return "Audio route error"
        case .unknown: return "Voice error"
        }
    }

    // MARK: - Orb

    private var orb: some View {
        VoiceOrbView(state: orbState, inputRMS: orbInputRMS)
    }

    /// Map the conversation manager's state machine onto the orb's visual
    /// state. Error and briefing-handoff states collapse onto `.idle` so
    /// the orb stays consistent — error chrome lives in the badge.
    private var orbState: VoiceOrbState {
        switch manager.state {
        case .idle: return .idle
        case .listening: return .listening
        case .thinking: return .thinking(toolName: nil)
        case .speaking:
            return manager.isUserBargingIn ? .bargeIn : .speaking
        case .duckedWhileBriefing: return .idle
        case .error: return .idle
        }
    }

    /// Hook for the orb's live input pulse. While we're in the barge-in
    /// preview window the manager surfaces a non-zero level; otherwise the
    /// orb stays still. Future work pipes the detector's running RMS
    /// through the manager.
    private var orbInputRMS: Float {
        manager.isUserBargingIn ? 0.18 : 0
    }

    // MARK: - Captions

    private var captionRail: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            ForEach(manager.captions.entries.suffix(3)) { caption in
                VoiceCaptionRow(caption: caption)
                    .transition(.opacity.combined(with: .move(edge: .bottom)))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .frame(minHeight: 120, alignment: .bottom)
        .padding(.horizontal, AppTheme.Spacing.md)
        .animation(.easeInOut(duration: 0.25), value: manager.captions.entries.count)
    }

    // MARK: - Actions

    private var actionRow: some View {
        HStack(spacing: AppTheme.Spacing.lg) {
            ambientToggle
            pttButton
            switchToTextButton
        }
        .padding(.top, AppTheme.Spacing.md)
    }

    private var pttButton: some View {
        Button {
            // Tap toggles ambient if already in ambient; otherwise serves
            // as the Cancel target while in PTT.
            if manager.isAmbient {
                manager.exitAmbientMode()
            } else if case .speaking = manager.state {
                manager.interruptCurrentSpeech()
            }
        } label: {
            ZStack {
                Circle()
                    .fill(.white.opacity(0.12))
                    .frame(width: 96, height: 96)
                Image(systemName: pttIcon)
                    .font(.system(size: 36, weight: .semibold))
                    .foregroundStyle(.white)
            }
        }
        .buttonStyle(.plain)
        .glassSurface(cornerRadius: 48, interactive: true)
        .accessibilityLabel(pttAccessibilityLabel)
        .gesture(pressGesture)
    }

    private var pttIcon: String {
        switch manager.state {
        case .listening: return "waveform"
        case .speaking: return "stop.fill"
        case .thinking: return "ellipsis"
        default: return manager.isAmbient ? "ear.fill" : "mic.fill"
        }
    }

    private var pttAccessibilityLabel: String {
        switch manager.state {
        case .listening: return "Listening — release to send"
        case .speaking: return "Tap to interrupt"
        case .thinking: return "Thinking"
        default: return manager.isAmbient ? "Ambient mode active — tap to exit" : "Press and hold to talk"
        }
    }

    /// `DragGesture` with `minimumDistance: 0` is the most reliable PTT
    /// pattern in SwiftUI — `LongPressGesture` doesn't expose `onEnded`
    /// for the release event in the way we want here.
    private var pressGesture: some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { _ in
                if !isPressing && !manager.isAmbient {
                    isPressing = true
                    manager.startPushToTalk()
                }
            }
            .onEnded { _ in
                if isPressing {
                    isPressing = false
                    manager.endPushToTalk()
                }
            }
    }

    private var ambientToggle: some View {
        Button {
            if manager.isAmbient {
                manager.exitAmbientMode()
            } else {
                manager.enterAmbientMode()
            }
        } label: {
            VStack(spacing: AppTheme.Spacing.xs) {
                Image(systemName: manager.isAmbient ? "ear.and.waveform" : "ear")
                    .font(.title2)
                Text(manager.isAmbient ? "On" : "Ambient")
                    .font(.caption2.weight(.medium))
            }
            .foregroundStyle(.white)
            .frame(width: 70, height: 70)
        }
        .buttonStyle(.plain)
        .glassSurface(cornerRadius: AppTheme.Corner.lg, interactive: true)
        .accessibilityLabel(manager.isAmbient ? "Disable ambient mode" : "Enable ambient mode")
    }

    private var switchToTextButton: some View {
        Button {
            if let onSwitchToText {
                onSwitchToText()
            } else {
                dismiss()
            }
        } label: {
            VStack(spacing: AppTheme.Spacing.xs) {
                Image(systemName: "keyboard")
                    .font(.title2)
                Text("Text")
                    .font(.caption2.weight(.medium))
            }
            .foregroundStyle(.white)
            .frame(width: 70, height: 70)
        }
        .buttonStyle(.plain)
        .glassSurface(cornerRadius: AppTheme.Corner.lg, interactive: true)
        .accessibilityLabel("Switch to text chat")
    }
}

// MARK: - VoiceCaptionRow

/// Single caption row used inside `VoiceView`'s caption rail.
private struct VoiceCaptionRow: View {

    let caption: VoiceCaption

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
            Text(speakerLabel)
                .font(.caption2.weight(.semibold))
                .foregroundStyle(speakerColor)
                .frame(width: 44, alignment: .leading)
            Text(caption.text)
                .font(.callout)
                .foregroundStyle(textColor)
                .frame(maxWidth: .infinity, alignment: .leading)
                .multilineTextAlignment(.leading)
        }
        .padding(.vertical, AppTheme.Spacing.xs)
        .accessibilityLabel("\(speakerLabel) said \(caption.text)")
    }

    private var speakerLabel: String {
        caption.speaker == .user ? "You" : "Agent"
    }

    private var speakerColor: Color {
        caption.speaker == .user
            ? AppTheme.Tint.voiceListening
            : AppTheme.Tint.voiceThinking
    }

    private var textColor: Color {
        caption.stability == .partial
            ? .white.opacity(0.65)
            : .white
    }
}

// MARK: - Preview

#Preview {
    VoiceView()
}
