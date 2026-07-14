import Foundation

/// System + user prompts handed to the LLM for one categorization run.
///
/// The model is asked to echo every subscription UUID exactly as supplied,
/// inside a single ```json``` fence; the parser tolerates a fenceless
/// response too. See `PodcastCategorizationParser` for the wire shape.
enum PodcastCategorizationPrompt {

    /// Hard cap on the description excerpt sent per show. Long RSS
    /// descriptions can run 2-3 KB each; trimming keeps the prompt under
    /// realistic context limits when the user follows hundreds of feeds.
    static let descriptionCharacterLimit = 600

    static func systemPrompt() -> String {
        """
        You are a podcast librarian. Given a list of podcasts the user follows, group them into 6-12 coherent categories that span the entire library. Return only JSON.

        Rules:
        - Every podcast must be assigned to exactly one category.
        - Use the exact subscription IDs supplied — do not invent new IDs.
        - Slug must be lowercase, hyphenated, ASCII (e.g. "tech-deep-dives").
        - Description is one short sentence describing what kind of show fits the category.
        - colorHex is optional; when given, use a #RRGGBB tint friendly to a dark, glassy UI.
        - Wrap the entire response in a single ```json``` code fence and do not include any prose outside the fence.
        """
    }

    static func userPrompt(podcasts: [Podcast]) -> String {
        var lines: [String] = []
        lines.reserveCapacity(podcasts.count * 4)
        lines.append("Subscriptions:")
        for podcast in podcasts {
            lines.append("- id: \(podcast.id.uuidString)")
            lines.append("  title: \(sanitize(podcast.title))")
            if !podcast.author.isEmpty {
                lines.append("  author: \(sanitize(podcast.author))")
            }
            let trimmedDescription = trimDescription(podcast.description)
            if !trimmedDescription.isEmpty {
                lines.append("  description: \(sanitize(trimmedDescription))")
            }
            if !podcast.categories.isEmpty {
                lines.append("  itunes_categories: \(podcast.categories.joined(separator: ", "))")
            }
        }
        lines.append("")
        lines.append("Return JSON in this exact shape:")
        lines.append(
            """
            ```json
            {
              "categories": [
                {
                  "name": "Display name",
                  "slug": "display-name",
                  "description": "One sentence about what fits here.",
                  "colorHex": "#5B8DEF",
                  "subscriptionIDs": ["<uuid>", "<uuid>"]
                }
              ]
            }
            ```
            """
        )
        return lines.joined(separator: "\n")
    }

    // MARK: - Helpers

    /// Strips characters that would confuse the YAML-ish bullet format above
    /// (newlines collapsed to spaces, control chars dropped). Quoting isn't
    /// needed because nothing here is parsed mechanically — the model reads
    /// the text directly.
    private static func sanitize(_ value: String) -> String {
        value
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: "\n", with: " ")
            .components(separatedBy: .controlCharacters)
            .joined()
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func trimDescription(_ value: String) -> String {
        let collapsed = sanitize(value)
        guard collapsed.count > descriptionCharacterLimit else { return collapsed }
        let endIndex = collapsed.index(collapsed.startIndex, offsetBy: descriptionCharacterLimit)
        return String(collapsed[..<endIndex]) + "…"
    }
}
