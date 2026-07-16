import Foundation

struct NMPMigrationRecordV1: Sendable, Codable, Equatable {
    enum Phase: String, Sendable, Codable, CaseIterable, Comparable {
        case prepared
        case nmpQualified
        case cutover
        case cleanupComplete

        static func < (lhs: Phase, rhs: Phase) -> Bool {
            guard let left = allCases.firstIndex(of: lhs), let right = allCases.firstIndex(of: rhs) else {
                return false
            }
            return left < right
        }
    }

    static let schemaVersion = 1

    let schemaVersion: Int
    var phase: Phase
    let pinnedNMPCommit: String
    let sourceStateDigest: String
    let legacyArchiveDigest: String
    let legacyArchivePath: String
    let nmpStoreGeneration: UUID
    let nmpStorePath: String
    let expectedPublicKeysByRole: [String: String]
    let failClosedLegacyIngress: Bool
    let preparedAt: Date
    var updatedAt: Date

    init(
        phase: Phase = .prepared,
        pinnedNMPCommit: String,
        sourceStateDigest: String,
        legacyArchiveDigest: String,
        legacyArchivePath: String,
        nmpStoreGeneration: UUID,
        nmpStorePath: String,
        expectedPublicKeysByRole: [Pod0IdentityRole: String],
        failClosedLegacyIngress: Bool,
        now: Date
    ) {
        schemaVersion = Self.schemaVersion
        self.phase = phase
        self.pinnedNMPCommit = pinnedNMPCommit
        self.sourceStateDigest = sourceStateDigest
        self.legacyArchiveDigest = legacyArchiveDigest
        self.legacyArchivePath = legacyArchivePath
        self.nmpStoreGeneration = nmpStoreGeneration
        self.nmpStorePath = nmpStorePath
        let mappedPublicKeys = Dictionary(uniqueKeysWithValues: expectedPublicKeysByRole.map {
            ($0.key.rawValue, $0.value)
        })
        self.expectedPublicKeysByRole = mappedPublicKeys
        self.failClosedLegacyIngress = failClosedLegacyIngress
        preparedAt = now
        updatedAt = now
    }
}

enum LegacyNostrDeletionMilestone: String, Sendable, Codable {
    case profilesAndTrustSurfaces = "M3"
    case remoteAgentConversations = "M4"
    case podcastLifecycle = "M5"
    case finalLegacyRemoval = "M6"
}

/// Read-only preservation of legacy facts that cannot be trusted as protocol
/// authority. This type has no method that inserts, signs, routes, authorizes,
/// or republishes an event.
struct LegacyNostrQuarantineV1: Sendable, Codable {
    static let schemaVersion = 1

    let schemaVersion: Int
    let archivedAt: Date
    let pendingApprovals: [NostrPendingApproval]
    let pendingApprovalsDeleteAt: LegacyNostrDeletionMilestone
    let pendingFriendMessages: [PendingFriendMessage]
    let pendingFriendMessagesDeleteAt: LegacyNostrDeletionMilestone
    let conversations: [NostrConversationRecord]
    let conversationsDeleteAt: LegacyNostrDeletionMilestone
    let respondedEventIDs: Set<String>
    let respondedEventIDsDeleteAt: LegacyNostrDeletionMilestone
    let profileCache: [String: NostrProfileMetadata]
    let profileCacheDeleteAt: LegacyNostrDeletionMilestone
    let discoveredRelayURLs: [String]
    let discoveredRelayURLsDeleteAt: LegacyNostrDeletionMilestone
    let excludedProtocolState: [String]
    let excludedProtocolStateDeleteAt: LegacyNostrDeletionMilestone

    init(state: AppState, now: Date) {
        schemaVersion = Self.schemaVersion
        archivedAt = now
        pendingApprovals = state.nostrPendingApprovals
        pendingApprovalsDeleteAt = .profilesAndTrustSurfaces
        pendingFriendMessages = state.pendingFriendMessages
        pendingFriendMessagesDeleteAt = .remoteAgentConversations
        conversations = state.nostrConversations
        conversationsDeleteAt = .remoteAgentConversations
        respondedEventIDs = state.nostrRespondedEventIDs
        respondedEventIDsDeleteAt = .remoteAgentConversations
        profileCache = state.nostrProfileCache
        profileCacheDeleteAt = .profilesAndTrustSurfaces
        discoveredRelayURLs = state.settings.nostrPublicRelays
        discoveredRelayURLsDeleteAt = .podcastLifecycle
        excludedProtocolState = [
            "nostrSinceCursor",
            "commentSeenIDs",
            "rawEventAuthority",
            "unverifiedReplacementWinners",
            "previousPublishMarkers",
        ]
        excludedProtocolStateDeleteAt = .finalLegacyRemoval
    }
}
