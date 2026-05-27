import Foundation

// MARK: - Legacy I/O capability
//
// One-shot read primitives the kernel uses on first launch to recover data
// the legacy Swift Podcastr app wrote into its App Group.
//
// Doctrine (docs/product-spec/overview-and-dx.md §1.5):
//   D6 — failures cross the boundary as `result_json` payloads, never as
//        thrown Swift errors. A missing file, a missing App Group container,
//        or a UserDefaults read miss is all data.
//   D7 — this capability READS and REPORTS only. It NEVER decides whether
//        the legacy data is "good enough" to migrate from, which fields to
//        merge or drop, or when to set the `pcst.migration.v1.done`
//        sentinel. Those decisions live in `podcast-core::migration`. The
//        capability hands back raw bytes (and the sentinel flag) and lets
//        Rust drive.
//
// iOS-only. On Android/web this capability returns `not_found` — both
// platforms are greenfield installs in M2; there is no legacy Swift data
// to migrate.
//
// Wire types (`LegacyIORequest`, `LegacyIOResult`) live in `LegacyIOTypes.swift`.

/// iOS-only one-shot legacy-data reader. The capability has no decisions of
/// its own (D7) — every byte it returns is fed to `podcast-core::migration`
/// which owns the merge/skip/abort policy.
final class LegacyIOCapability {
    static let namespace = "pcst.legacy_io.capability"

    /// App Group suite name. Reads from the app's `Info.plist`
    /// `AppGroupIdentifier` key when present; otherwise falls back to the
    /// legacy literal so a stripped-down developer build still works.
    private let appGroupIdentifier: String
    /// `UserDefaults` key the legacy `Persistence.swift` migration looks at.
    /// Kept here verbatim — see `Persistence.swift:289`.
    private let legacyStateUserDefaultsKey = "podcastr.state.v1"
    /// Sentinel key (UserDefaults — NOT keychain; it's not a secret).
    private let migrationDoneKey = "pcst.migration.v1.done"
    /// File name written by `Persistence.swift:273` inside the App Group
    /// container's `Library/Application Support` directory.
    private let stateFileName = "podcastr-state.v1.json"
    /// Sidecar file name. Derived from the state file's base name in
    /// `Persistence.episodeStoreURL(for:)`.
    private let episodeDbFileName = "podcastr-state.v1.episodes.sqlite"

    private var started = false

    init(appGroupIdentifier: String? = nil) {
        if let appGroupIdentifier {
            self.appGroupIdentifier = appGroupIdentifier
        } else {
            let bundled = Bundle.main.object(forInfoDictionaryKey: "AppGroupIdentifier") as? String
            self.appGroupIdentifier = bundled ?? "group.com.podcastr.app"
        }
    }

    // MARK: Lifecycle (idempotent)

    func start() { started = true }
    func stop() { started = false }
    var isStarted: Bool { started }

    // MARK: Envelope handling (never throws — D6)

    /// Decode → execute → encode. Any failure is returned inside the envelope's
    /// `result_json`, never raised.
    func handle(_ request: CapabilityRequest) -> CapabilityEnvelope {
        let result = process(request)
        let resultJSON = Self.encode(result)
            ?? "{\"status\":\"error\",\"message\":\"encode-failed\"}"
        return CapabilityEnvelope(
            namespace: Self.namespace,
            correlationID: request.correlationID,
            resultJSON: resultJSON)
    }

    /// Raw-JSON entry point for the FFI bridge.
    func handleJSON(_ requestJSON: String) -> String {
        guard
            let data = requestJSON.data(using: .utf8),
            let request = try? JSONDecoder().decode(CapabilityRequest.self, from: data)
        else {
            let env = CapabilityEnvelope(
                namespace: Self.namespace,
                correlationID: "",
                resultJSON: Self.encode(LegacyIOResult.error("malformed-request"))
                    ?? "{\"status\":\"error\"}")
            return Self.encode(env) ?? "{}"
        }
        return Self.encode(handle(request)) ?? "{}"
    }

    // MARK: - Internals

    private func process(_ request: CapabilityRequest) -> LegacyIOResult {
        guard started else {
            return .error("capability-not-started")
        }
        guard
            let payload = request.payloadJSON.data(using: .utf8),
            let typed = try? JSONDecoder().decode(LegacyIORequest.self, from: payload)
        else {
            return .error("malformed-payload")
        }

        switch typed {
        case .readStateJson:        return readStateJson()
        case .readEpisodeDb:        return readEpisodeDb()
        case .listAuditLogs:        return listAuditLogs()
        case .readAuditLog(let id): return readAuditLog(episodeID: id)
        case .migrationDoneRead:    return migrationDoneRead()
        case .migrationDoneSet:     return migrationDoneSet()
        }
    }

    // MARK: Read primitives

    private func readStateJson() -> LegacyIOResult {
        // 1. Try the file in the App Group container — that's where the
        //    shipping Persistence backend writes.
        if let url = appGroupApplicationSupportURL()?.appendingPathComponent(stateFileName),
           FileManager.default.fileExists(atPath: url.path),
           let data = try? Data(contentsOf: url) {
            return .ok(dataBase64: data.base64EncodedString(), source: "file")
        }
        // 2. Fall back to the legacy `UserDefaults(suite:).data(forKey:)`
        //    blob the very-early build wrote before the file backend
        //    existed. `Persistence.swift:209` is the symmetric Swift path.
        if let defaults = UserDefaults(suiteName: appGroupIdentifier),
           let data = defaults.data(forKey: legacyStateUserDefaultsKey) {
            return .ok(dataBase64: data.base64EncodedString(), source: "user_defaults")
        }
        return .notFound
    }

    private func readEpisodeDb() -> LegacyIOResult {
        guard let url = appGroupApplicationSupportURL()?
                .appendingPathComponent(episodeDbFileName) else {
            return .notFound
        }
        guard FileManager.default.fileExists(atPath: url.path),
              let data = try? Data(contentsOf: url) else {
            return .notFound
        }
        return .ok(dataBase64: data.base64EncodedString(), source: "file")
    }

    private func listAuditLogs() -> LegacyIOResult {
        guard let dir = auditLogDirectoryURL() else { return .notFound }
        let manager = FileManager.default
        var isDir: ObjCBool = false
        guard manager.fileExists(atPath: dir.path, isDirectory: &isDir), isDir.boolValue else {
            return .notFound
        }
        let contents = (try? manager.contentsOfDirectory(
            at: dir,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles])) ?? []
        let ids = contents.compactMap { url -> String? in
            // File names are `<UUID>.json`. Strip the extension and validate
            // it's a UUID — drop anything else as data, never report it.
            guard url.pathExtension == "json" else { return nil }
            let stem = url.deletingPathExtension().lastPathComponent
            return UUID(uuidString: stem) != nil ? stem : nil
        }
        return .ok(episodeIDs: ids, source: "file")
    }

    private func readAuditLog(episodeID: String) -> LegacyIOResult {
        guard let dir = auditLogDirectoryURL() else { return .notFound }
        // Validate the id is a UUID — the kernel sends what we listed but a
        // capability never trusts its caller. A non-UUID id is `not_found`,
        // not an error: the kernel may have stale ids.
        guard UUID(uuidString: episodeID) != nil else { return .notFound }
        let url = dir.appendingPathComponent("\(episodeID).json")
        guard FileManager.default.fileExists(atPath: url.path),
              let data = try? Data(contentsOf: url) else {
            return .notFound
        }
        return .ok(dataBase64: data.base64EncodedString(), source: "file")
    }

    // MARK: Sentinel

    private func migrationDoneRead() -> LegacyIOResult {
        guard let defaults = UserDefaults(suiteName: appGroupIdentifier) else {
            // Without an App Group, we can't read shared state. Report
            // `not_found` so the kernel can decide whether to attempt the
            // (single-process) migration anyway against a missing source.
            return .notFound
        }
        let done = defaults.bool(forKey: migrationDoneKey)
        return .ok(done: done)
    }

    private func migrationDoneSet() -> LegacyIOResult {
        guard let defaults = UserDefaults(suiteName: appGroupIdentifier) else {
            return .error("app-group-unavailable")
        }
        defaults.set(true, forKey: migrationDoneKey)
        return .ok(done: true)
    }

    // MARK: Path resolution

    private func appGroupApplicationSupportURL() -> URL? {
        // Mirrors `Persistence.appGroupStateFileURL` exactly. Returns nil
        // when the App Group entitlement is missing — the new app's
        // entitlements may not yet declare `group.com.podcastr.app`, in
        // which case the migration source is unreachable and we report
        // `not_found` to the kernel.
        FileManager.default
            .containerURL(forSecurityApplicationGroupIdentifier: appGroupIdentifier)?
            .appendingPathComponent("Library/Application Support", isDirectory: true)
    }

    private func auditLogDirectoryURL() -> URL? {
        // Audit logs live in the *per-app* Application Support, not the
        // App Group container — the legacy app didn't share them with
        // extensions. See `EpisodeAuditLogStore.swift:58`.
        guard let support = try? FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: false
        ) else { return nil }
        return support
            .appendingPathComponent("podcastr", isDirectory: true)
            .appendingPathComponent("audit", isDirectory: true)
    }

    private static func encode<T: Encodable>(_ value: T) -> String? {
        guard let data = try? JSONEncoder().encode(value) else { return nil }
        return String(data: data, encoding: .utf8)
    }
}
