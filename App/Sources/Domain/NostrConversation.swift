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
    /// Returns true if input looks like a Nostr private key (nsec1 prefix).
    /// Callers must reject these immediately with a user-visible warning —
    /// private keys must never be routed to any search or discovery handler.
    static func looksLikeNsecKey(_ input: String) -> Bool {
        input.trimmingCharacters(in: .whitespacesAndNewlines).lowercased().hasPrefix("nsec1")
    }

    /// Quick check if input looks like a public Nostr identifier or NIP-05 address
    /// (no FFI, pure prefix detection). Used by AddByURLForm and NostrDiscoverForm
    /// to route inputs to the kernel's open_search handler instead of RSS fallback.
    ///
    /// Does NOT match nsec1 (private keys). Call `looksLikeNsecKey` separately
    /// to guard against accidental private-key submission.
    ///
    /// Issue #605: eliminates ad-hoc string checks scattered across iOS.
    static func looksLikeNostrInput(_ input: String) -> Bool {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        let normalized = trimmed.lowercased()
        // Public Nostr identifiers: npub1, nevent1
        // BACKLOG: Parse nprofile1 TLVs to extract embedded pubkey for Nostr subscribe (#605)
        // nprofile1 is excluded until TLV parsing is implemented — pubkeyHex(from:) only
        // handles bare hex and npub1, so nprofile1 inputs silently failed to subscribe.
        if normalized.starts(with: "npub1") ||
           normalized.starts(with: "nevent1") {
            return true
        }
        // Raw 64-character hex pubkey (issue #605)
        let hexRegex = try? NSRegularExpression(pattern: "^[0-9a-fA-F]{64}$")
        if hexRegex?.firstMatch(in: trimmed, range: NSRange(trimmed.startIndex..., in: trimmed)) != nil {
            return true
        }
        // NIP-05 address: must be exactly localpart@domain with no path/query/scheme
        // characters. A URL containing @ (e.g. feeds.example.com/users/alice@example.com/rss)
        // must NOT be classified as NIP-05 — it will be handled by SubscriptionService. (issue #605)
        let nip05Regex = try? NSRegularExpression(pattern: "^[^@/\\?#:]+@[^@/\\?#:]+\\.[^@/\\?#:]+$")
        if nip05Regex?.firstMatch(in: trimmed, range: NSRange(trimmed.startIndex..., in: trimmed)) != nil {
            return true
        }
        return false
    }

    static func pubkeyHex(from input: String) -> String? {
        input.withCString { ptr in
            guard let result = nmp_app_podcast_parse_pubkey(ptr) else { return nil }
            defer { nmp_free_string(result) }
            let envelope = String(cString: result)
            guard let data = envelope.data(using: .utf8),
                  let decoded = try? JSONDecoder().decode(PubkeyResponse.self, from: data),
                  let pubkeyHex = decoded.pubkeyHex,
                  !pubkeyHex.isEmpty
            else { return nil }
            return pubkeyHex
        }
    }

    /// Encodes a hex pubkey as a full `npub1…` bech32 string. Returns the
    /// raw hex on failure so callers always have something to render.
    static func encode(fromHex hex: String) -> String {
        hex.withCString { ptr in
            guard let result = nmp_app_podcast_npub_from_hex(ptr) else { return hex }
            defer { nmp_free_string(result) }
            let envelope = String(cString: result)
            guard let data = envelope.data(using: .utf8),
                  let decoded = try? JSONDecoder().decode(NpubResponse.self, from: data),
                  let npub = decoded.npub,
                  !npub.isEmpty
            else { return hex }
            return npub
        }
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

private struct NpubResponse: Decodable {
    let npub: String?
}

private struct PubkeyResponse: Decodable {
    private enum CodingKeys: String, CodingKey {
        case pubkeyHex = "pubkey_hex"
    }

    let pubkeyHex: String?
}
