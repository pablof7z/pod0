import Foundation
import Observation
import Pod0Core

/// Native presentation state over the Rust-owned durable conversation.
/// Commands and projections are typed; this object owns no durable decisions.
@MainActor
@Observable
final class SharedAgentConversationSession {
    enum Phase: Equatable {
        case idle
        case running
        case failed(String)
    }

    static let productProofTools: [AgentToolName] = [
        .createNote,
        .listSubscriptions,
        .listPodcasts,
        .listEpisodes,
        .listInProgress,
        .listRecentUnplayed,
        .searchEpisodes,
        .pausePlayback,
        .setPlaybackRate,
    ]

    private let runtime: any SharedAgentConversationRuntime
    private let modelReference: @MainActor () -> String
    private var subscriber: SharedAgentConversationSubscriber?
    private var subscriptionID: SubscriptionId?
    private var subscribedConversationID: ConversationId?
    private(set) var conversation: AgentConversationProjection?
    private(set) var stateRevision: UInt64 = 0
    private(set) var phase: Phase = .idle

    init(
        runtime: any SharedAgentConversationRuntime,
        modelReference: @escaping @MainActor () -> String
    ) {
        self.runtime = runtime
        self.modelReference = modelReference
    }

    var conversationID: ConversationId? { conversation?.conversationId }

    var turns: [AgentTurnProjection] { conversation?.turns ?? [] }

    var messages: [AgentMessageProjection] {
        turns.reversed().flatMap(\.messages)
    }

    var activeTurn: AgentTurnProjection? {
        turns.first { !$0.stage.isTerminal }
    }

    var canSend: Bool {
        activeTurn == nil && phase != .running
    }

    func startTurn(_ userInput: String) async {
        let input = userInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !input.isEmpty, canSend else { return }
        phase = .running
        do {
            let result = try await runtime.execute(.startAgentTurn(
                conversationId: conversationID,
                userInput: input,
                modelReference: modelReference(),
                availableTools: Self.productProofTools
            ))
            guard case .agentTurnStarted(let conversationID, _) = result else {
                phase = .failed("The agent turn did not start")
                return
            }
            subscribe(to: conversationID)
            runtime.executePendingHostRequests()
        } catch {
            phase = .failed("The agent turn could not start")
        }
    }

    func cancelActiveTurn() async {
        guard let activeTurn else { return }
        do {
            _ = try await runtime.execute(.cancelAgentTurn(
                turnId: activeTurn.turnId,
                expectedTurnRevision: activeTurn.revision
            ))
            runtime.executePendingHostRequests()
        } catch {
            phase = .failed("The agent turn could not be cancelled")
        }
    }

    func openConversation(_ conversationID: ConversationId) {
        subscribe(to: conversationID)
    }

    func startNewConversation() {
        stopObserving()
        conversation = nil
        stateRevision = 0
        phase = .idle
    }

    func stopObserving() {
        if let subscriptionID {
            runtime.unsubscribeAgentConversation(subscriptionID)
        }
        subscriptionID = nil
        subscribedConversationID = nil
        subscriber = nil
    }

    private func subscribe(to conversationID: ConversationId) {
        if self.conversationID == conversationID, subscriptionID != nil { return }
        stopObserving()
        let subscriber = SharedAgentConversationSubscriber { [weak self] envelope in
            Task { @MainActor [weak self] in self?.receive(envelope) }
        }
        self.subscriber = subscriber
        subscribedConversationID = conversationID
        subscriptionID = runtime.subscribeAgentConversation(
            conversationID,
            subscriber: subscriber
        )
    }

    private func receive(_ envelope: ProjectionEnvelope) {
        guard envelope.stateRevision.value >= stateRevision,
              case .agentConversation(let conversation) = envelope.projection,
              conversation.conversationId == subscribedConversationID else { return }
        stateRevision = envelope.stateRevision.value
        self.conversation = conversation
        if conversation.failure != nil {
            phase = .failed("The agent conversation is unavailable")
        } else if conversation.turns.contains(where: { !$0.stage.isTerminal }) {
            phase = .running
        } else if let latest = conversation.turns.first,
                  latest.stage.isFailure {
            phase = .failed(latest.safeFailure ?? "The agent turn failed")
        } else {
            phase = .idle
        }
        runtime.executePendingHostRequests()
    }
}

private extension AgentTurnStage {
    var isTerminal: Bool {
        switch self {
        case .awaitingModel, .approvalRequired, .authorized, .executing, .commitPending:
            false
        case .committed, .completed, .denied, .cancelled, .blocked,
             .outcomeAmbiguous, .failed:
            true
        }
    }

    var isFailure: Bool {
        switch self {
        case .blocked, .outcomeAmbiguous, .failed:
            true
        default:
            false
        }
    }
}
