import Foundation

// MARK: - ClipTranscriptRow

/// One row in the clip transcript reader. Either a speaker turn or a run of
/// collapsed segments rendered as the ragged silhouette of paragraphs with no
/// words — the fold carries no glyph, no count and no "show more".
enum ClipTranscriptRow: Identifiable, Hashable {
    case fold(id: String, weight: Int)
    case turn(ClipTranscriptTurn)

    var id: String {
        switch self {
        case .fold(let id, _): return id
        case .turn(let turn):  return turn.id
        }
    }
}

// MARK: - ClipTranscriptTurn

/// A contiguous run of transcript segments sharing one speaker and one
/// clip membership, presented at a single weight of presence.
struct ClipTranscriptTurn: Identifiable, Hashable {

    /// How present this turn is on screen. Legibility *is* the highlight —
    /// nothing is drawn around the text, the way a highlight in a book is not
    /// a box but simply where the eye goes.
    enum Presence: Hashable {
        /// The clip the reader opened.
        case focus
        /// Another clip the reader made in this episode. Never folded, so the
        /// page doubles as a contents page made of their own attention.
        case mark
        /// Sentences either side of a clip. Present enough to read if you
        /// lean in, quiet enough that you don't.
        case context
    }

    let id: String
    let presence: Presence
    /// Rendered only when the speaker changes — who is talking is content in a
    /// conversation, and it earns its space by appearing rarely.
    let speakerName: String?
    let text: String
    let start: TimeInterval
    let end: TimeInterval
    /// Set when this turn is part of a clip, so a tap knows what it opens.
    let clipID: UUID?
    /// Drives the marginal dot. An unwritten clip carries nothing — it is
    /// complete, not incomplete.
    let isAnnotated: Bool
}
