import Foundation

// MARK: - WikiResearchSkill
//
// Defines the `wiki_research` skill. When activated via
// `use_skill(skill_id: "wiki_research")` the agent receives the manual below
// and gains access to three wiki-management tools: `create_wiki_page`,
// `list_wiki_pages`, `delete_wiki_page`.
//
// The cheap lookup tool `query_wiki` stays always-on (every conversation
// might want to read existing pages) — only the management surface is
// gated, because creating a page kicks off a heavy RAG-compile +
// citation-verification job that the agent should not run by accident.

enum WikiResearchSkill {

    static let skill = AgentSkill(
        id: AgentSkillID.wikiResearch,
        displayName: "Wiki Research",
        summary: "Compile, list, and delete citation-grounded wiki pages from the user's podcast transcripts. Use when the user explicitly asks to research a topic, person, or show and save the result.",
        manual: manualText,
        toolNames: [
            AgentTools.PodcastNames.createWikiPage,
            AgentTools.PodcastNames.listWikiPages,
            AgentTools.PodcastNames.deleteWikiPage,
        ],
        schema: { schemaEntries }
    )

    // MARK: - Manual

    private static let manualText: String = """
    # Wiki Research Skill

    The wiki is the user's persistent, citation-grounded knowledge base
    compiled from their podcast transcripts. Each page is a structured
    article — claims linked back to the transcript excerpts they came from —
    that auto-refreshes as new episodes land. This skill exposes the
    management surface (compile / list / delete). The cheap read tool
    `query_wiki` stays available without this skill.

    ## When to use this

    - The user explicitly says "build a wiki page on X", "research X from my
      podcasts and save it", "compile what my podcasts say about Peter
      Attia", or similar.
    - The user wants to remove a page from their wiki.
    - The user wants to audit which pages already exist before
      compiling new ones.

    Do NOT call `create_wiki_page` for casual questions — that's what
    `query_wiki` (always-on) and `query_transcripts` (always-on) are for.
    Compiling a page is expensive: it runs a full RAG search, drafts an
    article with an LLM, verifies every claim against transcript evidence,
    and writes the result to disk. Reserve it for explicit user requests.

    ## Tools

    - `create_wiki_page(title, kind?, scope?)` — compiles and persists a new
      page. Requires an AI provider key (OpenRouter or compatible). Returns
      a result with `pageID`, `slug`, `claimCount`, `citationCount`, and a
      `confidence` score.
    - `list_wiki_pages(scope?, limit?)` — fast index listing. Does not
      decode page bodies. Use BEFORE `create_wiki_page` to check whether the
      page already exists.
    - `delete_wiki_page(slug, scope?)` — removes a page by slug. Always
      `list_wiki_pages` first to confirm the slug. No-op when the page
      doesn't exist.

    ## Kind

    `kind` is one of `"topic"`, `"person"`, or `"show"`. Defaults to
    `"topic"` for unrecognised values. Pick the right one — it changes how
    the LLM structures the article:

    - `topic` — concepts, methods, claims (e.g. "Zone 2 training",
      "Bitcoin halving", "metabolic flexibility").
    - `person` — named individuals (e.g. "Peter Attia", "Lyn Alden").
      Article is biographical-ish: positions, recurring claims, podcast
      appearances.
    - `show` — entire podcast (e.g. "Huberman Lab"). Article surveys the
      show's themes, recurring guests, signature segments.

    ## Scope

    `scope` is an optional podcast UUID string:

    - Omit → library-wide page (searches all transcripts).
    - Provide a podcast_id → constrains compilation AND queries to that
      show's transcripts only.

    Use `list_subscriptions` (always-on) to resolve a show's podcast_id when
    the user says "from the Tim Ferriss show".

    ## Auto-refresh

    Once a page exists, the system can auto-refresh it as new transcripts
    land — but ONLY when the user has enabled
    `Settings → Wiki → Auto-generate on transcript ingest`. If a user asks
    "why isn't my wiki page updating?", check that setting.

    ## Suggested flow

    1. If the user asks to compile a page, call `list_wiki_pages` first
       (constrained by scope if relevant) to check whether a page on this
       topic already exists.
    2. If it exists: ask whether to overwrite or just read the existing one
       (via `query_wiki`).
    3. If it doesn't: call `upgrade_thinking` (compilation is multi-step
       reasoning), then `create_wiki_page` with an appropriate `kind` and
       `scope`.
    4. After compilation succeeds, surface the `claimCount`, `confidence`,
       and a brief preview of the summary to the user.

    ## Failure modes

    - **No AI key**: `create_wiki_page` will return an error mentioning the
      missing provider key. Tell the user to add an OpenRouter (or
      compatible) key in Settings → AI.
    - **No transcript evidence**: returns an error indicating the RAG index
      had nothing on the requested topic. Tell the user the topic doesn't
      appear in their corpus and offer to run a transcript / wiki query
      instead.
    """

    // MARK: - Tool schemas

    @MainActor
    private static var schemaEntries: [[String: Any]] {
        [
            createWikiPageSchema,
            listWikiPagesSchema,
            deleteWikiPageSchema,
        ]
    }

    // MARK: - create_wiki_page (moved from AgentToolSchema+Podcast.swift)

    private static var createWikiPageSchema: [String: Any] {
        functionTool(
            name: AgentTools.PodcastNames.createWikiPage,
            description: """
            Compile and save a wiki page about a topic, person, or show. Searches the user's \
            transcripts, drafts a citation-grounded article, verifies every claim, and persists it \
            so the system auto-refreshes it as new episodes land. \
            Use when the user says 'build a wiki page on X', 'research X from my podcasts', \
            or 'what do my podcasts say about X — save it'. \
            Requires an AI provider key (OpenRouter or compatible). Returns the compiled page. \
            Expensive — call only on explicit user request. Always check `list_wiki_pages` first.
            """,
            properties: [
                "title": ["type": "string", "description": "Topic, person name, or show name to compile a page about."],
                "kind": ["type": "string", "enum": ["topic", "person", "show"], "description": "Page type. Defaults to 'topic'."],
                "scope": ["type": "string", "description": "Optional podcast ID to constrain compilation to one show's transcripts. Omit for a library-wide page."],
            ],
            required: ["title"]
        )
    }

    // MARK: - list_wiki_pages (moved from AgentToolSchema+Podcast.swift)

    private static var listWikiPagesSchema: [String: Any] {
        functionTool(
            name: AgentTools.PodcastNames.listWikiPages,
            description: "List existing wiki pages in the user's library. Use before creating a page (to check if it already exists) or before deleting one. Fast — does not decode page bodies.",
            properties: [
                "scope": ["type": "string", "description": "Optional podcast ID to list only pages for one show. Omit for all pages."],
                "limit": ["type": "integer", "description": "Maximum pages to return (1–100). Defaults to 25."],
            ],
            required: []
        )
    }

    // MARK: - delete_wiki_page (moved from AgentToolSchema+Podcast.swift)

    private static var deleteWikiPageSchema: [String: Any] {
        functionTool(
            name: AgentTools.PodcastNames.deleteWikiPage,
            description: "Delete a wiki page by slug. Use only when the user explicitly asks to remove a page. Always call `list_wiki_pages` first to confirm the slug.",
            properties: [
                "slug": ["type": "string", "description": "URL slug of the page to delete (e.g. 'zone-2-training')."],
                "scope": ["type": "string", "description": "Optional podcast ID for pages scoped to one show. Omit for global pages."],
            ],
            required: ["slug"]
        )
    }

    // MARK: - Helper

    private static func functionTool(
        name: String,
        description: String,
        properties: [String: Any],
        required: [String]
    ) -> [String: Any] {
        [
            "type": "function",
            "function": [
                "name": name,
                "description": description,
                "parameters": [
                    "type": "object",
                    "properties": properties,
                    "required": required,
                ] as [String: Any],
            ] as [String: Any],
        ]
    }
}
