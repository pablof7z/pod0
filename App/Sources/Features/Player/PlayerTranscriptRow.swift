import SwiftUI

// USAGE:
// Internal-only renderer used by clip composer (`ClipComposerSheet`) and
// quote share (`QuoteShareView`) to display transcript segments inside their
// own surfaces. NOT a primary player view — transcripts are an extraction
// substrate, not user-visible content. The player surfaces chapters via
// `PlayerChaptersScrollView`.

// MARK: - PlayerTranscriptRow

/// Single transcript line as it appears inside the dark-chrome player surface.
///
/// Visual priorities differ from `TranscriptReaderView`'s row:
///   - Light text on a dark wallpaper (white at varying opacity, not
///     `.primary`).
///   - Active line gets a gentle white underlay rather than the editorial
///     yellow tint — yellow over the player wallpaper would clash with the
///     hero artwork.
///   - Speaker label sits inline as a small uppercase chip above the line,
///     so consecutive same-speaker rows stay visually packed without a
///     full paragraph re-render.
struct PlayerTranscriptRow: View {

    let segment: Segment
    let speaker: Speaker?
    let isActive: Bool
    let onTap: () -> Void
    /// Long-press → "Ask the agent about this moment". Optional so existing
    /// callers that don't yet wire the agent (previews, fixtures) keep
    /// compiling without a no-op stub.
    var onAskAgent: (() -> Void)? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            if let speakerName {
                Text(speakerName.uppercased())
                    .font(.system(.caption2, design: .rounded).weight(.semibold))
                    .tracking(0.6)
                    .foregroundStyle(isActive ? Color.primary : Color.secondary)
            }
            Text(segment.text)
                .font(.system(.body))
                .lineSpacing(6)
                .foregroundStyle(isActive ? Color.primary : Color.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(
            // Active row uses the system accent at low opacity instead
            // of hardcoded white — the whole player no longer assumes a
            // dark chrome background, so white-on-anything broke in
            // light mode.
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(isActive ? Color.accentColor.opacity(0.12) : Color.clear)
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .stroke(isActive ? Color.accentColor.opacity(0.20) : Color.clear, lineWidth: 0.5)
                )
        )
        .contentShape(Rectangle())
        .onTapGesture(perform: onTap)
        .contextMenu {
            if let onAskAgent {
                Button {
                    Haptics.selection()
                    onAskAgent()
                } label: {
                    Label("Ask the agent about this", systemImage: "sparkles")
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityText)
        .accessibilityAddTraits(.isButton)
        .accessibilityAction(named: Text("Ask the agent")) {
            onAskAgent?()
        }
    }

    /// Display name with a graceful fallback to the raw label so we never
    /// show "Unknown" — the source label is at least a stable identifier.
    private var speakerName: String? {
        guard let speaker else { return nil }
        return speaker.displayName ?? speaker.label
    }

    private var accessibilityText: String {
        if let speakerName {
            return "\(speakerName), \(segment.text)"
        }
        return segment.text
    }
}
