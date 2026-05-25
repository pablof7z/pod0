// Compat shim — replaced when domain projections land in nmp-app-podcast.
//
// These domain types are referenced by the migrated Identity / Onboarding /
// Settings views. The legacy implementations live under `App/Sources/Domain/`
// and `App/Sources/Podcast/`. For M1.E we copy enough of the shape to compile
// — full behaviour returns when the matching projection module ships.

import Foundation

// MARK: - Podcast

/// Compat stub — replaced by the podcast-core projection in M2.
struct Podcast: Identifiable, Hashable, Sendable {
    var id: UUID = UUID()
    var title: String = ""
    var author: String = ""
    var description: String = ""
    var feedURL: URL?
    var imageURL: URL?
    var lastRefreshedAt: Date?
}

// MARK: - Nostr conversation history

/// Compat shim — replaced when Agent (Nostr conversations) projection lands.
struct NostrConversationRecord: Codable, Identifiable, Hashable, Sendable {
    var id: String { rootEventID }
    var rootEventID: String
    var counterpartyPubkey: String
    var firstSeen: Date
    var lastTouched: Date
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
    var rawEventJSON: String?
}

// MARK: - Nostr profile metadata (kind:0)

/// Compat shim — replaced when the kind:0 profile cache lands.
struct NostrProfileMetadata: Codable, Equatable, Hashable, Sendable {
    var pubkey: String
    var name: String?
    var displayName: String?
    var about: String?
    var picture: String?
    var nip05: String?
    var fetchedFromCreatedAt: Int

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

// MARK: - Nostr pending approval

/// Compat shim — replaced when trust-this-user projection lands.
struct NostrPendingApproval: Codable, Identifiable, Hashable, Sendable {
    var id: UUID
    var pubkeyHex: String
    var displayName: String?
    var about: String?
    var pictureURL: String?
    var receivedAt: Date
    var content: String?

    init(
        pubkeyHex: String,
        displayName: String? = nil,
        about: String? = nil,
        pictureURL: String? = nil,
        content: String? = nil
    ) {
        self.id = UUID()
        self.pubkeyHex = pubkeyHex
        self.displayName = displayName
        self.about = about
        self.pictureURL = pictureURL
        self.receivedAt = Date()
        self.content = content
    }
}

// MARK: - Nostr npub helpers

enum NostrNpub {
    /// Compat shim: returns the hex unchanged. Replaced when Bech32 lands as
    /// part of the nmp-keys integration.
    static func encode(fromHex hex: String) -> String {
        guard let data = Data(hexString: hex) else { return hex }
        return Bech32.encode(hrp: "npub", data: data)
    }

    static func shortNpub(fromHex hex: String) -> String {
        let full = encode(fromHex: hex)
        guard full.count > 16 else { return full }
        return "\(full.prefix(10))…\(full.suffix(6))"
    }
}
