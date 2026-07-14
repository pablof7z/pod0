import Foundation

// MARK: - Wiki tools (query, create, list, delete)
//
// Split out of `AgentTools+Podcast.swift` to keep files under the 500-line
// hard limit. Dispatch entries in `AgentTools.dispatchPodcast` route all four
// wiki tool names here.

extension AgentTools {

    nonisolated(unsafe) private static let wikiISO8601 = ISO8601DateFormatter()

    // MARK: - query_wiki

    static func queryWikiTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let topic = (args["topic"] as? String)?.trimmed, !topic.isEmpty else {
            return toolError("Missing or empty 'topic'")
        }
        let scope = (args["scope"] as? String)?.trimmed.nilIfEmpty
        let limit = clampedLimit(args["limit"], default: podcastWikiDefaultLimit, max: 10)
        do {
            let hits = try await deps.wiki.queryWiki(topic: topic, scope: scope, limit: limit)
            let rows = hits.map { hit -> [String: Any] in
                var row: [String: Any] = [
                    "page_id": hit.pageID,
                    "title": hit.title,
                    "excerpt": hit.excerpt,
                ]
                if let s = hit.score { row["score"] = s }
                return row
            }
            return toolSuccess([
                "topic": topic,
                "total_found": rows.count,
                "results": rows,
            ])
        } catch {
            return toolError("query_wiki failed: \(error.localizedDescription)")
        }
    }

    // MARK: - create_wiki_page

    static func createWikiPageTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let title = (args["title"] as? String)?.trimmed, !title.isEmpty else {
            return toolError("Missing or empty 'title'")
        }
        let kind = (args["kind"] as? String)?.trimmed.nilIfEmpty ?? "topic"
        let scope = (args["scope"] as? String)?.trimmed.nilIfEmpty
        do {
            let result = try await deps.wiki.createWikiPage(title: title, kind: kind, scope: scope)
            return toolSuccess([
                "page_id": result.pageID,
                "slug": result.slug,
                "title": result.title,
                "kind": result.kind,
                "summary": result.summary,
                "claim_count": result.claimCount,
                "citation_count": result.citationCount,
                "confidence": result.confidence,
                "message": "Wiki page compiled and saved. The system will auto-refresh it as new episodes arrive.",
            ])
        } catch {
            return toolError("create_wiki_page failed: \(error.localizedDescription)")
        }
    }

    // MARK: - list_wiki_pages

    static func listWikiPagesTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        let scope = (args["scope"] as? String)?.trimmed.nilIfEmpty
        let limit = clampedLimit(args["limit"], default: 25, max: 100)
        do {
            let pages = try await deps.wiki.listWikiPages(scope: scope, limit: limit)
            let rows: [[String: Any]] = pages.map { page in
                [
                    "slug": page.slug,
                    "title": page.title,
                    "kind": page.kind,
                    "summary": page.summary,
                    "confidence": page.confidence,
                    "generated_at": wikiISO8601.string(from: page.generatedAt),
                    "citation_count": page.citationCount,
                ] as [String: Any]
            }
            return toolSuccess([
                "total_found": rows.count,
                "results": rows,
            ])
        } catch {
            return toolError("list_wiki_pages failed: \(error.localizedDescription)")
        }
    }

    // MARK: - delete_wiki_page

    static func deleteWikiPageTool(args: [String: Any], deps: PodcastAgentToolDeps) async -> String {
        guard let slug = (args["slug"] as? String)?.trimmed, !slug.isEmpty else {
            return toolError("Missing or empty 'slug'")
        }
        let scope = (args["scope"] as? String)?.trimmed.nilIfEmpty
        do {
            try await deps.wiki.deleteWikiPage(slug: slug, scope: scope)
            return toolSuccess([
                "slug": slug,
                "deleted": true,
            ])
        } catch {
            return toolError("delete_wiki_page failed: \(error.localizedDescription)")
        }
    }
}
