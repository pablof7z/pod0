import Foundation

enum AgentRunSource: String, Codable, Sendable {
    case typedChat
    case voiceMessage
    case nostrInbound
    case manual
    case scheduledTask
}

enum AgentRunOutcome: String, Codable, Sendable {
    case completed
    case turnsExhausted
    case failed
    case cancelled
}

struct AgentTokenUsage: Codable, Sendable {
    let promptTokens: Int
    let completionTokens: Int
    let cachedTokens: Int?
}

struct AgentRunToolCall: Codable, Sendable {
    let id: String
    let name: String
    let arguments: [String: AnyCodable]

    init(id: String, name: String, arguments: [String: Any]) {
        self.id = id
        self.name = name
        self.arguments = arguments.mapValues { AnyCodable($0) }
    }
}

struct AgentToolDispatch: Codable, Sendable {
    let toolCallID: String
    let toolName: String
    let arguments: [String: AnyCodable]
    let result: [String: AnyCodable]
    let error: String?

    init(
        toolCallID: String,
        toolName: String,
        arguments: [String: Any],
        result: [String: Any]? = nil,
        error: String? = nil
    ) {
        self.toolCallID = toolCallID
        self.toolName = toolName
        self.arguments = arguments.mapValues { AnyCodable($0) }
        self.result = (result ?? [:]).mapValues { AnyCodable($0) }
        self.error = error
    }
}

struct AgentAPIResponse: Codable, Sendable {
    let assistantMessage: [String: AnyCodable]
    let toolCalls: [AgentRunToolCall]
    let tokensUsed: AgentTokenUsage

    init(assistantMessage: [String: Any], toolCalls: [AgentRunToolCall], tokensUsed: AgentTokenUsage) {
        self.assistantMessage = assistantMessage.mapValues { AnyCodable($0) }
        self.toolCalls = toolCalls
        self.tokensUsed = tokensUsed
    }
}

struct AgentRunTurnData: Codable, Identifiable, Sendable {
    let id: UUID
    let turnNumber: Int
    let messagesBeforeCall: [[String: AnyCodable]]
    let apiResponse: AgentAPIResponse?
    let toolDispatches: [AgentToolDispatch]

    init(
        id: UUID = UUID(),
        turnNumber: Int,
        messagesBeforeCall: [[String: Any]],
        apiResponse: AgentAPIResponse?,
        toolDispatches: [AgentToolDispatch]
    ) {
        self.id = id
        self.turnNumber = turnNumber
        self.messagesBeforeCall = messagesBeforeCall.map { dict in
            dict.mapValues { AnyCodable($0) }
        }
        self.apiResponse = apiResponse
        self.toolDispatches = toolDispatches
    }
}

struct AgentRun: Codable, Identifiable, Sendable {
    let id: UUID
    let timestamp: Date
    let source: AgentRunSource
    let initialInput: String
    let systemPrompt: String
    let turns: [AgentRunTurnData]
    let finalOutcome: AgentRunOutcome
    let totalTokensUsed: Int
    let durationMs: Int
    let failureReason: String?

    private enum CodingKeys: String, CodingKey {
        case id, timestamp, source, initialInput, systemPrompt
        case turns, finalOutcome, totalTokensUsed, durationMs, failureReason
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        self.id = try c.decode(UUID.self, forKey: .id)
        self.timestamp = try c.decode(Date.self, forKey: .timestamp)
        self.source = try c.decode(AgentRunSource.self, forKey: .source)
        self.initialInput = try c.decode(String.self, forKey: .initialInput)
        self.systemPrompt = try c.decode(String.self, forKey: .systemPrompt)
        self.turns = try c.decode([AgentRunTurnData].self, forKey: .turns)
        self.finalOutcome = try c.decode(AgentRunOutcome.self, forKey: .finalOutcome)
        self.totalTokensUsed = try c.decode(Int.self, forKey: .totalTokensUsed)
        self.durationMs = try c.decode(Int.self, forKey: .durationMs)
        self.failureReason = try c.decodeIfPresent(String.self, forKey: .failureReason)
    }

    init(
        id: UUID,
        timestamp: Date,
        source: AgentRunSource,
        initialInput: String,
        systemPrompt: String,
        turns: [AgentRunTurnData],
        finalOutcome: AgentRunOutcome,
        totalTokensUsed: Int,
        durationMs: Int,
        failureReason: String? = nil
    ) {
        self.id = id
        self.timestamp = timestamp
        self.source = source
        self.initialInput = initialInput
        self.systemPrompt = systemPrompt
        self.turns = turns
        self.finalOutcome = finalOutcome
        self.totalTokensUsed = totalTokensUsed
        self.durationMs = durationMs
        self.failureReason = failureReason
    }
}

enum AnyCodable: Codable, Sendable {
    case null
    case bool(Bool)
    case int(Int)
    case double(Double)
    case string(String)
    case array([AnyCodable])
    case object([String: AnyCodable])

    init(_ value: Any) {
        if value is NSNull {
            self = .null
        } else if let bool = value as? Bool {
            self = .bool(bool)
        } else if let int = value as? Int {
            self = .int(int)
        } else if let double = value as? Double {
            self = .double(double)
        } else if let string = value as? String {
            self = .string(string)
        } else if let array = value as? [Any] {
            self = .array(array.map(AnyCodable.init))
        } else if let dict = value as? [String: Any] {
            self = .object(dict.mapValues(AnyCodable.init))
        } else {
            self = .null
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null:
            try container.encodeNil()
        case .bool(let bool):
            try container.encode(bool)
        case .int(let int):
            try container.encode(int)
        case .double(let double):
            try container.encode(double)
        case .string(let string):
            try container.encode(string)
        case .array(let array):
            try container.encode(array)
        case .object(let dict):
            try container.encode(dict)
        }
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let bool = try? container.decode(Bool.self) {
            self = .bool(bool)
        } else if let int = try? container.decode(Int.self) {
            self = .int(int)
        } else if let double = try? container.decode(Double.self) {
            self = .double(double)
        } else if let string = try? container.decode(String.self) {
            self = .string(string)
        } else if let array = try? container.decode([AnyCodable].self) {
            self = .array(array)
        } else if let dict = try? container.decode([String: AnyCodable].self) {
            self = .object(dict)
        } else {
            self = .null
        }
    }
}
