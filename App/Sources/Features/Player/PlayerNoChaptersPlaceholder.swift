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
    @Environment(WorkflowClient.self) private var workflows

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
        .workflowProjectionScope(
            subjectIDs: episode.map { [$0.id] } ?? [],
            kinds: [.transcriptIngest, .chapterArtifacts]
        )
    }

    // MARK: - Copy

    /// Glyph mirrors the lifecycle phase. `waveform` while we're working
    /// (transcript fetching / transcribing / AI chapters compiling); the
    /// generic "no marks" icon otherwise.
    private var glyph: String {
        guard let episode else { return "list.bullet.indent" }
        if workflowJob(for: episode)?.kind == .transcriptIngest { return "waveform" }
        if case .ready = episode.transcriptState { return "sparkles" }
        return "list.bullet.indent"
    }

    private var isWorking: Bool {
        guard let episode else { return false }
        return workflowJob(for: episode)?.state.isActive == true
    }

    private var headline: String {
        guard let episode else { return "No chapters" }
        if let job = workflowJob(for: episode), job.state.isActive {
            return job.kind == .chapterArtifacts ? "Compiling chapters" : "Preparing chapters"
        }
        if case .ready = episode.transcriptState { return "No chapters yet" }
        return "No chapters yet"
    }

    private var subhead: String {
        guard let episode else { return "Use the scrubber to navigate this episode." }
        if let job = workflowJob(for: episode) {
            if job.state == .blocked || job.state == .failedPermanent {
                return job.lastErrorMessage ?? "Chapter preparation needs attention."
            }
            if job.state.isActive {
                return job.kind == .chapterArtifacts
                    ? "AI chapters are compiling. Use the scrubber until they arrive."
                    : "We're preparing the transcript that powers AI chapters."
            }
        }
        return "This episode has no published chapters. Use the scrubber to navigate."
    }

    private func workflowJob(for episode: Episode) -> WorkflowJobProjection? {
        let transcript = workflows.latest(kind: .transcriptIngest, subjectID: episode.id)
        let chapters = workflows.latest(kind: .chapterArtifacts, subjectID: episode.id)
        return [transcript, chapters].compactMap { $0 }.max { $0.updatedAt < $1.updatedAt }
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
