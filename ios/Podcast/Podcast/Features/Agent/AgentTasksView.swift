import SwiftUI

// MARK: - AgentTasksView
//
// Feature #34 — agent scheduled tasks. Renders the
// `podcastSnapshot.agentTasks` projection as a swipe-to-delete list,
// with per-row enable/disable toggle + "Run Now" button and a "New
// Task" toolbar action that presents `CreateTaskSheet`.
//
// All mutations dispatch into the `podcast.tasks.*` action namespace
// (see `apps/nmp-app-podcast/src/ffi/actions/tasks_module.rs`). The
// view never holds its own task state — it re-renders from the
// snapshot each tick, matching the D2/D4/D8 thin-shell discipline.

struct AgentTasksView: View {

    @Environment(KernelModel.self) private var model
    @State private var createSheetPresented = false

    private var tasks: [AgentTaskSummary] {
        model.podcastSnapshot?.agentTasks ?? []
    }

    var body: some View {
        Group {
            if tasks.isEmpty {
                emptyState
            } else {
                taskList
            }
        }
        .navigationTitle("Scheduled Tasks")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    createSheetPresented = true
                } label: {
                    Label("New Task", systemImage: "plus")
                }
                .accessibilityLabel("New Task")
            }
        }
        .sheet(isPresented: $createSheetPresented) {
            CreateTaskSheet()
        }
    }

    // MARK: - Empty state

    @ViewBuilder
    private var emptyState: some View {
        VStack(spacing: PodcastSpace.m) {
            Image(systemName: "calendar.badge.clock")
                .font(.system(size: 44, weight: .regular))
                .foregroundStyle(.secondary)
            Text("No scheduled tasks")
                .font(PodcastFont.headline)
                .foregroundStyle(.primary)
            Text("The agent has no recurring jobs queued. Tap + to create one.")
                .font(PodcastFont.callout)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, PodcastSpace.xl)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding()
    }

    // MARK: - List

    private var taskList: some View {
        List {
            ForEach(tasks) { task in
                AgentTaskRow(task: task) { action in
                    dispatch(action, for: task)
                }
            }
            .onDelete(perform: delete)
        }
        .listStyle(.insetGrouped)
    }

    // MARK: - Dispatch

    private func dispatch(_ action: AgentTaskRow.RowAction, for task: AgentTaskSummary) {
        switch action {
        case .toggleEnabled(let newValue):
            let op = newValue ? "enable" : "disable"
            model.dispatch(namespace: "podcast.tasks", body: ["op": op, "task_id": task.id])
        case .runNow:
            model.dispatch(namespace: "podcast.tasks", body: ["op": "run_now", "task_id": task.id])
        }
    }

    private func delete(at offsets: IndexSet) {
        for index in offsets {
            let task = tasks[index]
            model.dispatch(namespace: "podcast.tasks", body: ["op": "delete", "task_id": task.id])
        }
    }
}

// MARK: - AgentTaskRow

private struct AgentTaskRow: View {

    enum RowAction { case toggleEnabled(Bool), runNow }

    let task: AgentTaskSummary
    let onAction: (RowAction) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: PodcastSpace.s) {
            HStack(alignment: .top, spacing: PodcastSpace.m) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(task.title)
                        .font(PodcastFont.headline)
                        .foregroundStyle(.primary)
                    if let description = task.description, !description.isEmpty {
                        Text(description)
                            .font(PodcastFont.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }
                }
                Spacer(minLength: 0)
                Toggle("", isOn: Binding(
                    get: { task.isEnabled },
                    set: { onAction(.toggleEnabled($0)) }
                ))
                .labelsHidden()
                .accessibilityLabel("Enable task")
            }

            HStack(spacing: PodcastSpace.s) {
                scheduleBadge
                statusBadge
                Spacer(minLength: 0)
                Button {
                    onAction(.runNow)
                } label: {
                    Label("Run Now", systemImage: "play.circle.fill")
                        .font(PodcastFont.caption.weight(.semibold))
                }
                .buttonStyle(.borderless)
                .tint(PodcastColor.accent)
                .accessibilityLabel("Run task now")
            }
        }
        .padding(.vertical, PodcastSpace.xs)
    }

    private var scheduleBadge: some View {
        Text(task.schedule.uppercased())
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, PodcastSpace.s)
            .padding(.vertical, 3)
            .background(PodcastColor.accentSoft, in: Capsule())
            .foregroundStyle(PodcastColor.accent)
    }

    @ViewBuilder
    private var statusBadge: some View {
        let label = task.status.capitalized
        Text(label)
            .font(.caption2.weight(.medium))
            .padding(.horizontal, PodcastSpace.s)
            .padding(.vertical, 3)
            .background(statusBackground, in: Capsule())
            .foregroundStyle(statusForeground)
    }

    private var statusBackground: Color {
        switch task.status {
        case "completed": PodcastColor.success.opacity(0.18)
        case "running": PodcastColor.warning.opacity(0.18)
        case "failed": PodcastColor.danger.opacity(0.18)
        default: PodcastColor.secondaryFill
        }
    }

    private var statusForeground: Color {
        switch task.status {
        case "completed": PodcastColor.success
        case "running": PodcastColor.warning
        case "failed": PodcastColor.danger
        default: PodcastColor.textSecondary
        }
    }
}
