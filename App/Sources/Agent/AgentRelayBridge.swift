import Foundation
import os.log

@MainActor
final class AgentRelayBridge {
    private let logger = Logger.app("AgentRelayBridge")
    private let store: AppStateStore
    /// Live podcast-tool dependencies. Nil only for callers that don't have a
    /// `PlaybackState` handy (Nostr-only headless flows); podcast tool calls
    /// then return a typed error envelope rather than crashing.
    private let podcastDeps: PodcastAgentToolDeps?
    /// Owner-consultation coordinator. Weakly held — the coordinator lives at
    /// `AppMain` scope and outlives any single inbound peer-agent reply. When
    /// nil (cold launches before the UI is up, headless tests), the `ask` tool
    /// returns a typed error envelope and the peer-agent loop continues.
    private weak var askCoordinator: AgentAskCoordinator?
    /// Matches `AgentChatSession.maxTurns`. Multi-step tool chains (e.g. a
    /// peer asking the agent to compile a wiki page or generate a podcast
    /// episode) routinely use 6–12 turns; the previous 8-turn cap tripped
    /// mid-chain. 20 is the same ceiling the in-app chat uses.
    private let maxTurns = 20

    init(
        store: AppStateStore,
        playback: PlaybackState? = nil,
        askCoordinator: AgentAskCoordinator? = nil
    ) {
        self.store = store
        self.podcastDeps = playback.map { LivePodcastAgentToolDeps.make(store: store, playback: $0) }
        self.askCoordinator = askCoordinator
    }

    /// Variant init that takes a pre-built `PodcastAgentToolDeps` directly.
    /// Used by `NostrAgentResponder`, which gets its `PlaybackState` via a
    /// late-bound closure provider (RootView wires it post-construction).
    init(
        store: AppStateStore,
        podcastDeps: PodcastAgentToolDeps?,
        askCoordinator: AgentAskCoordinator?
    ) {
        self.store = store
        self.podcastDeps = podcastDeps
        self.askCoordinator = askCoordinator
    }

    /// Thread-aware entrypoint used by the Nostr responder. Accepts a
    /// pre-built conversation history (with the `[from <label> (npub1…)]:`
    /// identity prefixes the responder already applies) and the peer's
    /// pubkey so we can compose a peer-context preamble in front of the
    /// owner-voice `AgentPrompt.build` payload.
    ///
    /// Why two prompt sections rather than one: the owner inventory in
    /// `AgentPrompt.build` is owner-flavoured ("Subscriptions", "In
    /// Progress"). Without a preamble explaining that `role:user` is a
    /// peer-not-owner, the model anchors on owner-voice and reads the
    /// preamble as override-mid-stream. The peer-identity block has to
    /// come first.
    ///
    /// Tools fire on the owner's behalf at the peer's request — this is
    /// intentional. Owner consent is granted via the `Allow` flow in
    /// `NostrApprovalSheet`; peers that haven't been Allowed never reach
    /// this code path.
    func reply(
        messages history: [[String: Any]],
        peerPubkey: String,
        rootEventID: String? = nil,
        inboundEventID: String? = nil
    ) async -> String? {
        guard !history.isEmpty else { return nil }
        let reference = LLMModelReference(storedID: store.state.settings.agentInitialModel)
        guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
            logger.warning("No \(reference.provider.displayName, privacy: .public) key for Nostr peer reply")
            return nil
        }
        var isUpgraded = false
        var enabledSkills: Set<String> = []

        // Build a PeerConversationContext when we have enough identifying
        // information so that tool dispatches can tag generated episodes
        // with their source conversation and send reply events correctly.
        let peerContext: PeerConversationContext?
        if let root = rootEventID, let inbound = inboundEventID {
            peerContext = PeerConversationContext(
                rootEventID: root,
                inboundEventID: inbound,
                peerPubkeyHex: peerPubkey
            )
        } else {
            peerContext = nil
        }

        let preamble = NostrPeerAgentPrompt.peerContextPreamble(
            for: store,
            peerPubkey: peerPubkey
        )
        let ownerPrompt = AgentPrompt.build(for: store.state)
        let systemPrompt = preamble + "\n\n" + ownerPrompt

        var messages: [[String: Any]] = [["role": "system", "content": systemPrompt]]
        messages.append(contentsOf: history)

        return await runTurnLoop(
            messages: &messages,
            isUpgraded: &isUpgraded,
            enabledSkills: &enabledSkills,
            peerContext: peerContext,
            source: .nostrInbound,
            initialInput: (history.last?["content"] as? String) ?? "",
            systemPrompt: systemPrompt
        )
    }

    func reply(to content: String, from senderPubkey: String) async -> String? {
        let trimmed = content.trimmed
        guard !trimmed.isEmpty else { return nil }

        let reference = LLMModelReference(storedID: store.state.settings.agentInitialModel)
        guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
            logger.warning("No \(reference.provider.displayName, privacy: .public) key available for Nostr agent reply")
            return nil
        }
        var isUpgraded = false
        var enabledSkills: Set<String> = []

        let senderName = displayName(for: senderPubkey)
        let systemPrompt = AgentPrompt.build(for: store.state)
        let userText = "[from \(senderName) via Nostr]\n\(trimmed)"
        var messages: [[String: Any]] = [
            ["role": "system", "content": systemPrompt],
            ["role": "user", "content": userText],
        ]
        return await runTurnLoop(
            messages: &messages,
            isUpgraded: &isUpgraded,
            enabledSkills: &enabledSkills,
            source: .nostrInbound,
            initialInput: userText,
            systemPrompt: systemPrompt
        )
    }

    /// Shared per-call turn loop. Both `reply(to:from:)` (single-message
    /// legacy path) and `reply(messages:peerPubkey:)` (thread-aware path
    /// used by `NostrAgentResponder`) feed into this loop. Mutates the
    /// passed `messages`/`isUpgraded`/`enabledSkills` so a caller could
    /// peek at them after the loop if it ever needed to.
    private func runTurnLoop(
        messages: inout [[String: Any]],
        isUpgraded: inout Bool,
        enabledSkills: inout Set<String>,
        peerContext: PeerConversationContext? = nil,
        source: AgentRunSource,
        initialInput: String,
        systemPrompt: String
    ) async -> String? {
        let batchID = UUID()
        let collector = AgentRunCollector(
            id: batchID,
            source: source,
            initialInput: initialInput,
            systemPrompt: systemPrompt
        )
        var turnNumber = 0

        for _ in 0..<maxTurns {
            let messagesBeforeCall = messages
            let result: AgentResult
            let modelForTurn = isUpgraded
                ? store.state.settings.agentThinkingModel
                : store.state.settings.agentInitialModel
            do {
                result = try await AgentLLMClient.streamCompletion(
                    messages: messages,
                    tools: AgentTools.schema
                         + AgentTools.podcastSchema
                         + (peerContext != nil ? AgentTools.peerOnlySchema : [])
                         + AgentSkillRegistry.schemas(for: enabledSkills),
                    model: modelForTurn,
                    feature: CostFeature.agentNostr,
                    onPartialContent: { _ in }
                )
            } catch {
                logger.error("Nostr agent turn failed: \(error, privacy: .public)")
                collector.appendTurn(
                    turnNumber: turnNumber,
                    messagesBeforeCall: messagesBeforeCall,
                    apiResponse: nil,
                    toolDispatches: []
                )
                collector.finish(outcome: .failed, failureReason: error.localizedDescription)
                return nil
            }

            messages.append(result.assistantMessage)
            let apiResponse = makeAPIResponse(from: result)

            if result.toolCalls.isEmpty {
                collector.appendTurn(
                    turnNumber: turnNumber,
                    messagesBeforeCall: messagesBeforeCall,
                    apiResponse: apiResponse,
                    toolDispatches: []
                )
                collector.finish(outcome: .completed)
                let text = (result.assistantMessage["content"] as? String)?.trimmed ?? ""
                return text.isEmpty ? nil : text
            }

            var toolDispatches: [AgentToolDispatch] = []
            for toolCall in result.toolCalls {
                let resultJSON: String
                if toolCall.name == AgentTools.Names.upgradeThinking {
                    isUpgraded = true
                    resultJSON = AgentTools.toolSuccess([
                        "upgraded": true,
                        "model": store.state.settings.agentThinkingModel,
                    ])
                } else if toolCall.name == AgentTools.Names.useSkill {
                    let activation = AgentSkillRegistry.activate(
                        argsJSON: toolCall.arguments,
                        currentEnabledSkills: enabledSkills
                    )
                    resultJSON = activation.resultJSON
                    enabledSkills = activation.updatedEnabledSkills
                } else {
                    let effectiveDeps = peerContext.map { podcastDeps?.withPeerContext($0) } ?? podcastDeps
                    resultJSON = await AgentTools.dispatch(
                        name: toolCall.name,
                        argsJSON: toolCall.arguments,
                        store: store,
                        batchID: batchID,
                        podcastDeps: effectiveDeps,
                        enabledSkills: enabledSkills,
                        askCoordinator: askCoordinator
                    )
                }
                messages.append([
                    "role": "tool",
                    "tool_call_id": toolCall.id,
                    "content": resultJSON,
                ])
                toolDispatches.append(makeToolDispatch(call: toolCall, resultJSON: resultJSON))
            }
            collector.appendTurn(
                turnNumber: turnNumber,
                messagesBeforeCall: messagesBeforeCall,
                apiResponse: apiResponse,
                toolDispatches: toolDispatches
            )
            turnNumber += 1
        }

        collector.finish(outcome: .turnsExhausted)
        logger.warning("Nostr agent turn reached max turn limit")
        return nil
    }

    private func makeAPIResponse(from result: AgentResult) -> AgentAPIResponse {
        let runToolCalls: [AgentRunToolCall] = result.toolCalls.map { call in
            let parsedArgs = (try? JSONSerialization.jsonObject(with: Data(call.arguments.utf8)) as? [String: Any]) ?? [:]
            return AgentRunToolCall(id: call.id, name: call.name, arguments: parsedArgs)
        }
        let usage = result.tokensUsed ?? AgentTokenUsage(promptTokens: 0, completionTokens: 0, cachedTokens: nil)
        return AgentAPIResponse(
            assistantMessage: result.assistantMessage,
            toolCalls: runToolCalls,
            tokensUsed: usage
        )
    }

    private func makeToolDispatch(call: AgentToolCall, resultJSON: String) -> AgentToolDispatch {
        let argsDict = (try? JSONSerialization.jsonObject(with: Data(call.arguments.utf8)) as? [String: Any]) ?? [:]
        let resultDict: [String: Any]
        if let parsed = try? JSONSerialization.jsonObject(with: Data(resultJSON.utf8)) as? [String: Any] {
            resultDict = parsed
        } else {
            resultDict = ["raw": resultJSON]
        }
        let errorMessage = resultDict["error"] as? String
        return AgentToolDispatch(
            toolCallID: call.id,
            toolName: call.name,
            arguments: argsDict,
            result: resultDict,
            error: errorMessage
        )
    }

    private func displayName(for pubkey: String) -> String {
        store.friend(identifier: pubkey)?.displayName ?? "Nostr contact \(String(pubkey.prefix(8)))"
    }
}
