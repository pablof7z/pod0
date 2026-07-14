import SwiftUI

// MARK: - PlayerNoChaptersPlaceholder
//
// Minimal stand-in for the secondary surface when an episode has no chapters
// yet. The transcript is never rendered as a primary surface (it's an
// internal extraction substrate); this placeholder communicates the
// lifecycle the user is in — transcript ingesting, AI chapters compiling,
// or simply no chapters available — without showing transcript text.
//
// Renders with a `minHeight` so it occupies useful real estate inside the
// player's vertical ScrollView even with no intrinsic content height — a
// `maxHeight: .infinity` here would collapse to zero because the parent
// scroll axis is unbounded.

struct PlayerNoChaptersPlaceholder: View {
    let episode: Episode?

    var body: some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: glyph)
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(.secondary)
                .symbolEffect(.pulse, options: .repeating, isActive: isWorking)
            Text(headline)
                .font(AppTheme.Typography.headline)
                .foregroundStyle(.primary)
                .multilineTextAlignment(.center)
            Text(subhead)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, AppTheme.Spacing.lg)
        }
        .frame(maxWidth: .infinity, minHeight: 280)
        .padding(AppTheme.Spacing.lg)
        .background(cardBackground)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(headline). \(subhead)")
    }

    // MARK: - Copy

    /// Glyph mirrors the lifecycle phase. `waveform` while we're working
    /// (transcript fetching / transcribing / AI chapters compiling); the
    /// generic "no marks" icon otherwise.
    private var glyph: String {
        guard let episode else { return "list.bullet.indent" }
        switch episode.transcriptState {
        case .queued, .fetchingPublisher, .transcribing:
            return "waveform"
        case .ready:
            // Transcript is ready but no chapters yet — AI chapter compile
            // is either in flight or the compile produced nothing usable.
            return "sparkles"
        case .failed, .none:
            return "list.bullet.indent"
        }
    }

    private var isWorking: Bool {
        guard let episode else { return false }
        switch episode.transcriptState {
        case .queued, .fetchingPublisher, .transcribing, .ready:
            return true
        case .failed, .none:
            return false
        }
    }

    private var headline: String {
        guard let episode else { return "No chapters" }
        switch episode.transcriptState {
        case .queued, .fetchingPublisher:
            return "Preparing chapters"
        case .transcribing:
            return "Preparing chapters"
        case .ready:
            return "Compiling chapters"
        case .failed, .none:
            return "No chapters yet"
        }
    }

    private var subhead: String {
        guard let episode else { return "Use the scrubber to navigate this episode." }
        switch episode.transcriptState {
        case .queued, .fetchingPublisher:
            return "We're fetching the transcript that powers AI chapters."
        case .transcribing(let p):
            let pct = Int((p * 100).rounded())
            return pct > 0
                ? "Transcribing — \(pct)% complete. Chapters will appear here."
                : "Transcribing… Chapters will appear here."
        case .ready:
            return "AI chapters are compiling. Use the scrubber until they arrive."
        case .failed:
            return "Transcript ingestion failed. Use the scrubber to navigate."
        case .none:
            return "This episode has no published chapters. Use the scrubber to navigate."
        }
    }

    // MARK: - Background

    @ViewBuilder
    private var cardBackground: some View {
        RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
            .fill(.ultraThinMaterial)
            .overlay(
                RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                    .stroke(Color.primary.opacity(0.06), lineWidth: 0.5)
            )
    }
}
