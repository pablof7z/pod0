import SwiftUI

// MARK: - EpisodeAuditLogView

/// "Diagnostics" sheet for a single episode. Answers the user's question:
/// *why doesn't this episode have a transcript / why didn't the download work?*
///
/// Renders the full audit log (download + transcript pipeline decisions, both
/// attempts and silent skips) in reverse-chronological order. Each row
/// summarises the event; tapping reveals every captured detail (URLs, HTTP
/// statuses, error strings, audio source, etc.).
///
/// Two retry affordances at the top:
///   - "Retry transcription" pushes a fresh `TranscriptIngestService.ingest`
///     so the user can watch new events stream in.
///   - "Retry download" kicks the download service for failed / missing files.
struct EpisodeAuditLogView: View {

    let episode: Episode

    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss

    @State private var auditStore = EpisodeAuditLogStore.shared
    @State private var expandedEventIDs: Set<UUID> = []

    private var events: [EpisodeAuditEvent] {
        auditStore.eventsNewestFirst(for: episode.id)
    }

    var body: some View {
        NavigationStack {
            List {
                summarySection
                actionsSection
                eventsSection
                metadataSection
            }
            .listStyle(.insetGrouped)
            .navigationTitle("Diagnostics")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
                ToolbarItem(placement: .topBarLeading) {
                    Menu {
                        Button(role: .destructive) {
                            EpisodeAuditLogStore.shared.clear(episodeID: episode.id)
                        } label: {
                            Label("Clear log", systemImage: "trash")
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
            }
        }
    }

    // MARK: - Sections

    private var summarySection: some View {
        Section {
            LabeledContent("Title") {
                Text(episode.title)
                    .multilineTextAlignment(.trailing)
                    .foregroundStyle(.secondary)
            }
            LabeledContent("Download") {
                Text(downloadStateSummary)
                    .foregroundStyle(.secondary)
            }
            LabeledContent("Transcript") {
                Text(transcriptStateSummary)
                    .foregroundStyle(.secondary)
            }
            if let url = episode.publisherTranscriptURL {
                LabeledContent("Publisher transcript") {
                    Text(url.host ?? url.absoluteString)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
            } else {
                LabeledContent("Publisher transcript") {
                    Text("none in feed")
                        .foregroundStyle(.secondary)
                }
            }
        } header: {
            Text("Current state")
        }
    }

    private var actionsSection: some View {
        Section {
            Button {
                retryTranscription(forceProvider: nil)
            } label: {
                Label("Retry transcription", systemImage: "arrow.clockwise")
            }
            if !availableRetryProviders.isEmpty {
                Menu {
                    ForEach(availableRetryProviders, id: \.self) { provider in
                        Button {
                            retryTranscription(forceProvider: provider)
                        } label: {
                            Label(provider.displayName, systemImage: providerIcon(provider))
                        }
                    }
                } label: {
                    Label("Retry with…", systemImage: "arrow.triangle.2.circlepath")
                }
            }
            Button {
                EpisodeDownloadService.shared.attach(appStore: store)
                EpisodeDownloadService.shared.download(episodeID: episode.id)
            } label: {
                Label(downloadButtonLabel, systemImage: "arrow.down.circle")
            }
            .disabled(downloadButtonDisabled)
        } header: {
            Text("Actions")
        } footer: {
            Text("Watch new events appear above as the pipeline runs.")
                .font(.footnote)
        }
    }

    // MARK: - Retry actions

    /// Kicks a fresh transcription. `forceProvider == nil` mirrors the legacy
    /// "Retry transcription" button (publisher → settings-configured STT);
    /// `forceProvider != nil` skips the publisher path and runs the chosen
    /// provider directly. State is reset to `.none` first so the user sees the
    /// failure marker clear and the new attempt's events stream in cleanly.
    private func retryTranscription(forceProvider: STTProvider?) {
        let providerLabel = forceProvider?.displayName ?? "settings-configured provider"
        EpisodeAuditLogStore.shared.record(
            episodeID: episode.id,
            kind: .transcriptRetryRequested,
            severity: .info,
            summary: "User tapped retry from Diagnostics (\(providerLabel))",
            details: [.init("Provider", providerLabel)]
        )
        store.setEpisodeTranscriptState(episode.id, state: .none)
        let episodeID = episode.id
        Task {
            await TranscriptIngestService.shared.ingest(
                episodeID: episodeID,
                forceProvider: forceProvider
            )
        }
    }

    /// Providers we can actually run on this device right now. Apple needs the
    /// episode downloaded (Apple's `SpeechTranscriber` requires a local file);
    /// ElevenLabs and OpenRouter each need their respective key configured.
    /// Order matches user-mental-priority: on-device first when available
    /// (free, private, offline), then the cloud options.
    private var availableRetryProviders: [STTProvider] {
        var out: [STTProvider] = []
        if case .downloaded = episode.downloadState {
            out.append(.appleNative)
        }
        if ElevenLabsCredentialStore.hasAPIKey() {
            out.append(.elevenLabsScribe)
        }
        if AssemblyAICredentialStore.hasAPIKey() {
            out.append(.assemblyAI)
        }
        if OpenRouterCredentialStore.hasAPIKey() {
            out.append(.openRouterWhisper)
        }
        return out
    }

    private func providerIcon(_ provider: STTProvider) -> String {
        switch provider {
        case .appleNative: return "cpu"
        case .elevenLabsScribe: return "waveform.and.mic"
        case .assemblyAI: return "waveform.badge.mic"
        case .openRouterWhisper: return "network"
        }
    }

    @ViewBuilder
    private var eventsSection: some View {
        Section {
            if events.isEmpty {
                emptyState
            } else {
                ForEach(events) { event in
                    EpisodeAuditEventRow(
                        event: event,
                        isExpanded: expandedEventIDs.contains(event.id),
                        onToggle: {
                            if expandedEventIDs.contains(event.id) {
                                expandedEventIDs.remove(event.id)
                            } else {
                                expandedEventIDs.insert(event.id)
                            }
                        }
                    )
                }
            }
        } header: {
            HStack {
                Text("Events")
                Spacer()
                Text("\(events.count)")
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
    }

    private var metadataSection: some View {
        Section {
            LabeledContent("Episode ID") {
                Text(episode.id.uuidString)
                    .font(.system(.caption, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            LabeledContent("Enclosure URL") {
                Text(episode.enclosureURL.absoluteString)
                    .font(.system(.caption, design: .monospaced))
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        } header: {
            Text("Metadata")
        }
    }

    private var emptyState: some View {
        HStack(spacing: 12) {
            Image(systemName: "tray")
                .foregroundStyle(.secondary)
            Text("No events recorded yet. Trigger a download or transcription to populate the log.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 8)
    }

    // MARK: - Derived strings

    private var downloadStateSummary: String {
        switch episode.downloadState {
        case .notDownloaded: return "not downloaded"
        case .queued: return "queued"
        case .downloading(let p, _): return "downloading (\(Int(p * 100))%)"
        case .downloaded(_, let bytes): return EpisodeDownloadService.formatBytes(bytes)
        case .failed(let m): return "failed — \(m)"
        }
    }

    private var transcriptStateSummary: String {
        switch episode.transcriptState {
        case .none: return "none"
        case .queued: return "queued"
        case .fetchingPublisher: return "fetching publisher"
        case .transcribing(let p): return "transcribing (\(Int(p * 100))%)"
        case .ready(let source): return "ready (\(String(describing: source)))"
        case .failed(let m): return "failed — \(m)"
        }
    }

    private var downloadInFlight: Bool {
        if case .downloading = episode.downloadState { return true }
        return false
    }

    private var downloadButtonLabel: String {
        switch episode.downloadState {
        case .downloaded: return "Already downloaded"
        case .downloading: return "Downloading…"
        case .failed: return "Retry download"
        case .notDownloaded, .queued: return "Start download"
        }
    }

    /// Button is inert when the download is already on disk or actively in
    /// flight — `EpisodeDownloadService.download` early-returns in both
    /// cases, so leaving the button enabled would be a silent no-op.
    private var downloadButtonDisabled: Bool {
        switch episode.downloadState {
        case .downloaded, .downloading: return true
        case .notDownloaded, .queued, .failed: return false
        }
    }
}

// MARK: - Row

private struct EpisodeAuditEventRow: View {
    let event: EpisodeAuditEvent
    let isExpanded: Bool
    let onToggle: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button(action: onToggle) {
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: event.kind.iconName)
                        .font(.system(size: 16))
                        .foregroundStyle(tint)
                        .frame(width: 22, alignment: .center)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(event.kind.displayLabel)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.primary)
                        Text(event.summary)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.leading)
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: 2) {
                        Text(event.timestamp, style: .time)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .monospacedDigit()
                        Text(event.timestamp, format: .dateTime.month(.abbreviated).day())
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
            }
            .buttonStyle(.plain)
            if isExpanded {
                detailGrid
                    .padding(.leading, 34)
            }
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private var detailGrid: some View {
        if event.details.isEmpty {
            Text("No additional detail captured.")
                .font(.caption2)
                .foregroundStyle(.tertiary)
        } else {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(event.details.enumerated()), id: \.offset) { _, detail in
                    HStack(alignment: .top, spacing: 8) {
                        Text(detail.label)
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(.secondary)
                            .frame(minWidth: 84, alignment: .leading)
                        Text(detail.value)
                            .font(.system(.caption2, design: .monospaced))
                            .foregroundStyle(.primary)
                            .textSelection(.enabled)
                            .lineLimit(nil)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .padding(.vertical, 4)
        }
    }

    private var tint: Color {
        switch event.severity {
        case .info: return .secondary
        case .success: return AppTheme.Tint.success
        case .warning: return AppTheme.Tint.warning
        case .failure: return AppTheme.Tint.error
        }
    }
}
