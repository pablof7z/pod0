import Foundation

// MARK: - CommentTarget
//
// What a comment is anchored to. Episodes use the Podcasting 2.0
// `<podcast:guid>` form so Nostr clients across the ecosystem (Fountain,
// generic NIP-22 readers) can address the same episode by the same key.
// Clips are Podcastr-specific — no global addressing scheme exists for
// user-authored excerpts, so we mint a custom scheme that's stable inside
// our own surfaces and intentionally inert outside them.
//
// NIP-73 (External Content IDs) governs the `i` tag value format; both
// schemes below are wire-compatible with NIP-22 comment events.
enum CommentTarget: Hashable, Sendable {
    case episode(guid: String)
    case clip(id: UUID)

    /// NIP-73 external content identifier. Set as the value of the `i`
    /// (and `I` for root) tag on the NIP-22 comment event.
    var nip73Identifier: String {
        switch self {
        case .episode(let guid):
            return "podcast:item:guid:\(guid)"
        case .clip(let id):
            return "podcastr:clip:\(id.uuidString.lowercased())"
        }
    }

    /// Protocol scheme used as the `k` (and `K` for root) tag. Mirrors
    /// the `i` value's `protocol:type` prefix per NIP-73 examples.
    var nip73Kind: String {
        switch self {
        case .episode: return "podcast:item:guid"
        case .clip:    return "podcastr:clip"
        }
    }
}

// MARK: - EpisodeComment
//
// One observed comment on an episode or clip, normalized from the wire
// `kind:1111` event. Field naming intentionally matches the on-wire form
// (no Swift-style renames) so the wire ↔ domain mapping is one-to-one.
//
// `authorPubkeyHex` is the 32-byte x-only hex; the UI truncates it for
// display until a profile-fetch path lands in a later batch.
struct EpisodeComment: Identifiable, Hashable, Sendable {
    /// Nostr event id (sha256 of the canonical event JSON). Stable across
    /// re-observations from the same or other relays.
    let id: String
    let target: CommentTarget
    let authorPubkeyHex: String
    let content: String
    let createdAt: Date

    /// Truncated `npub:` display form used by the row until a profile
    /// fetch / NIP-05 verifier lands. "abcd…wxyz" — first and last 4 of
    /// the hex pubkey is enough disambiguation for an MVP.
    var authorShortKey: String {
        guard authorPubkeyHex.count > 8 else { return authorPubkeyHex }
        return "\(authorPubkeyHex.prefix(4))…\(authorPubkeyHex.suffix(4))"
    }
}
