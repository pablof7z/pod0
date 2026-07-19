import Foundation

enum ProductSignalName: String, Codable, CaseIterable, Sendable {
    case appLaunch
    case firstSubscription
    case playStarted
    case meaningfulListening
    case resumeAttempt
    case playbackError
    case transcriptReady
    case transcriptUsed
    case recallAsked
    case recallGrounded
    case recallCitationOpened
    case recallShadowParity
    case noteCreated
    case clipCreated
    case agentTurnCompleted
    case uncleanTermination
    case dataLossEvidence
}

enum ProductSignalOutcome: String, Codable, CaseIterable, Sendable {
    case started
    case succeeded
    case failed
    case created
    case ready
    case used
    case grounded
    case noEvidence
    case opened
    case detected
    case cancelled
    case matched
    case mismatched
}

enum ProductSignalLatencyBucket: String, Codable, CaseIterable, Sendable {
    case under250Milliseconds
    case milliseconds250To749
    case milliseconds750To1999
    case seconds2To4
    case seconds5Plus

    static func bucket(_ duration: Duration) -> ProductSignalLatencyBucket {
        let milliseconds = duration.components.seconds * 1_000
            + duration.components.attoseconds / 1_000_000_000_000_000
        return switch milliseconds {
        case ..<250: .under250Milliseconds
        case ..<750: .milliseconds250To749
        case ..<2_000: .milliseconds750To1999
        case ..<5_000: .seconds2To4
        default: .seconds5Plus
        }
    }
}

struct ProductSignalObservation: Sendable, Equatable {
    let signalID: UUID
    let occurredAt: Date
    let name: ProductSignalName
    let outcome: ProductSignalOutcome
    let latencyBucket: ProductSignalLatencyBucket?
    let errorClass: ProductFailureCode?
    let domainRevision: UInt64?

    init(
        signalID: UUID = UUID(),
        occurredAt: Date = Date(),
        name: ProductSignalName,
        outcome: ProductSignalOutcome,
        latencyBucket: ProductSignalLatencyBucket? = nil,
        errorClass: ProductFailureCode? = nil,
        domainRevision: UInt64? = nil
    ) {
        self.signalID = signalID
        self.occurredAt = occurredAt
        self.name = name
        self.outcome = outcome
        self.latencyBucket = latencyBucket
        self.errorClass = errorClass
        self.domainRevision = domainRevision
    }

    static func once(
        name: ProductSignalName,
        subjectID: UUID,
        outcome: ProductSignalOutcome,
        occurredAt: Date = Date(),
        domainRevision: UInt64? = nil
    ) -> ProductSignalObservation {
        ProductSignalObservation(
            signalID: OccurrenceIdentity.uuid(
                for: "product-signal:\(name.rawValue):\(subjectID.uuidString)"
            ),
            occurredAt: occurredAt,
            name: name,
            outcome: outcome,
            domainRevision: domainRevision
        )
    }
}

struct ProductSignal: Identifiable, Codable, Sendable, Equatable {
    static let currentSchemaVersion = 1

    let schemaVersion: Int
    let id: UUID
    let anonymousInstallID: UUID
    let occurredAt: Date
    let name: ProductSignalName
    let outcome: ProductSignalOutcome
    let latencyBucket: ProductSignalLatencyBucket?
    let errorClass: ProductFailureCode?
    let domainRevision: UInt64?

    init(observation: ProductSignalObservation, anonymousInstallID: UUID) {
        schemaVersion = Self.currentSchemaVersion
        id = observation.signalID
        self.anonymousInstallID = anonymousInstallID
        occurredAt = observation.occurredAt
        name = observation.name
        outcome = observation.outcome
        latencyBucket = observation.latencyBucket
        errorClass = observation.errorClass
        domainRevision = observation.domainRevision
    }
}

protocol ProductSignalSink: Sendable {
    func record(_ observation: ProductSignalObservation) async
    func deleteAll() async
}

struct DiscardingProductSignalSink: ProductSignalSink {
    static let shared = DiscardingProductSignalSink()
    func record(_ observation: ProductSignalObservation) async {}
    func deleteAll() async {}
}
