import Foundation

// MARK: - Wiki page

/// The synthesized, citation-grounded article that is the heart of the
/// LLM Wiki surface.
///
/// Pages are *compiled* from raw transcript spans by `WikiGenerator`,
/// persisted by `WikiStorage`, and rendered by `WikiPageView`. They live in
/// one of two scopes: the cross-podcast **global** library, or **per
/// podcast** when the synthesis is scoped to a single feed.
///
/// A page is *immutable on disk* in the sense that the user never edits
/// the JSON directly — the agent regenerates pages by writing a new
/// version atomically (see `WikiStorage.write`). The user *contests*
/// claims; regeneration honours those contests.
struct WikiPage: Codable, Hashable, Identifiable, Sendable {

    /// Current on-disk schema version. Bump when adding required fields or
    /// changing the meaning of an existing one — `WikiStorage.read` will
    /// refuse to load pages whose `schemaVersion` is *higher* than this
    /// constant (i.e. a downgrade), and `init(from:)` upgrades older
    /// versions in-place by defaulting missing fields.
    static let currentSchemaVersion: Int = 1

    var id: UUID
    var slug: String
    var title: String
    var kind: WikiPageKind
    var scope: WikiScope
    var summary: String
    var sections: [WikiSection]
    var citations: [WikiCitation]
    var confidence: Double
    var generatedAt: Date
    var model: String
    var compileRevision: Int
    /// Schema version this page was written under. Stored on disk so a
    /// future field addition can default cleanly on old pages and a
    /// downgrade can be detected and skipped rather than silently parsed
    /// against an incompatible shape.
    var schemaVersion: Int
    /// Whether the user has pinned this page. Defaults to `false` so
    /// existing on-disk pages decode without a migration (synthesized
    /// Codable supplies the default when the key is absent, and the custom
    /// `init(from:)` below uses `decodeIfPresent`).
    var isPinned: Bool

    init(
        id: UUID = UUID(),
        slug: String,
        title: String,
        kind: WikiPageKind,
        scope: WikiScope,
        summary: String,
        sections: [WikiSection] = [],
        citations: [WikiCitation] = [],
        confidence: Double = 0.5,
        generatedAt: Date = Date(),
        model: String = "openai/gpt-4o",
        compileRevision: Int = 1,
        schemaVersion: Int = WikiPage.currentSchemaVersion,
        isPinned: Bool = false
    ) {
        self.id = id
        self.slug = WikiPage.normalize(slug: slug)
        self.title = title
        self.kind = kind
        self.scope = scope
        self.summary = summary
        self.sections = sections
        self.citations = citations
        self.confidence = max(0, min(1, confidence))
        self.generatedAt = generatedAt
        self.model = model
        self.compileRevision = compileRevision
        self.schemaVersion = schemaVersion
        self.isPinned = isPinned
    }

    // MARK: - Codable (back-compat)

    private enum CodingKeys: String, CodingKey {
        case id, slug, title, kind, scope, summary
        case sections, citations, confidence
        case generatedAt, model, compileRevision, schemaVersion
        case isPinned
    }

    /// Custom decode using `decodeIfPresent` on every field so older
    /// on-disk pages — written before `schemaVersion`, before a new
    /// optional field, etc. — round-trip cleanly instead of throwing on
    /// the first missing key. Anything truly required (id, slug, title,
    /// kind, scope) still throws if absent.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let id = try c.decode(UUID.self, forKey: .id)
        let slug = try c.decode(String.self, forKey: .slug)
        let title = try c.decode(String.self, forKey: .title)
        let kind = try c.decode(WikiPageKind.self, forKey: .kind)
        let scope = try c.decode(WikiScope.self, forKey: .scope)
        let summary = try c.decodeIfPresent(String.self, forKey: .summary) ?? ""
        let sections = try c.decodeIfPresent([WikiSection].self, forKey: .sections) ?? []
        let citations = try c.decodeIfPresent([WikiCitation].self, forKey: .citations) ?? []
        let confidence = try c.decodeIfPresent(Double.self, forKey: .confidence) ?? 0.5
        let generatedAt = try c.decodeIfPresent(Date.self, forKey: .generatedAt) ?? Date()
        let model = try c.decodeIfPresent(String.self, forKey: .model) ?? "openai/gpt-4o"
        let compileRevision = try c.decodeIfPresent(Int.self, forKey: .compileRevision) ?? 1
        let schemaVersion = try c.decodeIfPresent(Int.self, forKey: .schemaVersion) ?? 1
        let isPinned = try c.decodeIfPresent(Bool.self, forKey: .isPinned) ?? false
        self.init(
            id: id,
            slug: slug,
            title: title,
            kind: kind,
            scope: scope,
            summary: summary,
            sections: sections,
            citations: citations,
            confidence: confidence,
            generatedAt: generatedAt,
            model: model,
            compileRevision: compileRevision,
            schemaVersion: schemaVersion,
            isPinned: isPinned
        )
    }

    /// The flattened list of claims across all sections, ordered by
    /// section ordinal then in-section position. Useful for the verifier
    /// and for accessibility's "Claims" rotor.
    var allClaims: [WikiClaim] {
        sections
            .sorted { $0.ordinal < $1.ordinal }
            .flatMap(\.claims)
    }

    /// `true` when at least one claim references the supplied episode.
    func cites(episodeID: UUID) -> Bool {
        citations.contains { $0.episodeID == episodeID }
            || allClaims.contains { claim in
                claim.citations.contains { $0.episodeID == episodeID }
            }
    }

    // MARK: - Slug normalization

    /// Canonicalises the supplied string into a URL-safe slug. Lowercase,
    /// dash-separated, no diacritics, only `[a-z0-9-]` retained. Used
    /// both as the on-disk filename and as the dual-link target.
    static func normalize(slug: String) -> String {
        let folded = slug
            .folding(options: .diacriticInsensitive, locale: .current)
            .lowercased()
        let allowed = Set("abcdefghijklmnopqrstuvwxyz0123456789-")
        var out = ""
        var lastWasDash = false
        for char in folded {
            if allowed.contains(char) {
                out.append(char)
                lastWasDash = char == "-"
            } else if char.isWhitespace || char == "_" {
                if !lastWasDash {
                    out.append("-")
                    lastWasDash = true
                }
            }
        }
        let trimmed = out.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return trimmed.isEmpty ? "untitled" : trimmed
    }
}

// MARK: - Page kind

/// The taxonomy of wiki pages. Every page is exactly one kind; the kind
/// drives the section layout chosen by `WikiGenerator` and the rendering
/// path picked by `WikiPageView`.
enum WikiPageKind: String, Codable, CaseIterable, Sendable {

    /// A topic page (e.g. "Ozempic"). The default and most common kind.
    case topic

    /// A person page — thin variant referencing UX-13 speaker profiles.
    case person

    /// A show-level summary page covering one podcast end-to-end.
    case show

    /// An index page (e.g. "All topics", "Recent edits") that aggregates
    /// references to other pages without owning unique content.
    case index

    /// Display-friendly short label.
    var displayName: String {
        switch self {
        case .topic: "Topic"
        case .person: "Person"
        case .show: "Show"
        case .index: "Index"
        }
    }
}

// MARK: - Wiki scope

/// Controls whether a page lives in the cross-podcast library or is
/// scoped to a single feed.
///
/// On disk, scope determines the parent directory under
/// `Application Support/podcastr/wiki/`.
enum WikiScope: Codable, Hashable, Sendable {

    case global
    case podcast(UUID)

    /// On-disk directory segment. Stable across renames so that a
    /// regenerated page lands beside its prior version.
    var pathComponent: String {
        switch self {
        case .global: "global"
        case .podcast(let id): "podcast/\(id.uuidString)"
        }
    }

    /// The podcast this scope is bound to, if any.
    var podcastID: UUID? {
        if case .podcast(let id) = self { return id }
        return nil
    }
}
