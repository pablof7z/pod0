import Foundation

// MARK: - Nostr conversation history

/// One persisted record per conversation root the agent has participated in.
/// Used by Settings > Agent > Conversations and for rendering transcripts.
struct NostrConversationRecord: Codable, Identifiable, Hashable, Sendable {
    /// Conversation root event id (NIP-10) — primary key.
    var id: String { rootEventID }
    var rootEventID: String
    /// Pubkey we are conversing with (the original counterparty).
    var counterpartyPubkey: String
    var firstSeen: Date
    var lastTouched: Date
    /// Ordered transcript of every event we have ingested or published in this thread.
    var turns: [NostrConversationTurn]
}

struct NostrConversationTurn: Codable, Hashable, Sendable {
    enum Direction: String, Codable, Hashable, Sendable {
        case incoming
        case outgoing
    }
    var eventID: String
    var direction: Direction
    var pubkey: String
    var createdAt: Date
    var content: String
    /// Full Nostr event JSON for export. Optional for backwards compatibility.
    var rawEventJSON: String?
}

// MARK: - Nostr profile metadata (kind:0)

/// Cached kind:0 profile metadata for a counterparty pubkey. Source is
/// the raw JSON content of a kind:0 event published by that pubkey;
/// only the fields the conversations UI actually renders are decoded
/// here. All fields are optional because the kind:0 spec is informal —
/// publishers may include any subset.
struct NostrProfileMetadata: Codable, Equatable, Hashable, Sendable {
    var pubkey: String
    var name: String?
    var displayName: String?
    var about: String?
    var picture: String?
    var nip05: String?
    /// Unix timestamp of the kind:0 event we sourced this from. Used to
    /// avoid downgrading to an older profile if multiple kind:0s arrive.
    var fetchedFromCreatedAt: Int

    /// Best label to render for this profile. Falls back through
    /// display_name → name → nil so callers can decide on a final
    /// fallback (e.g. truncated npub).
    var bestLabel: String? {
        if let dn = displayName?.trimmingCharacters(in: .whitespacesAndNewlines), !dn.isEmpty {
            return dn
        }
        if let n = name?.trimmingCharacters(in: .whitespacesAndNewlines), !n.isEmpty {
            return n
        }
        return nil
    }

    var pictureURL: URL? {
        guard let raw = picture?.trimmingCharacters(in: .whitespacesAndNewlines), !raw.isEmpty else {
            return nil
        }
        guard let url = URL(string: raw), let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else {
            return nil
        }
        return url
    }
}

// MARK: - NIP-10 root extraction

enum NostrConversationRoot {
    /// Returns the NIP-10 conversation root for an event. Prefers explicit
    /// `["e", id, relay, "root"]` markers; falls back to the first e-tag;
    /// otherwise the event id itself is the root.
    static func rootEventID(eventID: String, tags: [[String]]) -> String {
        var firstETag: String?
        for tag in tags {
            guard tag.count >= 2, tag[0] == "e" else { continue }
            let id = tag[1]
            if tag.count >= 4, tag[3] == "root", !id.isEmpty {
                return id
            }
            if firstETag == nil, !id.isEmpty {
                firstETag = id
            }
        }
        return firstETag ?? eventID
    }
}

// MARK: - Bech32 npub helpers

enum NostrNpub {
    /// Encodes a hex pubkey as a full `npub1…` bech32 string. Returns the
    /// raw hex on failure so callers always have something to render.
    static func encode(fromHex hex: String) -> String {
        guard let data = Data(hexString: hex) else { return hex }
        return Bech32.encode(hrp: "npub", data: data)
    }

    /// Shortened display form: `npub1abcdefghij…uvwxyz`. Used in
    /// conversation rows and approval sheets when the full bech32 is too
    /// long. Falls back to the input when bech32 encoding fails.
    static func shortNpub(fromHex hex: String) -> String {
        let full = encode(fromHex: hex)
        guard full.count > 16 else { return full }
        return "\(full.prefix(10))…\(full.suffix(6))"
    }
}
