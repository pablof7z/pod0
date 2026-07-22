import Foundation
import Pod0Core

extension SharedLibraryClient {
    func observeDownloadEnvironment(
        network: DownloadNetworkStatus,
        availableCapacityBytes: Int64?
    ) {
        let mappedNetwork: DownloadNetworkState = switch network {
        case .unknown: .unknown
        case .unavailable: .unavailable
        case .wifi: .wifi
        case .other: .other
        }
        let capacity = availableCapacityBytes.flatMap { value in
            value >= 0 ? UInt64(value) : nil
        }
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(uuid: UUID()),
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: .observeDownloadEnvironment(
                observation: DownloadEnvironmentObservation(
                    network: mappedNetwork,
                    availableCapacityBytes: capacity
                )
            )
        ))
        dispatcher.executePendingRequests(from: facade)
    }
}
