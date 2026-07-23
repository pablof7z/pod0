import Foundation
import Pod0Core
import SwiftUI

struct AgentApprovalPresenter: ViewModifier {
    let coordinator: AgentApprovalCoordinator

    func body(content: Content) -> some View {
        content.sheet(item: Binding(
            get: { coordinator.current },
            set: { newValue in
                if newValue == nil, let current = coordinator.current {
                    coordinator.deny(current.id)
                }
            }
        )) { pending in
            AgentApprovalSheet(coordinator: coordinator, pending: pending)
                .presentationDetents([.medium])
                .presentationDragIndicator(.visible)
        }
    }
}

extension View {
    func agentApprovalPresenter(coordinator: AgentApprovalCoordinator) -> some View {
        modifier(AgentApprovalPresenter(coordinator: coordinator))
    }
}

private struct AgentApprovalSheet: View {
    let coordinator: AgentApprovalCoordinator
    let pending: AgentApprovalCoordinator.PendingApproval
    @State private var resolved = false

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            Label("Approve agent action", systemImage: "checkmark.shield")
                .font(.title3.weight(.semibold))
            Text("Pod0 will approve only this exact action.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text(summary.title)
                    .font(.headline)
                Text(summary.detail)
                    .font(.body)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(AppTheme.Spacing.md)
            .background(
                RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                    .fill(.thinMaterial)
            )
            Spacer(minLength: AppTheme.Spacing.sm)
            HStack(spacing: AppTheme.Spacing.md) {
                Button("Deny", role: .destructive) { deny() }
                    .buttonStyle(.bordered)
                    .frame(maxWidth: .infinity)
                Button("Approve") { approve() }
                    .buttonStyle(.glassProminent)
                    .frame(maxWidth: .infinity)
            }
        }
        .padding(AppTheme.Spacing.lg)
        .onDisappear {
            if !resolved { coordinator.deny(pending.id) }
        }
    }

    private var summary: AgentApprovalSummary {
        AgentApprovalSummary(action: pending.request.proposal.action)
    }

    private func approve() {
        resolved = true
        coordinator.approve(pending.id)
    }

    private func deny() {
        resolved = true
        coordinator.deny(pending.id)
    }
}

private struct AgentApprovalSummary {
    let title: String
    let detail: String

    init(action: AgentToolAction) {
        switch action {
        case .createNote(let text):
            title = "Save a note"
            detail = text
        case .search(_, let query, let scope, let limit):
            title = "Search your library"
            detail = "Query: \(query)\nScope: \(scope ?? "all podcasts")\nLimit: \(limit)"
        case .podcast(let tool, let podcastID):
            title = "Read podcast details"
            detail = "Tool: \(String(describing: tool))\nPodcast: \(podcastID.displayString)"
        case .noArguments(let tool):
            title = "Run \(String(describing: tool))"
            detail = "This action has no editable arguments."
        case .setPlaybackRate(let permille):
            title = "Change playback speed"
            detail = String(format: "%.2fx", Double(permille) / 1_000)
        default:
            title = "Run an exact agent action"
            detail = String(reflecting: action)
        }
    }
}

private extension PodcastId {
    var displayString: String {
        String(format: "%016llx%016llx", high, low)
    }
}
