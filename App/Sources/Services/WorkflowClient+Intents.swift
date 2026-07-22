import Foundation

@MainActor
extension WorkflowClient {
    func configurePodcastDependencies(
        _ provider: @escaping @MainActor @Sendable () -> PodcastAgentToolDeps?
    ) {
        WorkflowRuntime.shared.podcastDepsProvider = provider
    }

    func startAndReconcile() async {
        WorkflowRuntime.shared.attach(client: self)
        await WorkflowRuntime.shared.startAndReconcile()
    }

    func reconcileAndDrain() async {
        await WorkflowRuntime.shared.reconcileAndDrain()
    }

    func wake() {
        WorkflowRuntime.shared.wake()
    }

    func requestTranscript(episodeID: UUID, provider: STTProvider? = nil) {
        WorkflowRuntime.shared.requestTranscript(episodeID: episodeID, provider: provider)
    }

    func perform(
        _ action: WorkflowJobAction,
        on projection: WorkflowJobProjection
    ) -> WorkflowJobActionResult {
        WorkflowRuntime.shared.perform(action, on: projection)
    }
}
