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

    @ViewBuilder
    private var header: some View {
        switch episode.transcriptState {
        case .transcribing(let progress):
            HStack(spacing: AppTheme.Spacing.md) {
                ProgressView(value: max(0, min(progress, 1)))
                    .progressViewStyle(.linear)
                    .tint(AppTheme.Tint.warning)
                    .frame(maxWidth: .infinity)
                Text("\(Int((progress * 100).rounded()))%")
                    .font(.system(.subheadline, design: .monospaced).weight(.medium))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            .padding(.horizontal, AppTheme.Spacing.md)
        case .queued, .fetchingPublisher:
            HStack(spacing: AppTheme.Spacing.sm) {
                ProgressView()
                Text(headerLabel)
                    .font(.system(.subheadline, design: .rounded).weight(.medium))
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
        case .failed(let message):
            Label(message, systemImage: "exclamationmark.triangle.fill")
                .font(.system(.subheadline, design: .rounded).weight(.medium))
                .foregroundStyle(AppTheme.Tint.warning)
                .padding(.horizontal, AppTheme.Spacing.md)
        case .none, .ready:
            EmptyView()
        }
    }

    private var headerLabel: String {
        switch episode.transcriptState {
        case .queued: return "Queued for transcription"
        case .fetchingPublisher: return "Fetching publisher transcript"
        default: return ""
        }
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
        switch episode.transcriptState {
        case .none: return "No transcript yet."
        case .queued: return "Queued for transcription."
        case .fetchingPublisher: return "Fetching the publisher's transcript."
        case .transcribing: return "Transcribing this episode."
        case .failed: return "Transcription didn't finish."
        case .ready: return "Transcript ready."
        }
    }

    private var secondaryCopy: String {
        switch episode.transcriptState {
        case .none:
            return "Fetch one below. We'll use the publisher's transcript when available, or your configured transcription provider if no publisher transcript exists."
        case .queued, .fetchingPublisher, .transcribing:
            return "The text will appear here when it's ready. Keep listening — this runs in the background."
        case .failed(let message):
            return message
        case .ready:
            return ""
        }
    }

    private var cta: some View {
        VStack(spacing: AppTheme.Spacing.sm) {
            Button {
                let episodeID = episode.id
                Task { await TranscriptIngestService.shared.ingest(episodeID: episodeID) }
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

    /// True when a fresh ingest call would actually do something. While the
    /// pipeline is mid-flight (`queued`, `fetchingPublisher`, `transcribing`)
    /// we disable so the user can't pile redundant submits onto the in-flight
    /// dedup set in `TranscriptIngestService`.
    private var isRequestable: Bool {
        switch episode.transcriptState {
        case .none, .failed: return true
        case .queued, .fetchingPublisher, .transcribing, .ready: return false
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
}

#Preview("Transcribing") {
    let subID = UUID()
    let episode = Episode(
        podcastID: subID,
        guid: "preview-2",
        title: "How to Think About Keto",
        pubDate: Date(),
        enclosureURL: URL(string: "https://traffic.megaphone.fm/HSW1234567890.mp3")!,
        transcriptState: .transcribing(progress: 0.42)
    )
    return NavigationStack { TranscribingInProgressView(episode: episode) }
}
