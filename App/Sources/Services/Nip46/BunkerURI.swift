import Foundation

/// Parsed `bunker://` connection URI per NIP-46.
///
/// Example: `bunker://7e7e9c4...?relay=wss://relay.nsec.app&relay=wss://relay.damus.io&secret=abc123`
struct BunkerURI: Sendable, Equatable {
    /// 32-byte (64 hex chars) lowercase x-only pubkey of the remote signer.
    let remotePubkeyHex: String
    /// One or more relay WebSocket URLs to use for the JSON-RPC channel.
    let relays: [String]
    /// Optional connect secret the bunker requires us to echo back in the `connect` request.
    let secret: String?

    /// Comma-separated permissions list (e.g. `sign_event:1,nip44_encrypt`). Optional, ignored if absent.
    let permissions: [String]

    static let scheme = "bunker"

    /// Parse `bunker://<pubkey>?relay=...&relay=...&secret=...` strictly.
    static func parse(_ raw: String) throws -> BunkerURI {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let prefix = "\(scheme)://"
        guard let schemeRange = trimmed.range(of: prefix), schemeRange.lowerBound == trimmed.startIndex else {
            throw BunkerURIError.invalidScheme
        }
        let body = String(trimmed[schemeRange.upperBound...])
        let pubkeyPart: String
        let queryPart: String
        if let q = body.firstIndex(of: "?") {
            pubkeyPart = String(body[..<q]).lowercased()
            queryPart = String(body[body.index(after: q)...])
        } else {
            pubkeyPart = body.lowercased()
            queryPart = ""
        }
        guard isLowerHex(pubkeyPart, length: 64) else { throw BunkerURIError.invalidRemotePubkey }

        var relays: [String] = []
        var secret: String?
        var perms: [String] = []
        if !queryPart.isEmpty {
            for pair in queryPart.split(separator: "&", omittingEmptySubsequences: true) {
                let kv = pair.split(separator: "=", maxSplits: 1, omittingEmptySubsequences: false)
                guard kv.count == 2 else { continue }
                let key = String(kv[0])
                guard let value = String(kv[1]).removingPercentEncoding else { continue }
                switch key {
                case "relay":
                    if !value.isEmpty { relays.append(value) }
                case "secret":
                    secret = value.isEmpty ? nil : value
                case "perms":
                    perms = value.split(separator: ",", omittingEmptySubsequences: true).map(String.init)
                default:
                    continue // ignore unknown keys (forward-compatible)
                }
            }
        }
        guard !relays.isEmpty else { throw BunkerURIError.missingRelay }
        for r in relays {
            guard r.hasPrefix("ws://") || r.hasPrefix("wss://") else {
                throw BunkerURIError.invalidRelayURL(r)
            }
        }
        return BunkerURI(remotePubkeyHex: pubkeyPart, relays: relays, secret: secret, permissions: perms)
    }

    private static func isLowerHex(_ s: String, length: Int) -> Bool {
        guard s.count == length else { return false }
        return s.allSatisfy { ($0 >= "0" && $0 <= "9") || ($0 >= "a" && $0 <= "f") }
    }
}

enum BunkerURIError: LocalizedError, Equatable {
    case invalidScheme
    case invalidRemotePubkey
    case missingRelay
    case invalidRelayURL(String)

    var errorDescription: String? {
        switch self {
        case .invalidScheme: "Not a bunker:// URI."
        case .invalidRemotePubkey: "Bunker URI is missing a 64-hex-char remote pubkey."
        case .missingRelay: "Bunker URI must include at least one relay (?relay=wss://…)."
        case .invalidRelayURL(let r): "Bunker URI relay '\(r)' is not a ws:// or wss:// URL."
        }
    }
}
