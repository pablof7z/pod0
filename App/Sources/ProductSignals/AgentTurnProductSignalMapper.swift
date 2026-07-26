import Foundation
import Pod0Core

enum AgentTurnProductSignalMapper {
    static func observations(
        for turn: AgentTurnProjection,
        latencyBucket: ProductSignalLatencyBucket?
    ) -> [ProductSignalObservation] {
        guard let turnID = turn.turnId.uuid else { return [] }
        let occurredAt = turn.updatedAt.date
        let revision = turn.revision.value
        var observations: [ProductSignalObservation] = []

        guard turn.proposal?.action.isTranscriptQuery == true else {
            if turn.stage.isSuccessful {
                observations.append(.once(
                    name: .agentTurnCompleted,
                    subjectID: turnID,
                    outcome: .succeeded,
                    occurredAt: occurredAt,
                    domainRevision: revision
                ))
            }
            return observations
        }

        observations.append(.once(
            name: .recallAsked,
            subjectID: turnID,
            outcome: .started,
            occurredAt: occurredAt,
            domainRevision: revision
        ))
        if let outcome = turn.stage.recallOutcome(
            hasEvidence: !turn.recallEvidence.isEmpty
        ) {
            observations.append(ProductSignalObservation(
                signalID: OccurrenceIdentity.uuid(
                    for: "product-signal:\(ProductSignalName.recallGrounded.rawValue):\(turnID)"
                ),
                occurredAt: occurredAt,
                name: .recallGrounded,
                outcome: outcome,
                latencyBucket: latencyBucket,
                domainRevision: revision
            ))
        }
        if turn.stage.isSuccessful {
            observations.append(.once(
                name: .agentTurnCompleted,
                subjectID: turnID,
                outcome: .succeeded,
                occurredAt: occurredAt,
                domainRevision: revision
            ))
        }
        return observations
    }
}

private extension AgentToolAction {
    var isTranscriptQuery: Bool {
        if case .queryTranscripts = self { return true }
        return false
    }
}

private extension AgentTurnStage {
    var isSuccessful: Bool {
        switch self {
        case .committed, .completed: true
        default: false
        }
    }

    func recallOutcome(hasEvidence: Bool) -> ProductSignalOutcome? {
        switch self {
        case .committed, .completed: hasEvidence ? .grounded : .noEvidence
        case .denied, .cancelled: .cancelled
        case .blocked, .outcomeAmbiguous, .failed: .failed
        case .awaitingModel, .approvalRequired, .authorized, .executing,
             .commitPending: nil
        }
    }
}
