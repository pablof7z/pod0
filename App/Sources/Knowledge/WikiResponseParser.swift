import Foundation

// MARK: - LLM response decoding

/// Decodes the raw JSON the LLM returns into draft sections + claims.
///
/// The compile prompt locks the model to a stable schema (see
/// `WikiPrompts.system`). This parser is liberal in what it accepts —
/// missing fields fall back to defaults, snake_case and camelCase are
/// both accepted, and unknown sections are passed through as `freeform`.
/// Strict validation is the verifier's job, not the parser's.
enum WikiResponseParser {

    // MARK: - Public

    /// Parses an LLM JSON payload into a draft page (pre-verification).
    /// Throws when the payload is not valid JSON or is missing the top-
    /// level shape entirely; otherwise tolerates field-level errors.
    static func parse(
        json: String,
        slug: String,
        scope: WikiScope,
        kind: WikiPageKind,
        model: String
    ) throws -> WikiPage {
        guard
            let data = json.data(using: .utf8),
            let root = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            throw ParseError.invalidJSON
        }

        let title = (root["title"] as? String) ?? slug
        let summary = (root["summary"] as? String) ?? ""
        let confidence = (root["confidence"] as? Double) ?? 0.5

        let rawSections = (root["sections"] as? [[String: Any]]) ?? []
        let sections = rawSections.enumerated().map { idx, dict in
            parseSection(dict, ordinal: idx)
        }

        let dedupedCitations = sections
            .flatMap { $0.claims.flatMap(\.citations) }
            .uniquedByCitationLocation()

        return WikiPage(
            slug: slug,
            title: title,
            kind: kind,
            scope: scope,
            summary: summary,
            sections: sections,
            citations: dedupedCitations,
            confidence: confidence,
            generatedAt: Date(),
            model: model
        )
    }

    // MARK: - Private

    private static func parseSection(_ dict: [String: Any], ordinal: Int) -> WikiSection {
        let heading = (dict["heading"] as? String) ?? "Untitled"
        let kindString = (dict["kind"] as? String) ?? "freeform"
        let kind = WikiSectionKind(rawValue: kindString) ?? .freeform
        let rawClaims = (dict["claims"] as? [[String: Any]]) ?? []
        let claims = rawClaims.compactMap(parseClaim)
        return WikiSection(
            heading: heading,
            kind: kind,
            ordinal: ordinal,
            claims: claims
        )
    }

    private static func parseClaim(_ dict: [String: Any]) -> WikiClaim? {
        guard let text = dict["text"] as? String, !text.isEmpty else { return nil }
        let confString = (dict["confidence"] as? String) ?? "medium"
        let confidence = WikiConfidenceBand(rawValue: confString) ?? .medium
        let isGK = (dict["general_knowledge"] as? Bool)
            ?? (dict["isGeneralKnowledge"] as? Bool)
            ?? false
        let rawCites = (dict["citations"] as? [[String: Any]]) ?? []
        let citations = rawCites.compactMap(parseCitation)
        return WikiClaim(
            text: text,
            citations: citations,
            confidence: confidence,
            isGeneralKnowledge: isGK
        )
    }

    private static func parseCitation(_ dict: [String: Any]) -> WikiCitation? {
        let episodeString = (dict["episode_id"] as? String)
            ?? (dict["episodeID"] as? String)
            ?? ""
        guard let episodeID = UUID(uuidString: episodeString) else { return nil }
        let startMS = parseInt(dict["start_ms"]) ?? parseInt(dict["startMS"]) ?? 0
        let endMS = parseInt(dict["end_ms"]) ?? parseInt(dict["endMS"]) ?? startMS
        let snippet = (dict["quote_snippet"] as? String)
            ?? (dict["quoteSnippet"] as? String)
            ?? ""
        let speaker = (dict["speaker"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        return WikiCitation(
            episodeID: episodeID,
            startMS: startMS,
            endMS: endMS,
            quoteSnippet: snippet,
            speaker: speaker
        )
    }

    private static func parseInt(_ value: Any?) -> Int? {
        if let int = value as? Int { return int }
        if let double = value as? Double { return Int(double) }
        if let string = value as? String { return Int(string) }
        return nil
    }
}

// MARK: - Errors

extension WikiResponseParser {
    enum ParseError: LocalizedError {
        case invalidJSON

        var errorDescription: String? {
            switch self {
            case .invalidJSON: "LLM response was not valid JSON"
            }
        }
    }
}

// MARK: - Helpers

private struct CitationLocationKey: Hashable {
    let episodeID: UUID
    let startMS: Int
    let endMS: Int
}

private extension Array where Element == WikiCitation {
    /// Deduplicates citations by `(episodeID, startMS, endMS)` while
    /// preserving order. The LLM tends to repeat the same citation
    /// across consensus / contradictions / citations sections; the
    /// page-level list should hold each location once.
    func uniquedByCitationLocation() -> [WikiCitation] {
        var seen: Set<CitationLocationKey> = []
        var out: [WikiCitation] = []
        for citation in self {
            let key = CitationLocationKey(
                episodeID: citation.episodeID,
                startMS: citation.startMS,
                endMS: citation.endMS
            )
            if seen.insert(key).inserted {
                out.append(citation)
            }
        }
        return out
    }
}
