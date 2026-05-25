import SwiftUI

// MARK: - VoiceModeView
//
// Full-screen voice-mode overlay. Reads its state exclusively from the
// kernel snapshot (`snapshot.voice`) and drives transitions via
// `podcast.voice.activate` / `deactivate` actions. Per D2 the view is a
// pure projection of the Rust state — no local @State for "is listening"
// or "is speaking", just bindings into the snapshot.
//
// Tap the orb to start/stop listening. The orb pulses when listening,
// glows when the assistant is speaking, and rests in between. The
// caption underneath flips between the streaming partial transcript
// (while listening) and the last response (while idle / speaking).
//
// File-length budget: ≤ 300 LOC. Auxiliary subviews live inline.

/// Full-screen voice-mode UI. Presented as a sheet from the Library
/// toolbar microphone button (and any future surface that wants to
/// drop into voice mode).
struct VoiceModeView: View {
    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    /// One drives the orb animation. SwiftUI starts/stops the pulse
    /// based on the snapshot's `isListening`/`isSpeaking` flags.
    @State private var animationPhase: CGFloat = 0

    var body: some View {
        ZStack {
            backgroundGradient
                .ignoresSafeArea()
            VStack(spacing: 32) {
                Spacer()
                orb
                caption
                Spacer()
                doneButton
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 32)
        }
        .preferredColorScheme(.dark)
        .onAppear(perform: startPulse)
        .onDisappear(perform: ensureListeningStopped)
    }

    private var voice: VoiceSnapshot {
        model.podcastSnapshot?.voice ?? VoiceSnapshot()
    }

    // MARK: - Background

    private var backgroundGradient: some View {
        LinearGradient(
            colors: [
                Color(red: 0.07, green: 0.07, blue: 0.12),
                Color(red: 0.12, green: 0.04, blue: 0.22),
            ],
            startPoint: .top,
            endPoint: .bottom
        )
    }

    // MARK: - Orb

    private var orb: some View {
        let scale: CGFloat = {
            if voice.isListening {
                return 1.0 + 0.08 * sin(animationPhase * .pi * 2)
            } else if voice.isSpeaking {
                return 1.06
            }
            return 1.0
        }()
        return Button(action: toggleListening) {
            ZStack {
                Circle()
                    .fill(orbGradient)
                    .frame(width: 220, height: 220)
                    .shadow(color: orbGlowColor, radius: voice.isSpeaking ? 48 : 24)
                    .scaleEffect(scale)
                    .animation(.easeInOut(duration: 0.4), value: voice.isListening)
                    .animation(.easeInOut(duration: 0.4), value: voice.isSpeaking)
                Image(systemName: orbIcon)
                    .font(.system(size: 56, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.92))
                    .symbolEffect(.bounce, value: voice.isListening)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(voice.isListening ? "Stop listening" : "Start listening")
        .accessibilityHint("Voice mode toggle")
    }

    private var orbGradient: LinearGradient {
        let colors: [Color] = {
            if voice.isSpeaking {
                return [Color.purple.opacity(0.95), Color.indigo.opacity(0.85)]
            } else if voice.isListening {
                return [Color.blue.opacity(0.95), Color.cyan.opacity(0.8)]
            }
            return [Color.gray.opacity(0.55), Color.black.opacity(0.65)]
        }()
        return LinearGradient(colors: colors, startPoint: .topLeading, endPoint: .bottomTrailing)
    }

    private var orbGlowColor: Color {
        if voice.isSpeaking { return Color.purple.opacity(0.7) }
        if voice.isListening { return Color.cyan.opacity(0.5) }
        return Color.black.opacity(0.4)
    }

    private var orbIcon: String {
        if voice.isSpeaking { return "waveform" }
        if voice.isListening { return "mic.fill" }
        return "mic"
    }

    // MARK: - Caption

    private var caption: some View {
        VStack(spacing: 12) {
            Text(headlineText)
                .font(.headline.weight(.medium))
                .foregroundStyle(.white.opacity(0.9))
                .multilineTextAlignment(.center)
            if let detail = detailText {
                Text(detail)
                    .font(.body)
                    .foregroundStyle(.white.opacity(0.75))
                    .multilineTextAlignment(.center)
                    .lineLimit(4)
                    .transition(.opacity)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, 16)
    }

    private var headlineText: String {
        if voice.isListening { return "Listening…" }
        if voice.isSpeaking { return "Speaking…" }
        if voice.lastResponse != nil { return "Tap to speak again" }
        return "Tap the orb to start"
    }

    private var detailText: String? {
        if voice.isListening, let partial = voice.partialTranscript, !partial.isEmpty {
            return partial
        }
        if !voice.isListening, let last = voice.lastResponse, !last.isEmpty {
            return last
        }
        return nil
    }

    // MARK: - Done button

    private var doneButton: some View {
        Button {
            ensureListeningStopped()
            dismiss()
        } label: {
            Text("Done")
                .font(.headline.weight(.semibold))
                .foregroundStyle(.white)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
                .background(Color.white.opacity(0.18), in: Capsule())
        }
        .accessibilityIdentifier("voice-mode-done")
    }

    // MARK: - Actions

    private func toggleListening() {
        let op = voice.isListening ? "deactivate" : "activate"
        model.dispatch(namespace: "podcast.voice", body: ["op": op])
    }

    private func ensureListeningStopped() {
        guard voice.isListening else { return }
        model.dispatch(namespace: "podcast.voice", body: ["op": "deactivate"])
    }

    private func startPulse() {
        withAnimation(.linear(duration: 1.4).repeatForever(autoreverses: false)) {
            animationPhase = 1
        }
    }
}

#Preview {
    VoiceModeView()
        .environment(KernelModel())
}
