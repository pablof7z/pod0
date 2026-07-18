import Foundation

struct VNInputAudioChunk: Encodable {
    var messageType = "input_audio_chunk"
    var audioBase64: String
    var sampleRate: Int

    enum CodingKeys: String, CodingKey {
        case messageType = "message_type"
        case audioBase64 = "audio_base_64"
        case sampleRate = "sample_rate"
    }
}

struct VNSingleUseTokenResponse: Decodable {
    var token: String
}

struct VNRealtimeEvent: Decodable {
    var messageType: String
    var text: String?
    var error: String?
    var message: String?
    var detail: String?

    var errorMessage: String {
        error ?? message ?? detail ?? "Realtime transcription failed."
    }

    enum CodingKeys: String, CodingKey {
        case messageType = "message_type"
        case text, error, message, detail
    }
}
