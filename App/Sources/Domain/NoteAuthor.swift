import Foundation

// MARK: - NoteAuthor
//
// Discriminator stamped on every `Note` so the publish layer can honour the
// rule from `docs/spec/briefs/identity-05-synthesis.md` §5: user-authored
// notes sign with the user's Nostr identity; agent-authored notes stay
// local-only.
//
// Backward-compat: legacy persisted snapshots have no `author` field — the
// `Note` decoder defaults to `.user` so existing local data keeps working.

enum NoteAuthor: String, Codable, Sendable, Hashable {
    case user
    case agent
}
