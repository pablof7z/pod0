import SwiftUI

struct WorkflowDiagnosticsView: View {
    @Environment(WorkflowClient.self) private var workflows
    @State private var selectedKind: WorkflowProjectionKind?
    @State private var actionNotice: WorkflowActionNotice?

    private var jobs: [WorkflowJobProjection] {
        workflows.allJobs().filter { selectedKind == nil || $0.kind == selectedKind }
    }

    var body: some View {
        List {
            inventorySection
            jobsSection
        }
        .navigationTitle("Workflow Diagnostics")
        .navigationBarTitleDisplayMode(.inline)
        .workflowRecentScope(kinds: WorkflowProjectionKind.allCases)
        .toolbar { filterToolbar }
        .alert(item: $actionNotice) { notice in
            Alert(
                title: Text(notice.title),
                message: Text(notice.message),
                dismissButton: .default(Text("OK"))
            )
        }
    }

    private var inventorySection: some View {
        Section("Kinds") {
            ForEach(WorkflowProjectionKind.allCases, id: \.rawValue) { kind in
                Button {
                    selectedKind = selectedKind == kind ? nil : kind
                } label: {
                    HStack {
                        Label(
                            WorkflowDiagnosticPresenter.kindTitle(kind),
                            systemImage: WorkflowDiagnosticPresenter.kindIcon(kind)
                        )
                        Spacer()
                        Text("\(workflows.jobs(kind: kind).count)")
                            .foregroundStyle(.secondary)
                            .monospacedDigit()
                        if selectedKind == kind {
                            Image(systemName: "checkmark").foregroundStyle(Color.accentColor)
                        }
                    }
                }
                .buttonStyle(.plain)
            }
        }
    }

    @ViewBuilder
    private var jobsSection: some View {
        Section {
            if jobs.isEmpty {
                ContentUnavailableView(
                    "No workflow records",
                    systemImage: "checkmark.circle",
                    description: Text("Work will appear here after Pod0 schedules it.")
                )
            } else {
                ForEach(jobs) { job in
                    WorkflowDiagnosticRow(job: job, onAction: perform)
                }
            }
        } header: {
            Text(selectedKind.map(WorkflowDiagnosticPresenter.kindTitle) ?? "Recent work")
        } footer: {
            Text("Shows up to 1,000 recent durable jobs. Provider payloads, credentials, file paths, lease tokens, and external operation IDs are never displayed.")
        }
    }

    @ToolbarContentBuilder
    private var filterToolbar: some ToolbarContent {
        if selectedKind != nil {
            ToolbarItem(placement: .topBarTrailing) {
                Button("Show All") { selectedKind = nil }
            }
        }
    }

    private func perform(_ action: WorkflowJobAction, on job: WorkflowJobProjection) {
        actionNotice = .make(for: workflows.perform(action, on: job))
    }
}
