import Foundation
import Pod0Core

extension SharedLibraryClient {
    func ensureNostrSigner() {
        if cachedNostrSigner?.account?.stage == .ready { return }
        facade.dispatch(command: CommandEnvelope(
            commandId: CommandId(uuid: UUID()),
            cancellationId: CancellationId(uuid: UUID()),
            expectedRevision: nil,
            command: .ensureNostrSigner
        ))
        dispatcher.executePendingRequests(from: facade)
    }

    func receiveNostrSigner(_ projection: SignerProjection, revision: UInt64) {
        guard revision >= lastNostrSignerRevision else { return }
        lastNostrSignerRevision = revision
        cachedNostrSigner = projection
        resolveWaiters(projection.operations)
        dispatcher.executePendingRequests(from: facade)
    }
}
