import Pod0Core

protocol CoreRecallHosting: Sendable {
    func execute(_ request: HostRequest) async -> HostObservation
}

struct UnavailableCoreRecallHost: CoreRecallHosting {
    func execute(_ request: HostRequest) async -> HostObservation {
        .failed(code: .indexUnavailable, safeDetail: "Recall capabilities are not attached")
    }
}

actor DeferredRecallHost: CoreRecallHosting {
    private var host: any CoreRecallHosting = UnavailableCoreRecallHost()

    func attach(_ host: any CoreRecallHosting) {
        self.host = host
    }

    func execute(_ request: HostRequest) async -> HostObservation {
        await host.execute(request)
    }
}
