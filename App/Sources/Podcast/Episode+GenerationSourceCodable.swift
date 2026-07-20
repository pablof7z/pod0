import Foundation

extension Episode.GenerationSource: Codable {
    private enum CodingKeys: String, CodingKey {
        case type, conversationID
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "inAppChat":
            let id = try c.decode(UUID.self, forKey: .conversationID)
            self = .inAppChat(conversationID: id)
        default:
            throw DecodingError.dataCorrupted(.init(
                codingPath: [CodingKeys.type],
                debugDescription: "Unknown GenerationSource type: \(type)"
            ))
        }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .inAppChat(let conversationID):
            try c.encode("inAppChat", forKey: .type)
            try c.encode(conversationID, forKey: .conversationID)
        }
    }
}
