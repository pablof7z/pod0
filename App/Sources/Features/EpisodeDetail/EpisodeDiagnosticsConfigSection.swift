import SwiftUI

// MARK: - EpisodeDiagnosticsConfigSection
//
// The "Pipeline configuration" block of the Episode Diagnostics sheet. The
// event log says what *happened*; this says what *will* (or won't) happen, and
// with which services — answering the other half of "why doesn't this episode
// have a transcript / chapters?" before the user has to read a single event.
//
// The transcript verdict comes from Rust's transcript ingest planner. Swift
// renders the returned semantics, but does not mirror provider fallback,
// per-show opt-out, publisher/AI fallback, key, or local-file policy.
struct EpisodeDiagnosticsConfigSection: View {

    let episode: Episode
    @Environment(AppStateStore.self) private var store

    var body: some View {
        Section {
            transcriptionVerdictRow
            ForEach(transcriptionDetails, id: \.0) { label, value in
                LabeledContent(label) {
                    Text(value).foregroundStyle(.secondary)
                        .multilineTextAlignment(.trailing)
                }
            }
            Divider()
            chaptersRow
            searchIndexRow
        } header: {
            Text("Pipeline configuration")
        } footer: {
            Text("What the pipeline is set up to do for this episode, with the services it would use. Change these in Settings → Transcripts and Settings → Models.")
        }
    }

    // MARK: - Transcription

    /// The single most important line: a plain-language verdict of what will
    /// happen to this episode's transcript, color-coded by whether it's a
    /// go (info), a wait (info), or a dead end the user must act on (warning).
    private var transcriptionVerdictRow: some View {
        let v = transcriptionVerdict
        return HStack(alignment: .top, spacing: 10) {
            Image(systemName: v.icon)
                .foregroundStyle(v.tint)
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 2) {
                Text("Transcription")
                    .font(.subheadline.weight(.semibold))
                Text(v.message)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.vertical, 2)
    }

    private struct Verdict {
        let message: String
        let icon: String
        let tint: Color
    }

    private var snapshot: SettingsSnapshot? { store.kernel?.podcastSnapshot?.settings }

    private var transcriptionPlan: KernelModel.TranscriptIngestPlan? {
        store.kernel?.transcriptIngestPlan(
            episodeID: episode.id,
            forceProvider: nil,
            localAudioAvailable: isDownloaded,
            allowPublisher: true
        )
    }

    private var isDownloaded: Bool {
        if case .downloaded = episode.downloadState { return true }
        return false
    }

    /// Render Rust's transcript planner verdict. The planner owns the policy;
    /// this view only maps semantic statuses to UI copy and color.
    private var transcriptionVerdict: Verdict {
        guard let plan = transcriptionPlan else {
            return Verdict(
                message: "Transcript planner is not available yet.",
                icon: "hourglass",
                tint: .secondary)
        }
        switch plan.status {
        case "ready":
            if case .ready(let source) = episode.transcriptState {
                let label = TranscriptIngestService.sourceDisplayName(source, kernel: store.kernel) ?? source.rawValue
                return Verdict(
                    message: "Already transcribed via \(label).",
                    icon: "checkmark.seal.fill",
                    tint: AppTheme.Tint.success)
            }
            return Verdict(
                message: "Already transcribed.",
                icon: "checkmark.seal.fill",
                tint: AppTheme.Tint.success)
        case "publisher":
            return Verdict(
                message: "Will use the publisher-supplied transcript from the feed.",
                icon: "doc.text.magnifyingglass",
                tint: AppTheme.Tint.success)
        case "stt":
            let provider = providerDisplayName(plan.provider)
            if plan.requiresLocalFile && !isDownloaded {
                return Verdict(
                    message: "Will transcribe on-device with \(provider) once the episode finishes downloading.",
                    icon: "arrow.down.circle",
                    tint: .secondary)
            }
            return Verdict(
                message: "Will transcribe with \(provider).",
                icon: "waveform.badge.magnifyingglass",
                tint: AppTheme.Tint.success)
        case "skipped":
            return Verdict(
                message: plan.reason ?? "Transcription is not configured for this episode.",
                icon: "exclamationmark.triangle.fill",
                tint: AppTheme.Tint.warning)
        default:
            return Verdict(
                message: plan.reason ?? "Transcript planning failed.",
                icon: "exclamationmark.triangle.fill",
                tint: AppTheme.Tint.warning)
        }
    }

    private var transcriptionDetails: [(String, String)] {
        var rows: [(String, String)] = []
        if let plan = transcriptionPlan {
            rows.append(("Rust plan", plan.status))
            if let reason = plan.reason {
                rows.append(("Plan reason", reason))
            }
            if let provider = plan.provider {
                rows.append(("Planned provider", providerDisplayName(provider)))
            }
            if plan.requiresLocalFile {
                rows.append(("Local audio", isDownloaded ? "Available" : "Required"))
            }
        } else {
            rows.append(("Rust plan", "Unavailable"))
        }
        if let snap = snapshot {
            rows.append(("Selected provider", providerDisplayName(snap.selectedSTTProvider.rawValue)))
            if snap.resolvedSTTProvider != snap.selectedSTTProvider {
                rows.append(("Effective provider", providerDisplayName(snap.resolvedSTTProvider.rawValue)))
            }
        }
        rows.append((
            "Publisher transcript",
            episode.publisherTranscriptURL != nil ? "Available in feed" : "None in feed"))
        return rows
    }

    private func providerDisplayName(_ raw: String?) -> String {
        guard let raw else { return "the selected provider" }
        guard let provider = STTProvider(rawValue: raw) else { return raw }
        return TranscriptIngestService.providerDisplayName(provider, kernel: store.kernel) ?? raw
    }

    // MARK: - Chapters + search

    private var chaptersRow: some View {
        let modelName = snapshot?.chapterCompilationModelName ?? "DeepSeek Flash"
        return LabeledContent("AI chapters") {
            VStack(alignment: .trailing, spacing: 1) {
                Text(modelName).foregroundStyle(.secondary)
                Text(hasPublisherChapters
                    ? "Publisher chapters present — AI skipped"
                    : "Runs after transcription")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private var hasPublisherChapters: Bool {
        guard let chapters = episode.chapters else { return false }
        return !chapters.isEmpty
    }

    private var searchIndexRow: some View {
        let modelName = snapshot?.embeddingsModelName ?? "DeepSeek Flash"
        return LabeledContent("Search indexing") {
            VStack(alignment: .trailing, spacing: 1) {
                Text(modelName).foregroundStyle(.secondary)
                Text("Embeds the transcript for agent search")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
    }
}
