import Foundation

struct Pod0NMPDiagnosticsSnapshot: Sendable, Equatable {
    struct Relay: Sendable, Equatable, Identifiable {
        let id: String
        let relay: String
        let access: String
        let wireSubscriptionCount: UInt32
        let laneCounts: [String: UInt32]
        let receivedEventCounts: [UInt16: UInt64]
        let scopedCoverageFacts: Int
    }

    struct AuthSession: Sendable, Equatable {
        let relay: String
        let access: String
        let phase: String
        let capabilityBound: Bool
        let signerBound: Bool
    }

    let configuration: Pod0NMPConfiguration
    let relays: [Relay]
    let authSessions: [AuthSession]
    let uncoveredAuthorCount: UInt32
    let transportDegraded: String?
    let identityBlocker: Pod0IdentityBlocker?

    /// Deliberately scoped wording: EOSE and coverage rows are facts about
    /// exact filters/sessions, never a claim that "Nostr is fully synced."
    var supportSummary: String {
        let relayCount = relays.count
        let coverageCount = relays.reduce(0) { $0 + $1.scopedCoverageFacts }
        return "NMP configured for \(configuration.limits.maxRelays) relays; \(relayCount) active diagnostic rows and \(coverageCount) scoped coverage facts."
    }
}
