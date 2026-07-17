import SwiftUI

// MARK: - TranscribingInProgressView

/// Empty / in-progress state for the transcript surface.
///
/// Shown for any non-`.ready` `transcriptState`. The view inspects the state
/// and chooses an appropriate copy + indicator (idle, queued, fetching
/// publisher, mid-Scribe progress, or failed). The "Request transcript" CTA
/// fires a `TranscriptIngestService.ingest` for the episode when the state is
/// idle or has previously failed; while a request is mid-flight it disables
/// itself so the user can't double-tap.
struct TranscribingInProgressView: View {
    let episode: Episode
    @Environment(AppStateStore.self) private var store

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                header
                Divider()
                    .background(Color.secondary.opacity(0.2))
                    .padding(.horizontal, AppTheme.Spacing.md)
                copyBlock
                cta
            }
            .padding(.vertical, AppTheme.Spacing.xl)
        }
        .background(Color(.systemBackground).ignoresSafeArea())
        .navigationTitle("Transcript")
    }

    // MARK: - Subviews

    private var header: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            if activeJob?.state.isActive == true { ProgressView() }
            Text(jobStatusLabel)
                .font(.system(.subheadline, design: .rounded).weight(.medium))
                .foregroundStyle(activeJob?.state == .failedPermanent ? AppTheme.Tint.warning : .secondary)
        }
        .padding(.horizontal, AppTheme.Spacing.md)
    }

    private var copyBlock: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text(primaryCopy)
                .font(AppTheme.Typography.title3)
                .foregroundStyle(.primary)
            Text(secondaryCopy)
                .font(AppTheme.Typography.callout)
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, AppTheme.Spacing.md)
    }

    private var primaryCopy: String {
        if case .ready = episode.transcriptState { return "Transcript ready." }
        if activeJob?.state == .failedPermanent || activeJob?.state == .blocked {
            return "Transcription needs attention."
        }
        return activeJob?.state.isActive == true ? "Preparing this transcript." : "No transcript yet."
    }

    private var secondaryCopy: String {
        if let message = activeJob?.lastErrorMessage { return message }
        if activeJob?.state.isActive == true {
            return "The text will appear here when it's ready. Keep listening — this runs in the background."
        }
        return "Fetch one below. We'll use the publisher's transcript when available, or your configured transcription provider if no publisher transcript exists."
    }

    private var cta: some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            Button {
                let episodeID = episode.id
                store.setEpisodeTranscriptState(episodeID, state: .none)
                WorkflowRuntime.shared.requestTranscript(episodeID: episodeID)
            } label: {
                Text("Request transcript")
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
            }
            .buttonStyle(.borderedProminent)
            .disabled(!isRequestable)
            .padding(.horizontal, AppTheme.Spacing.md)

            Text(footerLabel)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var isRequestable: Bool {
        Self.canRequestTranscript(for: episode.transcriptState)
    }

    /// Every non-ready state stays manually repairable. The durable job store
    /// deduplicates repeated taps, including after a process died mid-step.
    nonisolated static func canRequestTranscript(for state: TranscriptState) -> Bool {
        switch state {
        case .none: return true
        case .ready: return false
        }
    }

    private var activeJob: WorkJob? {
        guard let jobs = try? WorkflowRuntime.shared.jobStore?.allJobs() else { return nil }
        return jobs.last {
            $0.kind == .transcriptIngest && $0.subjectID == episode.id
        }
    }

    private var jobStatusLabel: String {
        guard let job = activeJob else { return "Not requested" }
        switch job.state {
        case .pending, .leased: return "Queued for transcription"
        case .running: return "Transcribing"
        case .retryScheduled: return "Retry scheduled"
        case .blocked: return "Waiting for setup"
        case .failedPermanent: return "Transcription failed"
        case .cancelled: return "Cancelled"
        case .obsolete: return "Superseded"
        case .succeeded: return "Transcript ready"
        }
    }

    private var footerLabel: String {
        if episode.publisherTranscriptURL != nil {
            return "Publisher transcript available"
        }
        return "Configure your transcription provider in Settings → Intelligence → Models → Speech"
    }
}

// MARK: - Preview

#Preview("Idle") {
    let subID = UUID()
    let episode = Episode(
        podcastID: subID,
        guid: "preview-1",
        title: "How to Think About Keto",
        pubDate: Date(),
        enclosureURL: URL(string: "https://traffic.megaphone.fm/HSW1234567890.mp3")!,
        transcriptState: .none
    )
    return NavigationStack { TranscribingInProgressView(episode: episode) }
        .environment(AppStateStore())
}
