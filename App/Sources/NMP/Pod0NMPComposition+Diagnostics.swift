#if canImport(NMP)
import NMP

extension Pod0NMPComposition {
    func diagnostics(
        identityBlocker: Pod0IdentityBlocker? = nil
    ) throws -> AsyncStream<Pod0NMPDiagnosticsSnapshot> {
        let upstream = try engine.observeDiagnostics()
        let configuration = configuration
        return AsyncStream(bufferingPolicy: .bufferingNewest(1)) { continuation in
            let task = Task {
                for await snapshot in upstream {
                    continuation.yield(Self.map(
                        snapshot,
                        configuration: configuration,
                        identityBlocker: identityBlocker
                    ))
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in
                task.cancel()
                upstream.cancel()
            }
        }
    }

    private static func map(
        _ snapshot: DiagnosticsSnapshot,
        configuration: Pod0NMPConfiguration,
        identityBlocker: Pod0IdentityBlocker?
    ) -> Pod0NMPDiagnosticsSnapshot {
        Pod0NMPDiagnosticsSnapshot(
            configuration: configuration,
            relays: snapshot.relays.map { relay in
                Pod0NMPDiagnosticsSnapshot.Relay(
                    id: relay.id,
                    relay: relay.relay,
                    access: accessLabel(relay.access),
                    wireSubscriptionCount: relay.wireSubCount,
                    laneCounts: Dictionary(uniqueKeysWithValues: relay.byLane.map { ($0.lane, $0.count) }),
                    receivedEventCounts: Dictionary(uniqueKeysWithValues: relay.eventsByKind.map { ($0.kind, $0.count) }),
                    scopedCoverageFacts: relay.coverage.filter { $0.coverage != nil }.count
                )
            },
            authSessions: snapshot.authSessions.map { auth in
                Pod0NMPDiagnosticsSnapshot.AuthSession(
                    relay: auth.relay,
                    access: accessLabel(auth.access),
                    phase: String(describing: auth.phase),
                    capabilityBound: auth.policyBound,
                    signerBound: auth.signerBound
                )
            },
            uncoveredAuthorCount: snapshot.uncoveredAuthorCount,
            transportDegraded: snapshot.transportDegraded,
            identityBlocker: identityBlocker
        )
    }

    private static func accessLabel(_ access: NMPAccessContext) -> String {
        switch access {
        case .public:
            return "public"
        case .nip42(let publicKey):
            return "nip42:\(publicKey)"
        }
    }
}
#endif
