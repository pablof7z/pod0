import Foundation
import Pod0Core

extension SharedLibraryClient {
    func ensureNostrSigner() {
        if cachedNostrSigner?.account?.stage == .ready { return }
        dispatchCoreCommand(.ensureNostrSigner)
    }

    func receiveNostrSigner(_ projection: SignerProjection, revision: UInt64) {
        guard revision >= lastNostrSignerRevision else { return }
        lastNostrSignerRevision = revision
        cachedNostrSigner = projection
        resolveWaiters(projection.operations)
    }
}
