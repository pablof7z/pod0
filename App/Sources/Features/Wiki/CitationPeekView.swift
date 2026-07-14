import SwiftUI

// MARK: - Citation peek

/// Liquid Glass peek-from-below sheet shown when the user taps a citation
/// chip on a wiki page. Mirrors UX-04 §6c.
///
/// In lane 7 the "play this clip" affordance is a stub — it dispatches a
/// notification the player surface will pick up once Lane 4's deep-link
/// router is in place. The button is rendered fully so the UI flow is
/// reviewable end-to-end.
struct CitationPeekView: View {

    let citation: WikiCitation
    /// Optional commit handler — called when the user taps "Play clip"
    /// to commit to the full clip instead of letting the peek's
    /// auto-restore kick in on dismiss. The host (`CitationPeekSheet`)
    /// supplies a closure that suppresses the restore + dismisses.
    /// `nil` means the button posts the legacy notification fallback.
    var onCommitFullClip: (() -> Void)? = nil

    /// Legacy notification name — kept for backward compatibility with
    /// surfaces that wrap the view directly without the sheet (e.g.
    /// previews, ad-hoc embeds). New callers should pass
    /// `onCommitFullClip` to the initializer instead.
    static let playClipNotification = Notification.Name("podcastr.wiki.playClip")

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            header
            quote
            actions
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 22, style: .continuous)
                .fill(Color.clear)
                .glassEffect(.regular.interactive(), in: .rect(cornerRadius: 22))
        )
        .padding(.horizontal, 12)
        .padding(.bottom, 12)
        .accessibilityElement(children: .contain)
    }

    // MARK: - Subviews

    private var header: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                if let speaker = citation.speaker {
                    Text(speaker)
                        .font(.headline)
                        .foregroundStyle(.primary)
                } else {
                    Text("Cited span")
                        .font(.headline)
                        .foregroundStyle(.secondary)
                }
                Text("\(citation.formattedTimestamp) → +\(durationLabel)")
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.tertiary)
            }
            Spacer()
            confidencePill
        }
    }

    private var quote: some View {
        Text("\u{201C}\(citation.quoteSnippet)\u{201D}")
            .font(AppTheme.Typography.body)
            .italic()
            .foregroundStyle(.primary)
            .lineSpacing(4)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var actions: some View {
        HStack(spacing: 12) {
            Button {
                if let onCommitFullClip {
                    onCommitFullClip()
                } else {
                    NotificationCenter.default.post(
                        name: CitationPeekView.playClipNotification,
                        object: nil,
                        userInfo: [
                            "episodeID": citation.episodeID.uuidString,
                            "startMS": citation.startMS,
                            "endMS": citation.endMS,
                        ]
                    )
                }
            } label: {
                Label("Play clip", systemImage: "play.fill")
                    .font(.headline)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
            }
            .buttonStyle(.borderedProminent)
            .accessibilityLabel("Play cited clip at \(citation.formattedTimestamp)")

            Button {
                UIPasteboard.general.string = citation.quoteSnippet
            } label: {
                Label("Quote", systemImage: "quote.opening")
                    .font(.subheadline)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
            }
            .buttonStyle(.bordered)

            Spacer()
        }
    }

    // MARK: - Helpers

    private var durationLabel: String {
        let seconds = Double(citation.durationMS) / 1_000.0
        return String(format: "%.0fs", max(1, seconds))
    }

    private var confidencePill: some View {
        Text(citation.verificationConfidence.label)
            .font(.caption2.weight(.semibold))
            .textCase(.uppercase)
            .tracking(0.5)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(
                Capsule().fill(confidenceColor.opacity(0.18))
            )
            .foregroundStyle(confidenceColor)
            .accessibilityLabel("Confidence: \(citation.verificationConfidence.accessibilityValue)")
    }

    private var confidenceColor: Color {
        switch citation.verificationConfidence {
        case .high: AppTheme.Tint.evidenceHigh
        case .medium: AppTheme.Tint.evidenceMedium
        case .low: AppTheme.Tint.evidenceLow
        }
    }
}
