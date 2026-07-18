import SwiftUI

enum EpisodePreparationActionKind: Equatable, Sendable {
    case retry
    case cancel
    case openProviders
    case downloadEpisode
}

struct EpisodePreparationStatus: Equatable, Sendable {
    enum Tone: Equatable, Sendable { case working, attention, ready, quiet }

    let title: String
    let message: String
    let systemImage: String
    let tone: Tone
    let job: WorkflowJobProjection?
    let actions: [EpisodePreparationActionKind]
}

enum WorkflowPresentationCopy {
    static func title(for job: WorkflowJobProjection) -> String {
        let noun = noun(for: job.kind)
        switch job.state {
        case .pending, .leased: return "\(noun) queued"
        case .running: return runningTitle(for: job.kind)
        case .retryScheduled: return "\(noun) retry scheduled"
        case .blocked: return "\(noun) waiting for setup"
        case .failedPermanent: return "\(noun) needs attention"
        case .cancelled: return "\(noun) paused"
        case .obsolete: return "\(noun) superseded"
        case .succeeded: return job.kind == .scheduledAgentRun ? "Last run complete" : "\(noun) ready"
        }
    }

    static func detail(for job: WorkflowJobProjection) -> String {
        switch job.state {
        case .pending, .leased:
            return "Waiting for its background turn."
        case .running:
            return providerPhase(job.externalOperationState) ?? runningDetail(for: job.kind)
        case .retryScheduled:
            return "Pod0 will retry automatically when the workflow is due."
        case .blocked, .failedPermanent:
            return failureDetail(for: job)
        case .cancelled:
            return "This work was cancelled. You can restart it when you're ready."
        case .obsolete:
            return "A newer version of this work replaced the old attempt."
        case .succeeded:
            return "Finished \(RelativeTimestamp.extended(job.updatedAt))."
        }
    }

    static func failureDetail(for job: WorkflowJobProjection) -> String {
        UserFacingFailurePresenter.make(job: job).message
    }

    private static func noun(for kind: WorkJobKind) -> String {
        switch kind {
        case .transcriptIngest: return "Transcript"
        case .transcriptIndex, .metadataIndex: return "Search index"
        case .publisherChapters, .chapterArtifacts: return "Chapters"
        case .scheduledAgentRun: return "Agent task"
        case .download: return "Download"
        case .feedDiscovery: return "Feed refresh"
        case .autoDownload: return "Auto-download"
        case .newEpisodeNotification: return "Notification"
        }
    }

    private static func runningTitle(for kind: WorkJobKind) -> String {
        switch kind {
        case .transcriptIngest: return "Preparing transcript"
        case .transcriptIndex, .metadataIndex: return "Indexing episode"
        case .publisherChapters: return "Fetching chapters"
        case .chapterArtifacts: return "Creating chapters"
        case .scheduledAgentRun: return "Agent task running"
        default: return "\(noun(for: kind)) running"
        }
    }

    private static func runningDetail(for kind: WorkJobKind) -> String {
        switch kind {
        case .transcriptIngest: return "Audio is being turned into searchable text."
        case .transcriptIndex, .metadataIndex: return "Making this episode available to search and recall."
        case .publisherChapters: return "Fetching the publisher's chapter markers."
        case .chapterArtifacts: return "Building meaningful chapter markers from the transcript."
        case .scheduledAgentRun: return "The agent is working on this scheduled prompt."
        default: return "Background work is in progress."
        }
    }

    private static func providerPhase(_ rawValue: String?) -> String? {
        switch rawValue?.lowercased() {
        case "submitted", "queued": return "Sent to the provider and waiting to start."
        case "processing", "transcribing", "running": return "The provider is processing the audio."
        case "finalizing": return "The provider is finalizing the result."
        default: return nil
        }
    }
}

enum EpisodePreparationPresenter {
    static func make(
        episode: Episode,
        jobs: [WorkflowJobProjection]
    ) -> EpisodePreparationStatus? {
        let relevant = jobs.filter { $0.state != .obsolete }
        if let job = relevant.sorted(by: precedes).first {
            if job.state == .succeeded,
               case .ready = episode.transcriptState,
               relevant.allSatisfy({ !$0.state.isActive && !needsAttention($0.state) }) {
                return readyStatus(episode: episode, job: job)
            }
            return status(for: job, episode: episode)
        }
        if case .ready = episode.transcriptState { return readyStatus(episode: episode, job: nil) }
        return nil
    }

    private static func status(
        for job: WorkflowJobProjection,
        episode: Episode
    ) -> EpisodePreparationStatus {
        var actions: [EpisodePreparationActionKind] = []
        if job.allowedActions.contains(.retry) { actions.append(.retry) }
        if job.allowedActions.contains(.cancel) { actions.append(.cancel) }
        if job.lastErrorClass == .missingCredential { actions.insert(.openProviders, at: 0) }
        if job.lastErrorClass == .missingDependency,
           job.kind == .transcriptIngest,
           case .notDownloaded = episode.downloadState {
            actions.insert(.downloadEpisode, at: 0)
        }
        let attention = needsAttention(job.state)
        let working = job.state.isActive && !attention
        return EpisodePreparationStatus(
            title: WorkflowPresentationCopy.title(for: job),
            message: WorkflowPresentationCopy.detail(for: job),
            systemImage: attention ? "exclamationmark.triangle" : (working ? "sparkles" : "pause.circle"),
            tone: attention ? .attention : (working ? .working : .quiet),
            job: job,
            actions: Array(actions.prefix(2))
        )
    }

    private static func readyStatus(
        episode: Episode,
        job: WorkflowJobProjection?
    ) -> EpisodePreparationStatus {
        let hasChapters = episode.chapters?.isEmpty == false
        return EpisodePreparationStatus(
            title: "Ready to recall",
            message: hasChapters
                ? "Transcript and chapters are ready for search, clips, and the agent."
                : "The transcript is ready for search, clips, and the agent.",
            systemImage: "checkmark.circle.fill",
            tone: .ready,
            job: job,
            actions: []
        )
    }

    private static func precedes(
        _ lhs: WorkflowJobProjection,
        _ rhs: WorkflowJobProjection
    ) -> Bool {
        let left = priority(lhs.state)
        let right = priority(rhs.state)
        return left == right ? lhs.updatedAt > rhs.updatedAt : left < right
    }

    private static func priority(_ state: WorkJobState) -> Int {
        switch state {
        case .blocked, .failedPermanent: 0
        case .running: 1
        case .leased, .pending: 2
        case .retryScheduled: 3
        case .cancelled: 4
        case .succeeded: 5
        case .obsolete: 6
        }
    }

    private static func needsAttention(_ state: WorkJobState) -> Bool {
        state == .blocked || state == .failedPermanent
    }
}

struct EpisodePreparationStatusView: View {
    let status: EpisodePreparationStatus
    let onAction: (EpisodePreparationActionKind, WorkflowJobProjection?) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            HStack(spacing: AppTheme.Spacing.sm) {
                if status.tone == .working { ProgressView().controlSize(.small) }
                else { Image(systemName: status.systemImage).foregroundStyle(tint) }
                Text(status.title).font(AppTheme.Typography.callout.weight(.semibold))
            }
            Text(status.message)
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if !status.actions.isEmpty {
                HStack {
                    ForEach(Array(status.actions.enumerated()), id: \.offset) { _, action in
                        Button(actionTitle(action)) { onAction(action, status.job) }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                    }
                }
            }
        }
        .padding(AppTheme.Spacing.md)
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(
            cornerRadius: AppTheme.Corner.lg,
            style: .continuous
        ))
        .accessibilityElement(children: .contain)
    }

    private var tint: Color {
        switch status.tone {
        case .working: .accentColor
        case .attention: AppTheme.Tint.warning
        case .ready: .green
        case .quiet: .secondary
        }
    }

    private func actionTitle(_ action: EpisodePreparationActionKind) -> String {
        switch action {
        case .retry: "Retry"
        case .cancel: "Cancel"
        case .openProviders: "Connect provider"
        case .downloadEpisode: "Download episode"
        }
    }
}

struct WorkflowActionNotice: Identifiable {
    let id = UUID()
    let title: String
    let message: String

    static func make(for result: WorkflowJobActionResult) -> WorkflowActionNotice? {
        switch result {
        case .accepted:
            return nil
        case .stale:
            return .init(
                title: "Status changed",
                message: "This work changed before the action completed. Review its current status and try again."
            )
        case .notAllowed:
            return .init(
                title: "Action unavailable",
                message: "Pod0 cannot safely perform that action in the current state."
            )
        case .alreadyComplete:
            return .init(title: "Already finished", message: "No action is needed.")
        case .notFound:
            return .init(title: "Work not found", message: "The old workflow record is no longer available.")
        case .failed:
            return .init(
                title: "Couldn't update work",
                message: "Pod0 could not safely update this work. Review its current status and try again."
            )
        }
    }
}
