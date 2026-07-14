import Foundation

/// JSON-RPC envelope used for NIP-46 messages, wire-encoded as the `content` field of a
/// kind:24133 event after NIP-44 v2 encryption.
///
/// Outbound shape:  `{"id":"<request-id>","method":"<verb>","params":[...string args...]}`
/// Inbound shape:   `{"id":"<request-id>","result":"<value>","error":"<optional msg>"}`
enum Nip46Method: String, Sendable {
    case connect
    case getPublicKey = "get_public_key"
    case signEvent = "sign_event"
    case ping
    // Optional, not used for MVP:
    case nip04Encrypt = "nip04_encrypt"
    case nip04Decrypt = "nip04_decrypt"
    case nip44Encrypt = "nip44_encrypt"
    case nip44Decrypt = "nip44_decrypt"
}

struct Nip46Request: Sendable, Equatable {
    let id: String
    let method: String
    let params: [String]

    init(id: String = UUID().uuidString, method: Nip46Method, params: [String] = []) {
        self.id = id
        self.method = method.rawValue
        self.params = params
    }

    init(id: String, method: String, params: [String]) {
        self.id = id
        self.method = method
        self.params = params
    }

    func encode() throws -> String {
        let obj: [String: Any] = ["id": id, "method": method, "params": params]
        let data = try JSONSerialization.data(withJSONObject: obj, options: [])
        guard let s = String(data: data, encoding: .utf8) else {
            throw Nip46FramingError.encodingFailed
        }
        return s
    }
}

struct Nip46Response: Sendable, Equatable {
    let id: String
    /// Non-nil on success. May be the literal string `"ack"` (e.g. `connect`).
    let result: String?
    /// Non-nil on failure.
    let error: String?

    static func parse(_ text: String) throws -> Nip46Response {
        guard let data = text.data(using: .utf8),
              let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw Nip46FramingError.malformedJSON
        }
        guard let id = json["id"] as? String else { throw Nip46FramingError.missingID }
        let result = json["result"] as? String
        let error = json["error"] as? String
        return Nip46Response(id: id, result: result, error: error)
    }
}

enum Nip46FramingError: LocalizedError {
    case encodingFailed
    case malformedJSON
    case missingID

    var errorDescription: String? {
        switch self {
        case .encodingFailed: "Could not serialize NIP-46 request to JSON."
        case .malformedJSON: "NIP-46 response is not valid JSON."
        case .missingID: "NIP-46 response is missing an `id`."
        }
    }
}
