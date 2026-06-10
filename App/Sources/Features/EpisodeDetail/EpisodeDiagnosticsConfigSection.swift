import SwiftUI

// MARK: - EpisodeDiagnosticsConfigSection
//
// The "Pipeline configuration" block of the Episode Diagnostics sheet. The
// event log says what *happened*; this says what *will* (or won't) happen, and
// with which services — answering the other half of "why doesn't this episode
// have a transcript / chapters?" before the user has to read a single event.
//
// Everything here is derived live from `settings` + the kernel snapshot — no
// kernel round-trip — so it always reflects the current configuration. When
// nothing is configured to run, that is stated plainly (the whole point: "if
// nothing is configured it should be said there").
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

    private var isDownloaded: Bool {
        if case .downloaded = episode.downloadState { return true }
        return false
    }

    /// Decide, from the live config, what this episode's transcript pipeline
    /// will actually do. Mirrors the gating order in `TranscriptIngestService`.
    private var transcriptionVerdict: Verdict {
        // Already done.
        if case .ready(let source) = episode.transcriptState {
            return Verdict(
                message: "Already transcribed via \(TranscriptIngestService.sourceDisplayName(source)).",
                icon: "checkmark.seal.fill",
                tint: AppTheme.Tint.success)
        }
        // Category opt-out wins over everything.
        if !store.effectiveTranscriptionEnabled(forPodcast: episode.podcastID) {
            return Verdict(
                message: "Won't transcribe — transcription is turned off for this show's category.",
                icon: "minus.circle",
                tint: AppTheme.Tint.warning)
        }
        let settings = store.state.settings
        // Publisher transcript path.
        if episode.publisherTranscriptURL != nil {
            if settings.autoIngestPublisherTranscripts {
                return Verdict(
                    message: "Will use the publisher-supplied transcript from the feed.",
                    icon: "doc.text.magnifyingglass",
                    tint: AppTheme.Tint.success)
            }
            // Publisher transcript exists but auto-ingest is off; fall through
            // to the AI verdict, which may still cover it.
        }
        // AI fallback path.
        guard settings.autoFallbackToScribe else {
            return Verdict(
                message: "Won't transcribe automatically — AI transcription fallback is OFF (turn it on in Settings → Transcripts).",
                icon: "exclamationmark.triangle.fill",
                tint: AppTheme.Tint.warning)
        }
        guard let snap = snapshot else {
            return Verdict(
                message: "AI transcription is on; the provider will resolve once the kernel is ready.",
                icon: "hourglass",
                tint: .secondary)
        }
        let resolved = snap.resolvedSTTProvider
        let selected = snap.selectedSTTProvider
        // Selected provider downgraded for a missing key.
        if selected != resolved {
            return Verdict(
                message: "\(selected.displayName) needs a key; will use \(resolved.displayName) instead (connect \(selected.displayName) in Settings → Providers).",
                icon: "key.slash",
                tint: AppTheme.Tint.warning)
        }
        // Resolved provider requires a key it doesn't have.
        if snap.effectiveSttProviderRequiresKey && !snap.hasLoadedKey(for: resolved) {
            return Verdict(
                message: "Won't transcribe — \(resolved.displayName) needs an API key (connect it in Settings → Providers).",
                icon: "exclamationmark.triangle.fill",
                tint: AppTheme.Tint.warning)
        }
        // Apple on-device needs the file first.
        if resolved == .appleNative && !isDownloaded {
            return Verdict(
                message: "Will transcribe on-device with \(resolved.displayName) once the episode finishes downloading.",
                icon: "arrow.down.circle",
                tint: .secondary)
        }
        return Verdict(
            message: "Will transcribe with \(resolved.displayName).",
            icon: "waveform.badge.magnifyingglass",
            tint: AppTheme.Tint.success)
    }

    private var transcriptionDetails: [(String, String)] {
        let settings = store.state.settings
        var rows: [(String, String)] = []
        rows.append(("AI fallback", settings.autoFallbackToScribe ? "On" : "Off"))
        rows.append(("Publisher auto-ingest", settings.autoIngestPublisherTranscripts ? "On" : "Off"))
        if let snap = snapshot {
            rows.append(("Selected provider", snap.selectedSTTProvider.displayName))
            if snap.resolvedSTTProvider != snap.selectedSTTProvider {
                rows.append(("Effective provider", snap.resolvedSTTProvider.displayName))
            }
            let needsKey = snap.effectiveSttProviderRequiresKey
            let hasKey = snap.hasLoadedKey(for: snap.resolvedSTTProvider)
            rows.append(("Provider key", needsKey ? (hasKey ? "Configured" : "Missing") : "Not required"))
        }
        rows.append((
            "Category transcription",
            store.effectiveTranscriptionEnabled(forPodcast: episode.podcastID) ? "Enabled" : "Disabled"))
        rows.append((
            "Publisher transcript",
            episode.publisherTranscriptURL != nil ? "Available in feed" : "None in feed"))
        return rows
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
