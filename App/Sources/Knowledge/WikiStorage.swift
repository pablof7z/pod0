import Foundation

// MARK: - Wiki storage

/// Atomic on-disk persistence for `WikiPage` objects.
///
/// Layout (mirrors the llm-wiki ethos in `docs/spec/research/llm-wiki-deep-dive.md`):
///
/// ```
/// Application Support/podcastr/wiki/
///   _inventory.json                 → registry of every page
///   global/
///     <slug>.json
///   podcast/
///     <podcast-id>/
///       <slug>.json
/// ```
///
/// Writes are atomic via `Data.write(to:options:.atomic)` — readers never
/// observe a partial page. The inventory file is updated *after* the page
/// is written, then re-written atomically itself; if the inventory write
/// fails the page on disk is still self-describing.
///
/// `WikiStorage` is `Sendable` and safe to call from any actor.
struct WikiStorage: Sendable {

    // MARK: Configuration

    /// Root directory. Defaults to `Application Support/podcastr/wiki/`.
    let root: URL

    /// Filename of the inventory registry inside `root`.
    static let inventoryFilename = "_inventory.json"

    init(root: URL? = nil) {
        if let root {
            self.root = root
        } else {
            self.root = WikiStorage.defaultRoot()
        }
    }

    // MARK: - Shared instance

    /// Process-wide shared storage rooted at the default Application Support
    /// location. UI code (the wiki tab) should read through this; tests keep
    /// constructing their own `WikiStorage(root:)` against a temp directory.
    static let shared = WikiStorage()

    // MARK: - Public API

    /// Persists the supplied page atomically and updates the inventory.
    /// Replaces any existing page at the same `(scope, slug)` location.
    func write(_ page: WikiPage) throws {
        try ensureDirectory(for: page.scope)
        let url = pageURL(for: page)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(page)
        try data.write(to: url, options: .atomic)
        try updateInventory(by: { inventory in
            inventory.upsert(.init(from: page, fileURL: url))
        })
    }

    /// Reads the page at `(scope, slug)`. Returns `nil` if the file does
    /// not exist; throws on I/O or decode errors.
    ///
    /// Pages whose `schemaVersion` is *greater* than
    /// `WikiPage.currentSchemaVersion` (i.e. a build from a newer release
    /// wrote them) return `nil` rather than risk decoding against the
    /// wrong shape. Older versions decode in place via the page's
    /// back-compat `init(from:)`.
    func read(slug: String, scope: WikiScope) throws -> WikiPage? {
        let url = pageURL(slug: slug, scope: scope)
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        let data = try Data(contentsOf: url)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let page = try decoder.decode(WikiPage.self, from: data)
        guard page.schemaVersion <= WikiPage.currentSchemaVersion else {
            return nil
        }
        return page
    }

    /// Removes the page at `(scope, slug)` and updates the inventory.
    /// No-ops when the file does not exist.
    func delete(slug: String, scope: WikiScope) throws {
        let url = pageURL(slug: slug, scope: scope)
        if FileManager.default.fileExists(atPath: url.path) {
            try FileManager.default.removeItem(at: url)
        }
        try updateInventory(by: { inventory in
            inventory.remove(slug: WikiPage.normalize(slug: slug), scope: scope)
        })
    }

    /// Loads the inventory registry. Returns an empty inventory if the
    /// file does not exist yet.
    func loadInventory() throws -> WikiInventory {
        let url = inventoryURL()
        guard FileManager.default.fileExists(atPath: url.path) else {
            return WikiInventory()
        }
        let data = try Data(contentsOf: url)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(WikiInventory.self, from: data)
    }

    /// Lists every page entry in scope. When `scope` is `nil` returns
    /// every entry across all scopes (used by the global wiki home).
    func list(scope: WikiScope? = nil) throws -> [WikiInventory.Entry] {
        let inventory = try loadInventory()
        guard let scope else { return inventory.entries }
        return inventory.entries.filter { $0.scope == scope }
    }

    /// Loads every persisted page across all scopes. Walks the inventory
    /// and decodes each referenced JSON file. Pages whose backing file is
    /// missing, unreadable, or written under a *newer* schema version
    /// than this build supports are skipped silently — the inventory will
    /// be reconciled on the next write. Returned in inventory order;
    /// callers sort as needed.
    func allPages() throws -> [WikiPage] {
        let inventory = try loadInventory()
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        var pages: [WikiPage] = []
        pages.reserveCapacity(inventory.entries.count)
        for entry in inventory.entries {
            let url = pageURL(slug: entry.slug, scope: entry.scope)
            guard
                FileManager.default.fileExists(atPath: url.path),
                let data = try? Data(contentsOf: url),
                let page = try? decoder.decode(WikiPage.self, from: data),
                page.schemaVersion <= WikiPage.currentSchemaVersion
            else { continue }
            pages.append(page)
        }
        return pages
    }

    /// Deletes the page with the supplied identifier. Resolves `id` to a
    /// `(scope, slug)` by consulting the inventory then delegates to
    /// `delete(slug:scope:)`. No-ops when no inventory entry matches.
    func delete(pageID: UUID) throws {
        let pages = try allPages()
        guard let target = pages.first(where: { $0.id == pageID }) else { return }
        try delete(slug: target.slug, scope: target.scope)
    }

    // MARK: - URL builders

    func pageURL(for page: WikiPage) -> URL {
        pageURL(slug: page.slug, scope: page.scope)
    }

    func pageURL(slug: String, scope: WikiScope) -> URL {
        let normalized = WikiPage.normalize(slug: slug)
        return scopeDirectory(for: scope).appendingPathComponent("\(normalized).json")
    }

    func inventoryURL() -> URL {
        root.appendingPathComponent(WikiStorage.inventoryFilename)
    }

    func scopeDirectory(for scope: WikiScope) -> URL {
        root.appendingPathComponent(scope.pathComponent, isDirectory: true)
    }

    // MARK: - Helpers

    private func ensureDirectory(for scope: WikiScope) throws {
        let dir = scopeDirectory(for: scope)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    private func updateInventory(by mutate: (inout WikiInventory) -> Void) throws {
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        var inventory = (try? loadInventory()) ?? WikiInventory()
        mutate(&inventory)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(inventory)
        try data.write(to: inventoryURL(), options: .atomic)
    }

    /// Resolves `Application Support/podcastr/wiki/`. Falls back to the
    /// caches directory if Application Support is unavailable.
    static func defaultRoot() -> URL {
        let fm = FileManager.default
        let base = (try? fm.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )) ?? fm.temporaryDirectory
        return base
            .appendingPathComponent("podcastr", isDirectory: true)
            .appendingPathComponent("wiki", isDirectory: true)
    }
}

// MARK: - Inventory

/// Lightweight registry mapping `(scope, slug)` to a small descriptor of
/// each page on disk. Kept separate from the page bodies so the wiki home
/// can list 1k pages without paying the cost of decoding every JSON.
struct WikiInventory: Codable, Sendable {

    var version: Int = 1
    var entries: [Entry] = []
    var updatedAt: Date = Date()

    /// Inserts or replaces the entry for the supplied page.
    mutating func upsert(_ entry: Entry) {
        entries.removeAll { $0.slug == entry.slug && $0.scope == entry.scope }
        entries.append(entry)
        entries.sort { lhs, rhs in
            if lhs.scope != rhs.scope {
                return lhs.scope.pathComponent < rhs.scope.pathComponent
            }
            return lhs.slug < rhs.slug
        }
        updatedAt = Date()
    }

    /// Removes the entry at `(slug, scope)` if present.
    mutating func remove(slug: String, scope: WikiScope) {
        entries.removeAll { $0.slug == slug && $0.scope == scope }
        updatedAt = Date()
    }

    /// One row in the inventory. Mirrors the headline metadata of a page
    /// without dragging the section bodies along.
    struct Entry: Codable, Hashable, Sendable {
        var slug: String
        var title: String
        var kind: WikiPageKind
        var scope: WikiScope
        var summary: String
        var confidence: Double
        var generatedAt: Date
        var model: String
        var citationCount: Int
        var fileURL: URL

        init(
            slug: String,
            title: String,
            kind: WikiPageKind,
            scope: WikiScope,
            summary: String,
            confidence: Double,
            generatedAt: Date,
            model: String,
            citationCount: Int,
            fileURL: URL
        ) {
            self.slug = slug
            self.title = title
            self.kind = kind
            self.scope = scope
            self.summary = summary
            self.confidence = confidence
            self.generatedAt = generatedAt
            self.model = model
            self.citationCount = citationCount
            self.fileURL = fileURL
        }

        init(from page: WikiPage, fileURL: URL) {
            self.init(
                slug: page.slug,
                title: page.title,
                kind: page.kind,
                scope: page.scope,
                summary: page.summary,
                confidence: page.confidence,
                generatedAt: page.generatedAt,
                model: page.model,
                citationCount: page.allClaims.reduce(0) { $0 + $1.citations.count },
                fileURL: fileURL
            )
        }

        // MARK: - Codable (back-compat)

        private enum CodingKeys: String, CodingKey {
            case slug, title, kind, scope, summary, confidence
            case generatedAt, model, citationCount, fileURL
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            self.init(
                slug: try c.decode(String.self, forKey: .slug),
                title: try c.decodeIfPresent(String.self, forKey: .title) ?? "",
                kind: try c.decodeIfPresent(WikiPageKind.self, forKey: .kind) ?? .topic,
                scope: try c.decode(WikiScope.self, forKey: .scope),
                summary: try c.decodeIfPresent(String.self, forKey: .summary) ?? "",
                confidence: try c.decodeIfPresent(Double.self, forKey: .confidence) ?? 0.5,
                generatedAt: try c.decodeIfPresent(Date.self, forKey: .generatedAt) ?? Date(),
                model: try c.decodeIfPresent(String.self, forKey: .model) ?? "openai/gpt-4o",
                citationCount: try c.decodeIfPresent(Int.self, forKey: .citationCount) ?? 0,
                fileURL: try c.decode(URL.self, forKey: .fileURL)
            )
        }
    }
}
