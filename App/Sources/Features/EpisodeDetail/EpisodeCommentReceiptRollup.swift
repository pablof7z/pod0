import Foundation

/// Pod0's product-level receipt presentation. Per-relay facts remain distinct
/// so one ACK can mean posted without erasing failures or pending routes.
struct EpisodeCommentReceiptRollup {
    private enum RelayState {
        case routed
        case awaitingRelay
        case awaitingAuth
        case retryEligible
        case handoffAmbiguous
        case sent
        case acknowledged
        case rejected(String)
        case gaveUp
        case persistenceBlocked
        case routePersistenceBlocked
        case outcomeUnknown

        var isTerminal: Bool {
            switch self {
            case .acknowledged, .rejected, .gaveUp, .persistenceBlocked,
                 .routePersistenceBlocked, .outcomeUnknown:
                true
            default:
                false
            }
        }
    }

    var eventID: String?
    private var accepted = false
    private var relays: [String: RelayState] = [:]
    private var latest: EpisodeCommentWriteStatus?

    init(eventID: String? = nil) {
        self.eventID = eventID
    }

    mutating func apply(_ status: EpisodeCommentWriteStatus) {
        latest = status
        switch status {
        case .accepted: accepted = true
        case .signed(let id): eventID = id
        case .routed(let routed):
            for relay in routed where relays[relay] == nil { relays[relay] = .routed }
        case .awaitingRelay(let relay): relays[relay] = .awaitingRelay
        case .awaitingAuth(let relay): relays[relay] = .awaitingAuth
        case .retryEligible(let relay, _): relays[relay] = .retryEligible
        case .handoffAmbiguous(let relay): relays[relay] = .handoffAmbiguous
        case .sent(let relay): relays[relay] = .sent
        case .acknowledged(let relay): relays[relay] = .acknowledged
        case .rejected(let relay, let reason): relays[relay] = .rejected(reason)
        case .gaveUp(let relay): relays[relay] = .gaveUp
        case .persistenceBlocked(let relay): relays[relay] = .persistenceBlocked
        case .routePersistenceBlocked(let relay): relays[relay] = .routePersistenceBlocked
        case .outcomeUnknown(let relay): relays[relay] = .outcomeUnknown
        default: break
        }
    }

    func phase(streamEnded: Bool) -> OutgoingEpisodeCommentPhase {
        let confirmed = relays.values.filter { state in
            if case .acknowledged = state { return true }
            return false
        }.count
        let unconfirmed = relays.values.filter(\.isTerminal).count - confirmed
        let pending = relays.count - confirmed - unconfirmed
        if confirmed > 0 {
            return .published(
                confirmedRelayCount: confirmed,
                unconfirmedRelayCount: unconfirmed,
                pendingRelayCount: pending
            )
        }
        if !relays.isEmpty, pending == 0, let terminal = terminalFailure() {
            return terminal
        }
        if streamEnded {
            return .deliveryUnknown(
                accepted
                    ? "Delivery ended without relay confirmation."
                    : "Delivery ended before durable acceptance was confirmed; posting remains locked to prevent a duplicate."
            )
        }
        guard let latest else { return .queued }
        switch latest {
        case .accepted: return .queued
        case .awaitingCapability: return .awaitingCapability
        case .signed: return .signed
        case .awaitingRelay: return .awaitingRelay
        case .sent, .handoffAmbiguous: return .awaitingConfirmation
        case .awaitingAuth: return .awaitingRelayAuthorization
        case .retryEligible(_, let eligibleAt): return .retrying(eligibleAt: eligibleAt)
        case .rejected, .gaveUp, .persistenceBlocked, .routePersistenceBlocked,
             .outcomeUnknown:
            return .delivering
        case .failed(let reason): return .failed(reason)
        default: return .delivering
        }
    }

    private func terminalFailure() -> OutgoingEpisodeCommentPhase? {
        if let blocked = relays.first(where: { entry in
            switch entry.value {
            case .persistenceBlocked, .routePersistenceBlocked: true
            default: false
            }
        }) {
            return .persistenceBlocked(blocked.key)
        }
        if let unknown = relays.first(where: { entry in
            if case .outcomeUnknown = entry.value { return true }
            return false
        }) {
            return .deliveryUnknown(unknown.key)
        }
        if let gaveUp = relays.first(where: { entry in
            if case .gaveUp = entry.value { return true }
            return false
        }) {
            return .gaveUp(gaveUp.key)
        }
        if let rejected = relays.first(where: { entry in
            if case .rejected = entry.value { return true }
            return false
        }), case .rejected(let reason) = rejected.value {
            return .rejected("\(rejected.key): \(reason)")
        }
        return nil
    }
}
