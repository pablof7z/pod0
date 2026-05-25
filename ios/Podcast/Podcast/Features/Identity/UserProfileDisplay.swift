import Foundation

// MARK: - UserProfileDisplay
//
// Represents the user's kind-0 profile for display purposes.
// `from(identity:)` prefers stored kind-0 fields fetched from relays and
// falls back to the deterministic generated profile when none are available.

struct UserProfileDisplay {

    let displayName: String
    let slug: String
    let about: String
    let pictureURLString: String

    /// Derive a generated profile purely from an identity seed. The seed
    /// is whatever stable bytes the caller has — historically the kind-1
    /// hex pubkey; post-NMP migration the npub itself (its first 16
    /// characters carry the same entropy for hashing purposes). The
    /// generated values are stable for a given seed.
    init(seed: String) {
        let trimmedSeed = String(seed.prefix(16))
        let index = Self.stableIndex(trimmedSeed)
        let adjectives = ["Bright", "Quiet", "Swift", "Kind", "Clear", "North"]
        let nouns = ["Signal", "Notebook", "Harbor", "Lantern", "Thread", "Field"]
        let adjective = adjectives[index % adjectives.count]
        let noun = nouns[(index / adjectives.count) % nouns.count]
        self.displayName = "\(adjective) \(noun)"
        self.slug = "\(adjective.lowercased())-\(noun.lowercased())-\(seed.prefix(4))"
        self.about = ""
        self.pictureURLString = "https://api.dicebear.com/9.x/personas/svg?seed=\(trimmedSeed)"
    }

    init(displayName: String, slug: String, about: String, pictureURLString: String) {
        self.displayName = displayName
        self.slug = slug
        self.about = about
        self.pictureURLString = pictureURLString
    }

    /// Prefers real kind-0 fields from the identity projection; falls back
    /// to the deterministic generated profile when only the npub is known.
    /// Returns `nil` when no identity is loaded yet.
    static func from(identity: IdentityViewModel) -> UserProfileDisplay? {
        guard let npub = identity.npub, !npub.isEmpty else { return nil }
        let generated = UserProfileDisplay(seed: npub)
        // Kind-0 may be partially populated — display name and picture
        // arrive together with the relay fetch. When the projection has
        // nothing yet, fall through to the generated profile so the UI
        // always shows *something* tied to the pubkey.
        guard identity.displayName != nil || identity.pictureURLString != nil
        else { return generated }
        return UserProfileDisplay(
            displayName:     identity.displayName       ?? generated.displayName,
            slug:            generated.slug,
            about:           generated.about,
            pictureURLString: identity.pictureURLString ?? generated.pictureURLString
        )
    }

    var pictureURL: URL? {
        guard let s = pictureURLString.isEmpty ? nil : pictureURLString,
              let url = URL(string: s),
              let scheme = url.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else { return nil }
        return url
    }

    // MARK: - Hash helper

    /// Stable 31-multiplicative hash over the seed's UTF-8 bytes. Used to
    /// pick the adjective/noun pair so the generated profile is
    /// deterministic for a given identity (same npub always lands on the
    /// same generated display name).
    private static func stableIndex(_ seed: String) -> Int {
        seed.utf8.reduce(0) { partial, byte in
            (partial &* 31 &+ Int(byte)) & 0x7fffffff
        }
    }
}

// MARK: - DicebearStyle (curated 6, per identity-05-synthesis §4.4)

/// The six curated dicebear styles offered to the user. Order matches the
/// horizontal rail in §4.4: personas, notionists, lorelei, shapes, glass,
/// identicon. Selected style is currently held only in local UI state — Slice B
/// will persist the user's pick into the kind-0 picture URL.
enum DicebearStyle: String, CaseIterable, Identifiable {
    case personas
    case notionists
    case lorelei
    case shapes
    case glass
    case identicon

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .personas:   "Personas"
        case .notionists: "Notionist"
        case .lorelei:    "Lorelei"
        case .shapes:     "Shapes"
        case .glass:      "Glass"
        case .identicon:  "Identicon"
        }
    }

    /// Builds the canonical dicebear URL for a pubkey-derived seed.
    func url(seed: String) -> URL? {
        URL(string: "https://api.dicebear.com/9.x/\(rawValue)/svg?seed=\(seed)")
    }
}
