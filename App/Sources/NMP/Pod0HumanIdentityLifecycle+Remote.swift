#if canImport(NMP)
import NMP

extension Pod0HumanIdentityLifecycle {
    func awaitReady(
        _ connection: NMPNip46Connection,
        expectedPublicKey: String?
    ) async throws -> String {
        for await connectionState in connection.states {
            switch connectionState {
            case .authorizationRequired(let url):
                state = .authorizationRequired(url)
            case .ready(let publicKey):
                if let expectedPublicKey, publicKey != expectedPublicKey {
                    try mismatch(expected: expectedPublicKey, actual: publicKey)
                }
                return publicKey
            case .failed(let failure):
                throw Pod0HumanIdentityError.remoteFailure(String(describing: failure))
            case .connecting, .available, .unavailable, .relayAuthentication, .connected:
                continue
            }
        }
        throw Pod0HumanIdentityError.remoteConnectionEndedBeforeReady
    }

    func mismatch(expected: String, actual: String) throws -> Never {
        try block(.expectedPublicKeyMismatch(expected: expected, actual: actual))
    }

    func block(_ reason: Pod0IdentityBlocker) throws -> Never {
        blocker = reason
        state = .blocked(reason)
        switch reason {
        case .clientInitiatedNip46CheckpointUnsupported(let issue):
            throw Pod0HumanIdentityError.remoteFailure(
                "Scan-to-connect is unavailable until NMP issue #\(issue) supports secure restoration."
            )
        case .restoredLocalDetachUnsupported(let issue):
            throw Pod0HumanIdentityError.restoredLocalDetachUnsupported(issue: issue)
        case .orphanedRestoredLocal(let issue):
            throw Pod0HumanIdentityError.remoteFailure(
                "NMP restored a local account without a matching Pod0 catalog entry. It was made inactive, and identity changes are blocked until issue #\(issue) supports exact detachment."
            )
        case .identitySwitchUnsupported:
            throw Pod0HumanIdentityError.identitySwitchUnsupported
        case .expectedPublicKeyMismatch(let expected, let actual):
            throw Pod0HumanIdentityError.expectedPublicKeyMismatch(
                expected: expected,
                actual: actual
            )
        }
    }
}
#endif
