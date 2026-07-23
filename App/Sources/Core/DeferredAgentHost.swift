import Pod0Core

@MainActor
final class DeferredAgentHost: CoreAgentHosting {
    private var host: (any CoreAgentHosting)?

    func attach(_ host: any CoreAgentHosting) {
        self.host = host
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        guard let host else {
            return await UnavailableCoreAgentHost().execute(request)
        }
        return await host.execute(request)
    }
}
