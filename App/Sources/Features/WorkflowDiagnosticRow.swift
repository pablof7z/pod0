import SwiftUI

struct WorkflowDiagnosticSnapshot: Equatable {
    let kindTitle: String
    let stateTitle: String
    let detail: String
    let metadata: String
    let classification: String?
    let actions: [WorkflowJobAction]
}

enum WorkflowDiagnosticPresenter {
    static func make(job: WorkflowJobProjection) -> WorkflowDiagnosticSnapshot {
        WorkflowDiagnosticSnapshot(
            kindTitle: kindTitle(job.kind),
            stateTitle: stateTitle(job.state),
            detail: WorkflowPresentationCopy.detail(for: job),
            metadata: "Attempt \(job.attempt) of \(job.maxAttempts) · Updated \(RelativeTimestamp.extended(job.updatedAt))",
            classification: job.lastErrorClass.map(errorTitle),
            actions: WorkflowJobAction.allCases.filter(job.allowedActions.contains)
        )
    }

    static func errorTitle(_ errorClass: JobErrorClass) -> String {
        UserFacingFailurePresenter.make(
            failure: ProductFailure(code: errorClass.productFailureCode)
        ).title
    }

    static func kindTitle(_ kind: WorkJobKind) -> String {
        switch kind {
        case .feedDiscovery: "Feed discovery"
        case .download: "Download"
        case .transcriptIngest: "Transcript ingest"
        case .transcriptIndex: "Transcript index"
        case .publisherChapters: "Publisher chapters"
        case .chapterArtifacts: "Chapter artifacts"
        case .metadataIndex: "Metadata index"
        case .autoDownload: "Auto-download"
        case .newEpisodeNotification: "New-episode notification"
        case .scheduledAgentRun: "Scheduled agent run"
        }
    }

    static func kindIcon(_ kind: WorkJobKind) -> String {
        switch kind {
        case .feedDiscovery: "dot.radiowaves.left.and.right"
        case .download: "arrow.down.circle"
        case .transcriptIngest: "waveform.badge.mic"
        case .transcriptIndex: "text.magnifyingglass"
        case .publisherChapters: "list.number"
        case .chapterArtifacts: "sparkles.rectangle.stack"
        case .metadataIndex: "magnifyingglass.circle"
        case .autoDownload: "arrow.down.to.line.compact"
        case .newEpisodeNotification: "bell.badge"
        case .scheduledAgentRun: "calendar.badge.clock"
        }
    }

    static func stateTitle(_ state: WorkJobState) -> String {
        switch state {
        case .pending: "Pending"
        case .leased: "Starting"
        case .running: "Running"
        case .retryScheduled: "Retry scheduled"
        case .blocked: "Blocked"
        case .failedPermanent: "Failed"
        case .cancelled: "Cancelled"
        case .obsolete: "Superseded"
        case .succeeded: "Succeeded"
        }
    }
}

struct WorkflowDiagnosticRow: View {
    let job: WorkflowJobProjection
    var showsSubject = true
    let onAction: (WorkflowJobAction, WorkflowJobProjection) -> Void

    private var snapshot: WorkflowDiagnosticSnapshot {
        WorkflowDiagnosticPresenter.make(job: job)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            HStack(alignment: .firstTextBaseline) {
                Label(snapshot.kindTitle, systemImage: WorkflowDiagnosticPresenter.kindIcon(job.kind))
                    .font(AppTheme.Typography.callout.weight(.semibold))
                Spacer()
                Text(snapshot.stateTitle)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(stateTint)
            }
            Text(snapshot.detail)
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Text(snapshot.metadata)
                .font(AppTheme.Typography.caption2)
                .foregroundStyle(.tertiary)
            if let classification = snapshot.classification {
                Text(classification)
                    .font(AppTheme.Typography.caption2.weight(.medium))
                    .foregroundStyle(stateTint)
            }
            if showsSubject {
                Text("Job \(job.id.uuidString)")
                    .font(.system(.caption2, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                Text("Subject \(job.subjectID.uuidString)")
                    .font(.system(.caption2, design: .monospaced))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            }
            if !snapshot.actions.isEmpty {
                HStack {
                    ForEach(snapshot.actions, id: \.rawValue) { action in
                        Button(action == .retry ? "Retry" : "Cancel") {
                            onAction(action, job)
                        }
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        .tint(action == .cancel ? AppTheme.Tint.warning : .accentColor)
                    }
                }
            }
        }
        .padding(.vertical, AppTheme.Spacing.xs)
        .accessibilityElement(children: .contain)
    }

    private var stateTint: Color {
        switch job.state {
        case .blocked, .failedPermanent: AppTheme.Tint.warning
        case .running: .accentColor
        case .succeeded: .green
        default: .secondary
        }
    }
}
