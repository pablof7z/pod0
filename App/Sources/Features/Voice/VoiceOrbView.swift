import SwiftUI

// MARK: - VoiceOrbState

/// Single source of truth for the orb's visual state. Higher-level views
/// derive this from the conversation manager / barge-in detector and
/// hand it in — the orb itself owns no audio state.
enum VoiceOrbState: Equatable, Sendable {
    case idle
    case listening
    case transcribing
    case thinking(toolName: String? = nil)
    case speaking
    case bargeIn
}

// MARK: - VoiceOrbView

/// Single state-machine glass orb. Morphs between idle / listening /
/// transcribing / thinking / speaking / barge-in shapes using iOS 26
/// `glassEffect(.regular.interactive())` and `glassEffectID` for visual
/// continuity across transitions.
///
/// The orb has one geometry — a circle whose diameter and tint vary with
/// state — plus an optional tool chip that morphs out of its perimeter
/// during `.thinking(toolName:)` via `glassEffectUnion`. Per the UX spec
/// (`docs/spec/briefs/ux-06-voice-mode.md` §4) all transitions are
/// spring-based except the `.bargeIn` snap, which is the only deliberately
/// sharp motion in the system.
///
/// Sizing (matches §4 of the spec):
/// - idle: 24 pt bead
/// - listening: 64 pt lens (slight horizontal squash for the "lens" feel)
/// - transcribing: 64 pt circle, subtle iridescence
/// - thinking: 56 pt circle + chip
/// - speaking: 96 pt bloom with 2.4 s breath rhythm
/// - bargeIn: 0.7× speaking size, listening-blue tint, breath halted
struct VoiceOrbView: View {

    let state: VoiceOrbState

    /// Live mic input RMS, 0...1. Drives the listening/bargeIn ripple.
    /// Default 0 keeps the orb still in previews.
    var inputRMS: Float = 0

    /// Optional tint sampled from the now-playing artwork. Defaults to a
    /// neutral system blue — the state-specific tints layer on top.
    var artworkTint: Color = Color(red: 0.36, green: 0.55, blue: 0.95)

    /// Honour Reduce Motion: collapse the breath / ripple to opacity-only
    /// transitions when the system flag is set.
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    /// Time anchor for the deterministic phase used by the breath / ripple.
    @State private var anchor: Date = .init()

    /// Namespace shared between the orb core and the tool chip so
    /// `glassEffectID` keeps morph continuity across state changes.
    @Namespace private var orbNamespace

    var body: some View {
        GlassEffectContainer(spacing: 8) {
            HStack(spacing: 8) {
                orbCore
                if case .thinking(let toolName?) = state {
                    toolChip(label: toolName)
                        .transition(.scale(scale: 0.6).combined(with: .opacity))
                }
            }
        }
        .frame(width: containerSize, height: containerSize)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .animation(transitionAnimation, value: state)
    }

    // MARK: - Core orb

    private var orbCore: some View {
        TimelineView(.animation(minimumInterval: reduceMotion ? 1 : 1.0 / 60.0)) { context in
            let phase = context.date.timeIntervalSince(anchor)
            let scale = currentScale(phase: phase)
            ZStack {
                if !reduceMotion {
                    Circle()
                        .fill(haloGradient)
                        .frame(width: diameter * 1.6, height: diameter * 1.6)
                        .blur(radius: 22)
                        .opacity(haloOpacity(phase: phase))
                }

                Circle()
                    .frame(width: diameter, height: diameter)
                    .scaleEffect(x: lensSquashX, y: lensSquashY, anchor: .center)
                    .scaleEffect(scale)
                    .glassEffect(
                        .regular.tint(currentTint).interactive(),
                        in: .circle
                    )
                    .glassEffectID("orb.core", in: orbNamespace)
                    .overlay(
                        Circle()
                            .stroke(
                                LinearGradient(
                                    colors: [.white.opacity(0.55), .white.opacity(0.05)],
                                    startPoint: .topLeading,
                                    endPoint: .bottomTrailing
                                ),
                                lineWidth: 1
                            )
                            .scaleEffect(x: lensSquashX, y: lensSquashY)
                            .scaleEffect(scale)
                            .opacity(0.8)
                    )
                    .shadow(color: currentTint.opacity(0.45), radius: 18, x: 0, y: 6)

                if showsRimLight && !reduceMotion {
                    Circle()
                        .stroke(rimColor, lineWidth: 2)
                        .frame(width: diameter, height: diameter)
                        .scaleEffect(x: lensSquashX, y: lensSquashY)
                        .scaleEffect(scale)
                        .blur(radius: 1.5)
                        .opacity(rimOpacity(phase: phase))
                }
            }
            .compositingGroup()
        }
    }

    // MARK: - Tool chip

    private func toolChip(label: String) -> some View {
        Text(label)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .glassEffect(.regular.tint(.purple.opacity(0.35)), in: .capsule)
            .glassEffectID("orb.toolChip", in: orbNamespace)
            .foregroundStyle(.white)
            .lineLimit(1)
    }

    // MARK: - Accessibility

    private var accessibilityLabel: String {
        switch state {
        case .idle: return "Voice idle"
        case .listening: return "Listening"
        case .transcribing: return "Transcribing"
        case .thinking(let tool):
            if let tool { return "Thinking, \(tool)" }
            return "Thinking"
        case .speaking: return "Speaking"
        case .bargeIn: return "Listening, interrupting"
        }
    }

    // MARK: - Geometry

    private var diameter: CGFloat {
        switch state {
        case .idle: return 24
        case .listening: return 64
        case .transcribing: return 64
        case .thinking: return 56
        case .speaking: return 96
        case .bargeIn: return 96 * 0.7
        }
    }

    private var containerSize: CGFloat { 200 }

    /// Lens squash is only applied while listening — gives the orb a
    /// horizontally-stretched "lens" silhouette. All other states keep a
    /// circle (squash factors at 1).
    private var lensSquashX: CGFloat {
        state == .listening ? 1.12 : 1.0
    }

    private var lensSquashY: CGFloat {
        state == .listening ? 0.86 : 1.0
    }

    private func currentScale(phase: TimeInterval) -> CGFloat {
        if reduceMotion { return 1.0 }
        switch state {
        case .speaking:
            // 2.4 s breath: 1.00 → 1.04 → 1.00.
            let p = (sin(phase / 2.4 * 2 * .pi) + 1) / 2
            return 1.0 + 0.04 * CGFloat(p)
        case .listening, .bargeIn:
            // Small pulse driven by the input RMS so the orb visibly
            // responds to the user's voice.
            let envelope = max(min(CGFloat(inputRMS) * 4, 0.18), 0)
            return 1.0 + envelope
        case .thinking:
            let p = (sin(phase / 1.6 * 2 * .pi) + 1) / 2
            return 0.99 + 0.02 * CGFloat(p)
        case .transcribing:
            let p = (sin(phase / 1.2 * 2 * .pi) + 1) / 2
            return 0.99 + 0.015 * CGFloat(p)
        case .idle:
            return 1.0
        }
    }

    // MARK: - Style

    private var currentTint: Color {
        switch state {
        case .idle: return .white.opacity(0.18)
        case .listening: return AppTheme.Tint.voiceListening
        case .transcribing:
            // Subtle iridescence — slow hue blend between the listening
            // blue and the artwork tint.
            return artworkTint.opacity(0.55)
        case .thinking: return AppTheme.Tint.voiceThinking
        case .speaking: return AppTheme.Tint.voiceSpeaking
        case .bargeIn: return AppTheme.Tint.voiceListening
        }
    }

    private var haloGradient: RadialGradient {
        RadialGradient(
            colors: [currentTint.opacity(0.55), .clear],
            center: .center,
            startRadius: 0,
            endRadius: diameter
        )
    }

    private func haloOpacity(phase: TimeInterval) -> Double {
        switch state {
        case .speaking:
            let p = (sin(phase / 2.4 * 2 * .pi) + 1) / 2
            return 0.35 + 0.35 * p
        case .listening:
            return 0.55 + 0.30 * Double(min(inputRMS * 4, 1.0))
        case .bargeIn: return 0.85
        case .thinking: return 0.55
        case .transcribing: return 0.45
        case .idle: return 0.15
        }
    }

    private var showsRimLight: Bool {
        switch state {
        case .bargeIn, .listening: return true
        default: return false
        }
    }

    private var rimColor: Color {
        state == .bargeIn ? .white : currentTint
    }

    private func rimOpacity(phase: TimeInterval) -> Double {
        switch state {
        case .bargeIn: return 0.9
        case .listening:
            let p = (sin(phase / 0.85 * 2 * .pi) + 1) / 2
            return 0.45 + 0.35 * p
        default: return 0
        }
    }

    // MARK: - Animation

    private var transitionAnimation: Animation {
        if reduceMotion { return .easeInOut(duration: 0.25) }
        switch state {
        case .bargeIn:
            return .spring(response: 0.25, dampingFraction: 0.85)
        case .speaking:
            return .spring(response: 0.6, dampingFraction: 0.7)
        case .thinking:
            return .spring(response: 0.5, dampingFraction: 0.9)
        case .listening:
            return .spring(response: 0.4, dampingFraction: 0.75)
        case .transcribing, .idle:
            return .spring(response: 0.55, dampingFraction: 0.9)
        }
    }
}

// MARK: - Previews

#Preview("Idle") {
    VoiceOrbView(state: .idle)
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.black)
}

#Preview("Listening") {
    VoiceOrbView(state: .listening, inputRMS: 0.05)
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.black)
}

#Preview("Thinking + chip") {
    VoiceOrbView(state: .thinking(toolName: "Searching transcripts"))
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.black)
}

#Preview("Speaking") {
    VoiceOrbView(state: .speaking)
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.black)
}

#Preview("Barge-in") {
    VoiceOrbView(state: .bargeIn, inputRMS: 0.2)
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.black)
}
