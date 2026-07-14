import Foundation

// MARK: - BriefingStorage

/// File-system persistence for composed briefings.
///
/// Layout (under `Application Support/podcastr/briefings/`):
///   `<id>.json`   — `BriefingScript` data
///   `<id>.m4a`    — stitched audio (composer writes; player streams from)
///   `<id>/`       — per-segment artifacts (TTS m4a, downloaded quotes)
///
/// Briefings beyond a 30-day rolling window are pruned by the caller (UX-08 §9
/// — *Library noise*); this type only owns CRUD.
struct BriefingStorage: Sendable {

    // MARK: Init

    /// `rootDirectory` defaults to `Application Support/podcastr/briefings`.
    /// Tests inject a temporary directory to keep filesystem fixtures isolated.
    init(rootDirectory: URL? = nil) throws {
        let fm = FileManager.default
        if let root = rootDirectory {
            self.rootURL = root
        } else {
            let support = try fm.url(
                for: .applicationSupportDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            )
            self.rootURL = support
                .appendingPathComponent("podcastr", isDirectory: true)
                .appendingPathComponent("briefings", isDirectory: true)
        }
        try fm.createDirectory(at: rootURL, withIntermediateDirectories: true)
    }

    let rootURL: URL

    // MARK: Paths

    func scriptURL(id: UUID) -> URL {
        rootURL.appendingPathComponent("\(id.uuidString).json")
    }

    func audioURL(id: UUID) -> URL {
        rootURL.appendingPathComponent("\(id.uuidString).m4a")
    }

    /// Per-briefing scratch directory. The composer drops segment-level TTS
    /// renders and any cached quote audio under here so re-stitching after a
    /// "make it shorter" pinch (UX-08 §5) does not re-synthesise.
    func segmentsDirectory(id: UUID) throws -> URL {
        let url = rootURL.appendingPathComponent(id.uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    // MARK: Read

    /// Loads every script in the root directory, sorted by `generatedAt` desc.
    /// Malformed files are silently skipped — the user shouldn't lose access to
    /// their library because one rogue artifact was hand-edited.
    func listScripts() throws -> [BriefingScript] {
        let contents = try FileManager.default.contentsOfDirectory(
            at: rootURL,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        )
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        var scripts: [BriefingScript] = []
        for url in contents where url.pathExtension == "json" {
            guard let data = try? Data(contentsOf: url),
                  let script = try? decoder.decode(BriefingScript.self, from: data)
            else { continue }
            scripts.append(script)
        }
        return scripts.sorted { $0.generatedAt > $1.generatedAt }
    }

    /// Loads one script by id, or `nil` if missing.
    func loadScript(id: UUID) throws -> BriefingScript? {
        let url = scriptURL(id: id)
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        let data = try Data(contentsOf: url)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return try decoder.decode(BriefingScript.self, from: data)
    }

    // MARK: Write

    /// Atomically persists `script`. Overwrites any prior version with the
    /// same id (re-narrate / branch-record both update in place).
    func save(_ script: BriefingScript) throws {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(script)
        try data.write(to: scriptURL(id: script.id), options: [.atomic])
    }

    // MARK: Delete

    /// Removes the script JSON, the stitched m4a, and the per-briefing scratch
    /// directory. Idempotent — missing files are not an error.
    func delete(id: UUID) throws {
        let fm = FileManager.default
        for url in [scriptURL(id: id), audioURL(id: id)] {
            if fm.fileExists(atPath: url.path) {
                try fm.removeItem(at: url)
            }
        }
        let segmentsURL = rootURL.appendingPathComponent(id.uuidString, isDirectory: true)
        if fm.fileExists(atPath: segmentsURL.path) {
            try fm.removeItem(at: segmentsURL)
        }
    }

    // MARK: Maintenance

    /// Returns the ids of briefings older than `days` days. Caller decides
    /// whether to prune (UX-08 §9 — 30-day auto-archive unless saved).
    func staleScriptIDs(olderThanDays days: Int, now: Date = Date()) throws -> [UUID] {
        let scripts = try listScripts()
        let cutoff = now.addingTimeInterval(-Double(days) * 86_400)
        return scripts.filter { $0.generatedAt < cutoff }.map(\.id)
    }
}
