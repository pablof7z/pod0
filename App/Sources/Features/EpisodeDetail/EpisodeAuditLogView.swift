import SwiftUI

// MARK: - EpisodeAuditLogView

/// "Diagnostics" sheet for a single episode. Answers the user's question:
/// *why doesn't this episode have a transcript / why didn't the download work?*
///
/// Renders the full audit log in reverse-chronological order. Each row
/// summarises the event; tapping reveals its captured details.
///
/// Two retry affordances at the top:
///   - "Retry transcription" pushes a fresh `TranscriptIngestService.ingest`
///     so the user can watch new events stream in.
///   - "Retry download" kicks the download service for failed / missing files.
struct EpisodeAuditLogView: View {
    let episode: Episode

    @Environment(AppStateStore.self) private var store
    @Environment(WorkflowClient.self) private var workflows
    @Environment(\.dismiss) private var dismiss

    @State private var auditStore = EpisodeAuditLogStore.shared
    @State private var expandedEventIDs: Set<UUID> = []
    @State private var actionNotice: WorkflowActionNotice?
    private var events: [EpisodeAuditEvent] {
        auditStore.eventsNewestFirst(for: episode.id)
    }

    var body: some View {
        NavigationStack {
            List {
                summarySection
                workflowSection
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
        .alert(item: $actionNotice) { notice in
            Alert(
                title: Text(notice.title),
                message: Text(notice.message),
                dismissButton: .default(Text("OK"))
            )
        }
        .workflowProjectionScope(
            subjectIDs: [episode.id],
            kinds: WorkJobKind.allCases
        )
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

    private var workflowSection: some View {
        Section {
            if workflowJobs.isEmpty {
                Text("No durable work has been scheduled for this episode.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(workflowJobs) { job in
                    WorkflowDiagnosticRow(job: job, showsSubject: false, onAction: perform)
                }
            }
            if transcriptJob == nil, case .none = episode.transcriptState {
                Button("Request transcript", systemImage: "waveform.badge.mic") {
                    requestTranscript()
                }
            }
            if downloadJob == nil, case .notDownloaded = episode.downloadState {
                Button("Start download", systemImage: "arrow.down.circle") {
                    EpisodeDownloadService.shared.attach(appStore: store)
                    EpisodeDownloadService.shared.download(episodeID: episode.id)
                }
            }
        } header: {
            Text("Durable work")
        } footer: {
            Text("Actions appear only when the current revision permits them. Sensitive provider and lease details are never displayed.")
                .font(.footnote)
        }
    }

    private var workflowJobs: [WorkflowJobProjection] {
        WorkJobKind.allCases
            .compactMap { workflows.latest(kind: $0, subjectID: episode.id) }
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    private func requestTranscript() {
        EpisodeAuditLogStore.shared.record(
            episodeID: episode.id,
            kind: .transcriptRetryRequested,
            severity: .info,
            summary: "User requested a transcript from Diagnostics"
        )
        workflows.requestTranscript(episodeID: episode.id)
    }

    private func perform(_ action: WorkflowJobAction, on job: WorkflowJobProjection) {
        actionNotice = .make(for: workflows.perform(action, on: job))
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
        case .downloaded(_, let bytes): return EpisodeDownloadService.formatBytes(bytes)
        case .notDownloaded:
            return jobSummary(downloadJob) ?? "not downloaded"
        }
    }

    private var transcriptStateSummary: String {
        switch episode.transcriptState {
        case .ready(let source): return "ready (\(String(describing: source)))"
        case .none:
            return jobSummary(transcriptJob) ?? "none"
        }
    }

    private var downloadJob: WorkflowJobProjection? {
        workflows.latest(kind: .download, subjectID: episode.id)
    }

    private var transcriptJob: WorkflowJobProjection? {
        workflows.latest(kind: .transcriptIngest, subjectID: episode.id)
    }

    private func jobSummary(_ job: WorkflowJobProjection?) -> String? {
        job.map { WorkflowDiagnosticPresenter.stateTitle($0.state).lowercased() }
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
                        Text(EpisodeAuditPresentation.summary(for: event))
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
        if safeDetails.isEmpty {
            Text("No additional detail captured.")
                .font(.caption2)
                .foregroundStyle(.tertiary)
        } else {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(safeDetails.enumerated()), id: \.offset) { _, detail in
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

    private var safeDetails: [EpisodeAuditEvent.Detail] {
        EpisodeAuditPresentation.details(for: event)
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
