import Foundation

// MARK: - Wire types

/// What the model is asked to return inside a single ```json``` fence.
/// Matches the field names in the prompt (`PodcastCategorizationPrompt`).
struct PodcastCategorizationResponse: Decodable, Sendable {
    let categories: [Category]

    struct Category: Decodable, Sendable {
        let name: String
        let slug: String
        let description: String
        let colorHex: String?
        let subscriptionIDs: [String]

        private enum CodingKeys: String, CodingKey {
            case name, slug, description, colorHex, subscriptionIDs
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            name = try c.decode(String.self, forKey: .name)
            slug = try c.decode(String.self, forKey: .slug)
            description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
            colorHex = try c.decodeIfPresent(String.self, forKey: .colorHex)
            subscriptionIDs = try c.decodeIfPresent([String].self, forKey: .subscriptionIDs) ?? []
        }
    }
}

// MARK: - Parser

/// Pulls JSON out of the assistant message, decodes it, and validates it
/// against the user's actual subscription set.
///
/// The prompt asks the model to wrap its answer in a ```json``` fence; the
/// extractor falls back to the whole content if no fence is found so a
/// well-behaved provider that strips markdown still works.
enum PodcastCategorizationParser {

    /// Validates and converts the wire response into domain `PodcastCategory`
    /// values.
    ///
    /// Validation rules (decided up-front; see service docs):
    ///   • Every UUID echoed by the model must resolve to a real subscription
    ///     in `validIDs` — otherwise throws `.invalidResponse`.
    ///   • Every subscription must end up in some category — otherwise
    ///     throws `.invalidResponse`.
    ///   • If the model places the same subscription in two categories, the
    ///     last assignment wins (deduped) so the "every subscription in
    ///     exactly one category" invariant holds in storage.
    static func categories(
        from rawContent: String,
        podcasts: [Podcast],
        generatedAt: Date,
        model: String?
    ) throws -> [PodcastCategory] {
        let json = extractJSON(from: rawContent)
        guard let data = json.data(using: .utf8) else {
            throw CategorizationError.invalidResponse
        }
        let decoded: PodcastCategorizationResponse
        do {
            decoded = try JSONDecoder().decode(PodcastCategorizationResponse.self, from: data)
        } catch {
            throw CategorizationError.invalidResponse
        }
        guard !decoded.categories.isEmpty else {
            throw CategorizationError.invalidResponse
        }

        let validIDs = Set(podcasts.map(\.id))
        var seen: [UUID: Int] = [:]
        var built: [PodcastCategory] = []
        built.reserveCapacity(decoded.categories.count)

        for raw in decoded.categories {
            var assigned: [UUID] = []
            assigned.reserveCapacity(raw.subscriptionIDs.count)
            for idString in raw.subscriptionIDs {
                guard let uuid = UUID(uuidString: idString) else {
                    throw CategorizationError.invalidResponse
                }
                guard validIDs.contains(uuid) else {
                    throw CategorizationError.invalidResponse
                }
                // Last-write-wins dedupe: if the model placed this show in a
                // previous category, drop it from there before re-assigning.
                if let priorIdx = seen[uuid] {
                    built[priorIdx].subscriptionIDs.removeAll { $0 == uuid }
                }
                assigned.append(uuid)
                seen[uuid] = built.count
            }
            built.append(
                PodcastCategory(
                    id: UUID(),
                    name: raw.name.trimmingCharacters(in: .whitespacesAndNewlines),
                    slug: raw.slug.trimmingCharacters(in: .whitespacesAndNewlines),
                    description: raw.description.trimmingCharacters(in: .whitespacesAndNewlines),
                    colorHex: raw.colorHex?.trimmingCharacters(in: .whitespacesAndNewlines),
                    subscriptionIDs: assigned,
                    generatedAt: generatedAt,
                    model: model
                )
            )
        }

        // Drop any categories the dedupe left empty rather than persisting
        // ghost rows the UI would render as headerless.
        let nonEmpty = built.filter { !$0.subscriptionIDs.isEmpty }

        let assignedSet = Set(nonEmpty.flatMap(\.subscriptionIDs))
        guard assignedSet == validIDs else {
            throw CategorizationError.invalidResponse
        }

        return nonEmpty
    }

    // MARK: - JSON extraction

    /// Returns the substring inside the first ```json``` fence in `content`,
    /// or the trimmed full content if no fence is present. Trailing prose
    /// after the closing fence is discarded.
    static func extractJSON(from content: String) -> String {
        if let inside = fencedSubstring(in: content, fenceLanguage: "json") {
            return inside
        }
        if let inside = fencedSubstring(in: content, fenceLanguage: nil) {
            return inside
        }
        return content.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func fencedSubstring(in content: String, fenceLanguage: String?) -> String? {
        let openMarker = fenceLanguage.map { "```\($0)" } ?? "```"
        guard let openRange = content.range(of: openMarker) else { return nil }
        let afterOpen = content.index(openRange.upperBound, offsetBy: 0)
        // Skip the rest of the opening fence line (any trailing language tag
        // or whitespace before the newline).
        guard let newlineAfterOpen = content[afterOpen...].firstIndex(of: "\n") else {
            return nil
        }
        let bodyStart = content.index(after: newlineAfterOpen)
        guard let closeRange = content.range(of: "```", range: bodyStart..<content.endIndex) else {
            return nil
        }
        return String(content[bodyStart..<closeRange.lowerBound])
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
