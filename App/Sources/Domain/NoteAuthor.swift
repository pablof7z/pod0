import Foundation

// MARK: - NoteAuthor
//
// Discriminator stamped on every `Note` distinguishing user-authored notes
// from agent-authored ones. All notes are local-only.
//
// Backward-compat: legacy persisted snapshots have no `author` field — the
// `Note` decoder defaults to `.user` so existing local data keeps working.

enum NoteAuthor: String, Codable, Sendable, Hashable {
    case user
    case agent
}
