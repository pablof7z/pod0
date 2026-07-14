import Foundation

@MainActor
final class AgentRunCollector {
    let id: UUID
    let startTime: Date
    let source: AgentRunSource
    let initialInput: String
    let systemPrompt: String

    private var turns: [AgentRunTurnData] = []
    private var totalTokensUsed: Int = 0
    private var finalised: Bool = false

    init(
        id: UUID,
        source: AgentRunSource,
        initialInput: String,
        systemPrompt: String,
        startTime: Date = Date()
    ) {
        self.id = id
        self.startTime = startTime
        self.source = source
        self.initialInput = initialInput
        self.systemPrompt = systemPrompt
    }

    func appendTurn(
        turnNumber: Int,
        messagesBeforeCall: [[String: Any]],
        apiResponse: AgentAPIResponse?,
        toolDispatches: [AgentToolDispatch]
    ) {
        turns.append(AgentRunTurnData(
            turnNumber: turnNumber,
            messagesBeforeCall: messagesBeforeCall,
            apiResponse: apiResponse,
            toolDispatches: toolDispatches
        ))
        if let usage = apiResponse?.tokensUsed {
            totalTokensUsed += usage.promptTokens + usage.completionTokens
        }
    }

    func finish(outcome: AgentRunOutcome, failureReason: String? = nil) {
        guard !finalised else { return }
        finalised = true
        let durationMs = Int(Date().timeIntervalSince(startTime) * 1000)
        let run = AgentRun(
            id: id,
            timestamp: startTime,
            source: source,
            initialInput: initialInput,
            systemPrompt: systemPrompt,
            turns: turns,
            finalOutcome: outcome,
            totalTokensUsed: totalTokensUsed,
            durationMs: durationMs,
            failureReason: failureReason
        )
        AgentRunLogger.shared.log(run: run)
    }
}
