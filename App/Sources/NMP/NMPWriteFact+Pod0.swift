import NMP
import Pod0Core

extension PublicationStatusObservation {
    static let nmpAccepted = value(.accepted)

    static func nmpFailure(_ detail: String) -> Self {
        value(.failed, detail: detail)
    }

    static let nmpReattachmentNotFound = value(.reattachmentNotFound)
    static let nmpReattachmentUnreadable = value(.reattachmentUnreadable)

    fileprivate static func value(
        _ kind: PublicationFactKind,
        attempt: UInt64? = nil,
        eventID: String? = nil,
        observedAt: UInt64? = nil,
        detail: String? = nil
    ) -> Self {
        Self(
            kind: kind,
            routeId: nil,
            attempt: attempt,
            eventIdHex: eventID,
            observedAt: observedAt.map {
                UnixTimestampMilliseconds(
                    value: Int64(min($0, UInt64(Int64.max / 1_000))) * 1_000
                )
            },
            detail: detail.map { String($0.prefix(512)) }
        )
    }
}

extension WriteFact {
    var pod0Observations: [PublicationStatusObservation] {
        let value: (PublicationFactKind, UInt64?, String?, UInt64?, String?)?
        switch self {
        case .signing(.awaitingSigner(let pubkey)):
            value = (.awaitingCapability, nil, nil, nil, pubkey)
        case .signing(.inFlight), .outcome(.settled):
            value = nil
        case .signing(.signed(let eventID)):
            value = (.signed, nil, eventID, nil, nil)
        case .signing(.refused(let reason)):
            value = (.failed, nil, nil, nil, reason)
        case .destinations(let relays, _, _):
            return relays.isEmpty ? [] : [.value(.routed)]
        case .relay(_, .waiting(.notConnected)):
            value = (.awaitingRelay, nil, nil, nil, nil)
        case .relay(_, .waiting(.needsAuth)):
            value = (.awaitingAuth, nil, nil, nil, nil)
        case .relay(_, .waiting(.backingOff(let attempt, let at, _, _))):
            value = (.retryEligible, attempt, nil, at, nil)
        case .relay(_, .waiting(.persistenceStalled(let detail))):
            value = (.persistenceBlocked, nil, nil, nil, detail)
        case .relay(_, .sent(let attempt, let at)):
            value = (.sent, attempt, nil, at, nil)
        case .relay(_, .published):
            value = (.acknowledged, nil, nil, nil, nil)
        case .relay(_, .rejected(let reason)):
            value = (.rejected, nil, nil, nil, reason)
        case .relay(_, .authFailed(_, _, let reason)):
            value = (.rejected, nil, nil, nil, reason)
        case .relay(_, .gaveUp):
            value = (.gaveUp, nil, nil, nil, nil)
        case .outcome(.noDestination):
            value = (.failed, nil, nil, nil, "NMP found no destination")
        case .outcome(.notSent(.cancelled)):
            value = (.cancelled, nil, nil, nil, nil)
        case .outcome(.notSent(.superseded)):
            value = (.cancelled, nil, nil, nil, "Superseded by a newer NMP write")
        case .outcome(.refused(.replaceableBaseChanged)),
             .outcome(.refused(.replaceableBaseOnRegularEvent)):
            value = (.replaceableConflict, nil, nil, nil, "NMP refused the replacement")
        case .outcome(.refused(.alreadyExpired)):
            value = (.failed, nil, nil, nil, "NMP refused an expired write")
        case .outcome(.refused(.tombstoned)):
            value = (.rejected, nil, nil, nil, "NMP refused a tombstoned write")
        }
        guard let value else { return [] }
        return [.value(
            value.0,
            attempt: value.1,
            eventID: value.2,
            observedAt: value.3,
            detail: value.4
        )]
    }
}
