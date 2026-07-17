import Foundation

// MARK: - Slug normalization

/// Canonicalises free-form strings into URL-safe slugs. Lowercase,
/// dash-separated, no diacritics, only `[a-z0-9-]` retained.
///
/// Shared by the threading layer (topic dual-link keys) and anything else
/// that needs a stable, filesystem/URL-safe identifier derived from a
/// display string.
enum SlugNormalizer {

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
