import SwiftUI

struct AgentScheduledTasksView: View {
    @Environment(AppStateStore.self) private var store
    @Environment(WorkflowClient.self) private var workflows

    @State private var showCreate = false
    @State private var editingTask: AgentScheduledTask? = nil
    @State private var showProviderSettings = false
    @State private var workflowActionNotice: WorkflowActionNotice?

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
        .sheet(isPresented: $showProviderSettings) {
            NavigationStack { AIProvidersSettingsView() }
        }
        .alert(item: $workflowActionNotice) { notice in
            Alert(
                title: Text(notice.title),
                message: Text(notice.message),
                dismissButton: .default(Text("OK"))
            )
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
                    if let job = job(for: task) {
                        Divider()
                        if job.lastErrorClass == .missingCredential {
                            Button("Connect provider") { showProviderSettings = true }
                        }
                        if job.allowedActions.contains(.retry) {
                            Button("Retry now") { perform(.retry, on: job) }
                        }
                        if job.allowedActions.contains(.cancel) {
                            Button("Cancel run", role: .destructive) { perform(.cancel, on: job) }
                        }
                    }
                }
        }
    }

    private func job(for task: AgentScheduledTask) -> WorkflowJobProjection? {
        workflows.latest(kind: .scheduledAgentRun, subjectID: task.id)
    }

    private func perform(_ action: WorkflowJobAction, on job: WorkflowJobProjection) {
        workflowActionNotice = .make(for: workflows.perform(action, on: job))
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

                if let job {
                    Label(WorkflowPresentationCopy.title(for: job), systemImage: jobIcon(job))
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(jobNeedsAttention ? AppTheme.Tint.warning : .secondary)
                    if job.state == .running || job.state == .retryScheduled || jobNeedsAttention {
                        Text(WorkflowPresentationCopy.detail(for: job))
                            .font(AppTheme.Typography.caption2)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
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

                }
                .padding(.leading, 18)

                if let lastRunAt = task.lastRunAt {
                    Text("Last run: \(RelativeTimestamp.extended(lastRunAt))")
                        .font(AppTheme.Typography.caption2)
                        .foregroundStyle(.tertiary)
                        .padding(.leading, 18)
                }
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

        private var job: WorkflowJobProjection? {
            workflows.latest(kind: .scheduledAgentRun, subjectID: task.id)
        }

        private var jobNeedsAttention: Bool {
            let state = job?.state
            return state == .blocked || state == .failedPermanent
        }

        private func jobIcon(_ job: WorkflowJobProjection) -> String {
            if jobNeedsAttention { return "exclamationmark.triangle" }
            switch job.state {
            case .running: return "sparkles"
            case .succeeded: return "checkmark.circle"
            case .cancelled: return "pause.circle"
            default: return "clock"
            }
        }
    }
}
