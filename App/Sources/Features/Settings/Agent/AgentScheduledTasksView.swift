import SwiftUI

struct AgentScheduledTasksView: View {
    @Environment(AppStateStore.self) private var store
    @Environment(WorkflowClient.self) private var workflows

    @State private var showCreate = false
    @State private var editingTask: AgentScheduledTask? = nil

    // MARK: - Derived

    private var sortedTasks: [AgentScheduledTask] {
        store.scheduledTasks.sorted { $0.nextRunAt < $1.nextRunAt }
    }

    // MARK: - Body

    var body: some View {
        List {
            if sortedTasks.isEmpty {
                emptyState
            } else {
                taskRows
            }
        }
        .navigationTitle("Tasks")
        .navigationBarTitleDisplayMode(.large)
        .toolbar { toolbarContent }
        .sheet(isPresented: $showCreate) {
            AgentScheduledTaskFormSheet(mode: .create) { label, prompt, interval in
                store.addScheduledTask(label: label, prompt: prompt, intervalSeconds: interval)
            }
        }
        .sheet(item: $editingTask) { task in
            AgentScheduledTaskFormSheet(mode: .edit(task)) { label, prompt, interval in
                store.updateScheduledTask(id: task.id, label: label, prompt: prompt, intervalSeconds: interval)
            }
        }
    }

    // MARK: - Subviews

    @ViewBuilder
    private var emptyState: some View {
        ContentUnavailableView {
            Label("No scheduled tasks", systemImage: "calendar.badge.clock")
        } description: {
            Text("Ask your agent to schedule a recurring task, or tap + to create one.")
        } actions: {
            Button("Add Task") { showCreate = true }
                .buttonStyle(.glassProminent)
        }
        .listRowBackground(Color.clear)
    }

    @ViewBuilder
    private var taskRows: some View {
        ForEach(sortedTasks) { task in
            TaskRow(task: task)
                .workflowProjectionScope(subjectIDs: [task.id], kinds: [.scheduledAgentRun])
                .contentShape(Rectangle())
                .onTapGesture { editingTask = task }
                .swipeActions(edge: .leading) {
                    Button("Edit") { editingTask = task }
                        .tint(.blue)
                }
                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                    Button("Delete", role: .destructive) {
                        store.removeScheduledTask(id: task.id)
                        Haptics.selection()
                    }
                }
                .contextMenu {
                    Button("Edit") { editingTask = task }
                    Button("Delete", role: .destructive) {
                        store.removeScheduledTask(id: task.id)
                    }
                }
        }
    }

    @ToolbarContentBuilder
    private var toolbarContent: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                showCreate = true
            } label: {
                Label("Add Task", systemImage: "plus")
            }
        }
    }

    // MARK: - TaskRow

    private struct TaskRow: View {
        let task: AgentScheduledTask
        @Environment(WorkflowClient.self) private var workflows

        var body: some View {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                HStack(alignment: .top) {
                    Image(systemName: "calendar.badge.clock")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.teal)
                        .padding(.top, 2)
                        .accessibilityHidden(true)

                    VStack(alignment: .leading, spacing: 2) {
                        Text(task.label)
                            .font(AppTheme.Typography.callout.weight(.medium))

                        Text(task.prompt)
                            .font(AppTheme.Typography.subheadline)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                }

                HStack(spacing: AppTheme.Spacing.sm) {
                    Label(intervalLabel(task.intervalSeconds), systemImage: "repeat")
                        .font(AppTheme.Typography.caption2)
                        .foregroundStyle(.teal)
                        .padding(.horizontal, AppTheme.Spacing.xs)
                        .padding(.vertical, 1)
                        .background(Color.teal.opacity(0.10), in: Capsule())

                    Text(nextRunLabel(task))
                        .font(AppTheme.Typography.mono)
                        .foregroundStyle(.tertiary)

                    if let jobLabel {
                        Text(jobLabel)
                            .font(AppTheme.Typography.caption2)
                            .foregroundStyle(jobNeedsAttention ? AppTheme.Tint.warning : .secondary)
                    }
                }
                .padding(.leading, 18)
            }
            .padding(.vertical, AppTheme.Spacing.xs)
        }

        private func intervalLabel(_ seconds: TimeInterval) -> String {
            switch seconds {
            case 3_600:   return "Hourly"
            case 86_400:  return "Daily"
            case 604_800: return "Weekly"
            default:
                let hours = seconds / 3_600
                if hours >= 1, seconds.truncatingRemainder(dividingBy: 3_600) == 0 {
                    let h = Int(hours)
                    return "Every \(h)h"
                }
                return "Every \(Int(seconds))s"
            }
        }

        private func nextRunLabel(_ task: AgentScheduledTask) -> String {
            if task.isDue { return "Due now" }
            return "Next: \(RelativeTimestamp.extended(task.nextRunAt))"
        }

        private var jobLabel: String? {
            guard let state = workflows.latest(
                kind: .scheduledAgentRun,
                subjectID: task.id
            )?.state else { return nil }
            switch state {
            case .pending, .leased: return "Queued"
            case .running: return "Running"
            case .retryScheduled: return "Retry scheduled"
            case .blocked, .failedPermanent: return "Needs attention"
            case .succeeded: return "Last run complete"
            case .cancelled: return "Cancelled"
            case .obsolete: return nil
            }
        }

        private var jobNeedsAttention: Bool {
            let state = workflows.latest(kind: .scheduledAgentRun, subjectID: task.id)?.state
            return state == .blocked || state == .failedPermanent
        }
    }
}
