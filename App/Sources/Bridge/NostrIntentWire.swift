import Foundation

// MARK: - NMP input-intent wire types

/// Registered NMP input scope id, serialized as `{"namespace":"...","name":"..."}`.
struct NostrIntentScope: Codable, Equatable {
    let namespace: String
    let name: String

    static let nostrRef = NostrIntentScope(namespace: "nostr", name: "ref")
    static let nip50Profiles = NostrIntentScope(namespace: "nip50", name: "profiles")
    static let nip50Notes = NostrIntentScope(namespace: "nip50", name: "notes")
}

enum NostrIntentTextTargets: Encodable, Equatable {
    case userPreferred

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .userPreferred:
            try container.encode("UserPreferred")
        }
    }
}

struct NostrIntentRequest: Encodable, Equatable {
    let input: String
    let scopes: [NostrIntentScope]
    let textTargets: NostrIntentTextTargets

    private enum CodingKeys: String, CodingKey {
        case input
        case scopes
        case textTargets = "text_targets"
    }

    func jsonString() -> String? {
        guard let data = try? JSONEncoder().encode(self) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}

struct NostrIntentClassificationEnvelope: Decodable, Equatable {
    let ok: Bool
    let classification: NostrIntentClassification?
    let error: String?

    static func decode(json: String) -> NostrIntentClassificationEnvelope? {
        guard let data = json.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(Self.self, from: data)
    }
}

enum NostrIntentClassification: Decodable, Equatable {
    case candidates([NostrIntentCandidate])
    case rejection(NostrIntentRejection)

    private enum CodingKeys: String, CodingKey {
        case Candidates
        case Rejection
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if let candidates = try container.decodeIfPresent(
            [NostrIntentCandidate].self,
            forKey: .Candidates
        ) {
            self = .candidates(candidates)
            return
        }
        if let rejection = try container.decodeIfPresent(
            NostrIntentRejection.self,
            forKey: .Rejection
        ) {
            self = .rejection(rejection)
            return
        }
        throw DecodingError.dataCorrupted(
            .init(codingPath: decoder.codingPath, debugDescription: "unknown intent classification")
        )
    }
}

struct NostrIntentCandidate: Decodable, Equatable {
    let scope: NostrIntentScope?
    let target: NostrIntentTarget
}

enum NostrIntentDispatchOutcome: Equatable {
    case dispatched(NostrIntentTarget)
    case rejection(NostrIntentRejection)

    static func decode(json: String) -> NostrIntentDispatchOutcome? {
        guard let data = json.data(using: .utf8),
              let envelope = try? JSONDecoder().decode(Envelope.self, from: data),
              envelope.ok else { return nil }
        if let candidate = envelope.dispatched {
            return .dispatched(candidate.target)
        }
        if let rejection = envelope.rejection {
            return .rejection(rejection)
        }
        return nil
    }

    private struct Envelope: Decodable {
        let ok: Bool
        let dispatched: NostrIntentCandidate?
        let rejection: NostrIntentRejection?
    }
}

enum NostrIntentTarget: Decodable, Equatable {
    case directRef(uri: String)
    case nip05(identifier: String)
    case relayURL(url: String)
    case textQuery(requestJSON: String)
    case registered

    private enum CodingKeys: String, CodingKey {
        case DirectRef
        case Nip05
        case RelayUrl
        case TextQuery
        case Registered
    }

    private struct DirectRefBody: Decodable { let uri: String }
    private struct Nip05Body: Decodable { let identifier: String }
    private struct RelayURLBody: Decodable { let url: String }
    private struct TextQueryBody: Decodable {
        let requestJSON: String

        private enum CodingKeys: String, CodingKey {
            case requestJSON = "request_json"
        }
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if let body = try container.decodeIfPresent(DirectRefBody.self, forKey: .DirectRef) {
            self = .directRef(uri: body.uri)
        } else if let body = try container.decodeIfPresent(Nip05Body.self, forKey: .Nip05) {
            self = .nip05(identifier: body.identifier)
        } else if let body = try container.decodeIfPresent(RelayURLBody.self, forKey: .RelayUrl) {
            self = .relayURL(url: body.url)
        } else if let body = try container.decodeIfPresent(TextQueryBody.self, forKey: .TextQuery) {
            self = .textQuery(requestJSON: body.requestJSON)
        } else if container.contains(.Registered) {
            self = .registered
        } else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: decoder.codingPath, debugDescription: "unknown intent target")
            )
        }
    }
}

struct NostrSearchResultsSnapshot: Codable, Equatable {
    var hits: [NostrSearchHit] = []
}

struct NostrSearchHit: Codable, Equatable, Identifiable {
    var id: String = ""
    var author: String = ""
    var kind: UInt32 = 0
    var createdAt: UInt64 = 0
    var content: String = ""
    var tags: [[String]] = []
    var relayProvenance: [String] = []
    var source: NostrSearchHitSource = .cache

    var profileMetadata: NostrProfileSearchMetadata? {
        guard kind == 0, let data = content.data(using: .utf8) else { return nil }
        return try? JSONDecoder().decode(NostrProfileSearchMetadata.self, from: data)
    }

    var displayName: String {
        let metadata = profileMetadata
        if let display = metadata?.displayName?.trimmed, !display.isEmpty { return display }
        if let name = metadata?.name?.trimmed, !name.isEmpty { return name }
        return NostrNpub.shortNpub(fromHex: author)
    }

    var detail: String {
        if let about = profileMetadata?.about?.trimmed, !about.isEmpty { return about }
        return source.label
    }
}

struct NostrProfileSearchMetadata: Codable, Equatable {
    var name: String?
    var displayName: String?
    var about: String?
    var picture: String?

    private enum CodingKeys: String, CodingKey {
        case name
        case displayName = "display_name"
        case about
        case picture
    }
}

enum NostrSearchHitSource: Codable, Equatable {
    case cache
    case relay(String)

    var label: String {
        switch self {
        case .cache:
            return "Local cache"
        case .relay(let relay):
            return relay.isEmpty ? "Relay" : relay
        }
    }

    init(from decoder: Decoder) throws {
        if let value = try? decoder.singleValueContainer().decode(String.self),
           value == "Cache" {
            self = .cache
            return
        }
        let container = try decoder.container(keyedBy: CodingKeys.self)
        if let relay = try container.decodeIfPresent(String.self, forKey: .Relay) {
            self = .relay(relay)
        } else if container.contains(.Cache) {
            self = .cache
        } else {
            self = .cache
        }
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .cache:
            try container.encode("Cache")
        case .relay(let relay):
            try container.encode(["Relay": relay])
        }
    }

    private enum CodingKeys: String, CodingKey {
        case Cache
        case Relay
    }
}

enum NostrSearchProjection {
    static let keyPrefix = "nmp.nip50.search."

    static func decodeSessions(from data: Data) -> [String: NostrSearchResultsSnapshot] {
        guard let raw = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let value = raw["v"] as? [String: Any],
              let projections = value["projections"] as? [String: Any]
        else { return [:] }

        let decoder = KernelDecoding.makeDecoder()
        var sessions: [String: NostrSearchResultsSnapshot] = [:]
        for (key, object) in projections where key.hasPrefix(keyPrefix) {
            guard let bytes = try? JSONSerialization.data(withJSONObject: object),
                  let decoded = try? decoder.decode(NostrSearchResultsSnapshot.self, from: bytes)
            else { continue }
            let sessionID = String(key.dropFirst(keyPrefix.count))
            sessions[sessionID] = decoded
        }
        return sessions
    }
}

enum NostrIntentRejection: Decodable, Equatable {
    case secretLike
    case unparseable
    case unregisteredScope
    case disallowedScope

    private enum ObjectKeys: String, CodingKey {
        case UnregisteredScope
        case DisallowedScope
    }

    init(from decoder: Decoder) throws {
        if let single = try? decoder.singleValueContainer(),
           let value = try? single.decode(String.self) {
            switch value {
            case "SecretLike":
                self = .secretLike
                return
            case "Unparseable":
                self = .unparseable
                return
            default:
                break
            }
        }
        let container = try decoder.container(keyedBy: ObjectKeys.self)
        if container.contains(.UnregisteredScope) {
            self = .unregisteredScope
        } else if container.contains(.DisallowedScope) {
            self = .disallowedScope
        } else {
            self = .unparseable
        }
    }
}

enum DecodedNostrRefTarget: Equatable {
    case profile(pubkey: String)
    case event(eventID: String)
    case address(pubkey: String)

    static func decode(json: String) -> DecodedNostrRefTarget? {
        guard let data = json.data(using: .utf8),
              let envelope = try? JSONDecoder().decode(Envelope.self, from: data),
              envelope.ok else { return nil }
        switch envelope.target {
        case "profile":
            guard let pubkey = envelope.pubkey else { return nil }
            return .profile(pubkey: pubkey)
        case "event":
            guard let eventID = envelope.eventID else { return nil }
            return .event(eventID: eventID)
        case "address":
            guard let pubkey = envelope.pubkey else { return nil }
            return .address(pubkey: pubkey)
        default:
            return nil
        }
    }

    private struct Envelope: Decodable {
        let ok: Bool
        let target: String?
        let pubkey: String?
        let eventID: String?

        private enum CodingKeys: String, CodingKey {
            case ok
            case target
            case pubkey
            case eventID = "event_id"
        }
    }
}
