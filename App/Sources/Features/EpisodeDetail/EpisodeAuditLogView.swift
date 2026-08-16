import Pod0Core
import SwiftUI

// MARK: - EpisodeAuditLogView

/// "Diagnostics" sheet for a single episode. Answers the user's question:
/// *why doesn't this episode have a transcript / why didn't the download work?*
///
/// Renders a bounded audit snapshot in reverse-chronological order. Each row
/// summarises the event; tapping reveals its captured details.
///
/// Two retry affordances at the top:
///   - "Retry transcription" dispatches a typed retry to the Rust workflow
///     so the user can watch new events stream in.
///   - "Retry download" kicks the download service for failed / missing files.
struct EpisodeAuditLogView: View {
    let episode: Episode

    @Environment(AppStateStore.self) private var store
    @Environment(WorkflowClient.self) private var workflows
    @Environment(\.dismiss) private var dismiss

    @State private var activityPage: LatestEpisodeActivityPage?
    @State private var expandedSequences: Set<UInt64> = []
    @State private var isLoadingActivity = false
    @State private var actionNotice: WorkflowActionNotice?
    private var events: [EpisodeActivityEntry] { activityPage?.items ?? [] }

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
                    Button("Refresh", systemImage: "arrow.clockwise") {
                        Task { await loadActivity() }
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
            kinds: WorkflowProjectionKind.allCases
        )
        .task(id: activityRefreshToken) { await loadActivity() }
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
                    store.sharedLibrary?.requestDownload(episodeID: episode.id)
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
        WorkflowProjectionKind.allCases
            .compactMap { workflows.latest(kind: $0, subjectID: episode.id) }
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    private func requestTranscript() {
        workflows.requestTranscript(episodeID: episode.id)
    }

    private func perform(_ action: WorkflowJobAction, on job: WorkflowJobProjection) {
        Task { actionNotice = .make(for: await workflows.perform(action, on: job)) }
    }

    @ViewBuilder
    private var eventsSection: some View {
        Section {
            if isLoadingActivity, activityPage == nil {
                ProgressView()
            } else if activityPage?.available == false {
                Text("The durable activity journal is unavailable.")
                    .foregroundStyle(.secondary)
            } else if events.isEmpty {
                emptyState
            } else {
                ForEach(events, id: \.sequence) { event in
                    EpisodeActivityEntryRow(
                        entry: event,
                        isExpanded: expandedSequences.contains(event.sequence),
                        onToggle: {
                            if expandedSequences.contains(event.sequence) {
                                expandedSequences.remove(event.sequence)
                            } else {
                                expandedSequences.insert(event.sequence)
                            }
                        }
                    )
                }
                if activityPage?.nextBeforeSequence != nil {
                    Button("Load more", systemImage: "arrow.down") {
                        Task { await loadMoreActivity() }
                    }
                    .disabled(isLoadingActivity)
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
            Text("No durable activity has been recorded for this episode.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .padding(.vertical, 8)
    }

    // MARK: - Derived strings

    private var downloadStateSummary: String {
        switch episode.downloadState {
        case .downloaded(_, let bytes):
            return ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
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

    private var activityRefreshToken: String {
        workflowJobs
            .map { "\($0.id):\($0.updatedAt.timeIntervalSince1970)" }
            .joined(separator: "|")
    }

    private func loadActivity() async {
        isLoadingActivity = true
        activityPage = await CoreEpisodeActivityReader.shared.firstPage(
            for: EpisodeId(uuid: episode.id),
            from: store.sharedLibrary?.facade
        )
        isLoadingActivity = false
    }

    private func loadMoreActivity() async {
        guard let facade = store.sharedLibrary?.facade,
              let activityPage else { return }
        isLoadingActivity = true
        self.activityPage = await CoreEpisodeActivityReader.shared.loadMore(
            for: EpisodeId(uuid: episode.id),
            current: activityPage,
            from: facade
        )
        isLoadingActivity = false
    }
}
