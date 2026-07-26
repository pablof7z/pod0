import SwiftUI

// MARK: - ClipTranscriptTurnView

/// One speaker turn in the clip reader.
///
/// The four states differ only in weight of presence — nothing is labelled and
/// nothing is boxed. A clip is legible while the conversation around it
/// recedes, which is what a highlight actually is on a page.
struct ClipTranscriptTurnView: View {

    let turn: ClipTranscriptTurn
    let onOpenNote: (UUID) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            if let speakerName = turn.speakerName {
                Text(speakerName)
                    .font(.system(.caption2, design: .default).weight(.medium))
                    .tracking(1.1)
                    .textCase(.uppercase)
                    .foregroundStyle(.primary.opacity(speakerOpacity))
            }
            Text(turn.text)
                .font(.system(size: 19, weight: .semibold))
                .lineSpacing(3)
                .foregroundStyle(.primary.opacity(textOpacity))
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.leading, turn.clipID == nil ? 0 : AppTheme.Spacing.md)
        .padding(.vertical, turn.presence == .focus ? AppTheme.Spacing.sm : 0)
        .background(alignment: .topLeading) { marginalDot }
        .background(focusGround)
        .contentShape(Rectangle())
        .onTapGesture {
            guard let clipID = turn.clipID else { return }
            Haptics.selection()
            onOpenNote(clipID)
        }
        .highPriorityGesture(pullUp, including: turn.clipID == nil ? .subviews : .all)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(turn.clipID == nil ? [] : .isButton)
        .accessibilityHint(turn.clipID == nil ? "" : "Opens your note on this clip")
    }

    // MARK: - Gesture

    /// Pull up on the passage to raise the note. Tapping does the same thing —
    /// deliberately *not* seeking, because playing is a separate act.
    private var pullUp: some Gesture {
        DragGesture(minimumDistance: 18)
            .onEnded { value in
                guard let clipID = turn.clipID else { return }
                guard value.translation.height < -18 else { return }
                Haptics.selection()
                onOpenNote(clipID)
            }
    }

    // MARK: - Marks

    /// The dot a book reader would put in the margin. Present only when the
    /// clip has been written on — an unwritten clip carries nothing, because
    /// it is complete rather than incomplete.
    @ViewBuilder
    private var marginalDot: some View {
        if turn.isAnnotated {
            Circle()
                .fill(Color.accentColor)
                .frame(width: 5, height: 5)
                .opacity(turn.presence == .focus ? 1 : 0.5)
                .padding(.top, turn.speakerName == nil ? 9 : 22)
                .accessibilityHidden(true)
        }
    }

    @ViewBuilder
    private var focusGround: some View {
        if turn.presence == .focus {
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color.accentColor.opacity(0.07))
                .padding(.horizontal, -AppTheme.Spacing.sm)
        }
    }

    // MARK: - Presence

    private var textOpacity: Double {
        switch turn.presence {
        case .focus:   return 1
        case .mark:    return 0.82
        case .context: return 0.32
        }
    }

    private var speakerOpacity: Double {
        switch turn.presence {
        case .focus:   return 0.42
        case .mark:    return 0.34
        case .context: return 0.18
        }
    }
}

// MARK: - ClipTranscriptFoldView

/// A collapsed run of the conversation, drawn as the ragged silhouette of
/// paragraphs with no words. No glyph, no count, no "show more" — tapping
/// opens it in place.
struct ClipTranscriptFoldView: View {

    let weight: Int
    let onExpand: () -> Void

    private static let widths: [CGFloat] = [0.88, 0.96, 0.62, 0.80]

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            ForEach(0..<max(2, min(weight, 4)), id: \.self) { line in
                GeometryReader { proxy in
                    Capsule()
                        .fill(.primary.opacity(0.13))
                        .frame(width: proxy.size.width * Self.widths[line % Self.widths.count])
                        .opacity(1 - (Double(line) * 0.22))
                }
                .frame(height: 3)
            }
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .onTapGesture {
            Haptics.selection()
            onExpand()
        }
        .accessibilityElement()
        .accessibilityLabel("Hidden conversation")
        .accessibilityHint("Shows the part of the episode between your clips")
        .accessibilityAddTraits(.isButton)
    }
}
