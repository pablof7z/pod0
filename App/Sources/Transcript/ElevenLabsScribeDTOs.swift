import Foundation

struct ElevenLabsScribeIntent: Encodable {
    let audioURL: String
    let languageHint: String?

    enum CodingKeys: String, CodingKey {
        case audioURL = "audio_url"
        case languageHint = "language_hint"
    }
}

struct ElevenLabsScribeEnvelope: Decodable {
    var result: ScribeRawResult?
    var error: ElevenLabsScribeBackendError?
}

struct ElevenLabsScribeBackendError: Decodable {
    var kind: String
    var message: String?
    var statusCode: Int?

    enum CodingKeys: String, CodingKey {
        case kind, message
        case statusCode = "status_code"
    }
}

struct ScribeJob: Sendable, Hashable {
    let requestID: String
    let episodeID: UUID
    let createdAt: Date
    let languageHint: String?
    let inlineResult: ScribeRawResult?
}

struct ScribeRawResult: Codable, Sendable, Hashable {
    let language_code: String?
    let text: String?
    let words: [ScribeWord]?
    let model: String?
    let duration: Double?
    let latencyMs: Int?

    enum CodingKeys: String, CodingKey {
        case language_code, text, words, model, duration
        case latencyMs = "latency_ms"
    }
}

struct ScribeWord: Codable, Sendable, Hashable {
    let text: String
    let start: Double
    let end: Double
    let type: String?
    let speaker_id: String?
}
