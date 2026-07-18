import SwiftUI

private struct WorkflowProjectionScopeModifier: ViewModifier {
    @Environment(WorkflowClient.self) private var workflows
    @State private var token: UUID?
    let request: WorkflowProjectionRequest

    func body(content: Content) -> some View {
        content
            .onAppear {
                if token == nil { token = workflows.register(request) }
            }
            .onChange(of: request) { _, newRequest in
                if let token { workflows.updateRegistration(token, request: newRequest) }
                else { token = workflows.register(newRequest) }
            }
            .onDisappear {
                if let token { workflows.unregister(token) }
                token = nil
            }
    }
}

extension View {
    func workflowProjectionScope(
        subjectIDs: some Sequence<UUID>,
        kinds: some Sequence<WorkJobKind>
    ) -> some View {
        modifier(WorkflowProjectionScopeModifier(request: WorkflowProjectionRequest(
            subjectIDs: subjectIDs,
            kinds: kinds
        )))
    }

    func workflowAttentionScope(kinds: some Sequence<WorkJobKind>) -> some View {
        modifier(WorkflowProjectionScopeModifier(request: WorkflowProjectionRequest(
            attentionKinds: kinds
        )))
    }

    func workflowRecentScope(kinds: some Sequence<WorkJobKind>) -> some View {
        modifier(WorkflowProjectionScopeModifier(request: WorkflowProjectionRequest(
            recentKinds: kinds
        )))
    }
}
