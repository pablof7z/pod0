import Foundation

struct SharedEpisodeImportRequest: Codable, Equatable, Identifiable, Sendable {
    let id: UUID
    let sourceURL: URL
    let createdAt: Date
    /// Number of import attempts that have already failed for this request.
    /// Missing on disk (pre-existing queued shares) decodes to 0.
    var attemptCount: Int

    init(id: UUID = UUID(), sourceURL: URL, createdAt: Date = Date(), attemptCount: Int = 0) {
        self.id = id
        self.sourceURL = sourceURL
        self.createdAt = createdAt
        self.attemptCount = attemptCount
    }

    private enum CodingKeys: String, CodingKey {
        case id, sourceURL, createdAt, attemptCount
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(UUID.self, forKey: .id)
        sourceURL = try container.decode(URL.self, forKey: .sourceURL)
        createdAt = try container.decode(Date.self, forKey: .createdAt)
        attemptCount = try container.decodeIfPresent(Int.self, forKey: .attemptCount) ?? 0
    }
}

struct SharedEpisodeImportRequestStore: Sendable {
    static let appGroupIdentifier = "group.com.podcastr.app"

    enum StoreError: Error, LocalizedError {
        case appGroupUnavailable

        var errorDescription: String? {
            switch self {
            case .appGroupUnavailable:
                "Pod0 could not access its shared import inbox."
            }
        }
    }

    private let directoryURL: URL

    init(directoryURL: URL) {
        self.directoryURL = directoryURL
    }

    static func appGroup(
        fileManager: FileManager = .default
    ) throws -> SharedEpisodeImportRequestStore {
        guard let container = fileManager.containerURL(
            forSecurityApplicationGroupIdentifier: appGroupIdentifier
        ) else {
            throw StoreError.appGroupUnavailable
        }
        return SharedEpisodeImportRequestStore(
            directoryURL: container.appending(
                path: "SharedEpisodeImports",
                directoryHint: .isDirectory
            )
        )
    }

    @discardableResult
    func enqueue(
        sourceURL: URL,
        now: Date = Date(),
        fileManager: FileManager = .default
    ) throws -> SharedEpisodeImportRequest {
        let request = SharedEpisodeImportRequest(sourceURL: sourceURL, createdAt: now)
        try ensureDirectory(fileManager: fileManager)
        let data = try JSONEncoder().encode(request)
        try data.write(to: fileURL(for: request.id), options: .atomic)
        return request
    }

    func pendingRequests(
        fileManager: FileManager = .default
    ) throws -> [SharedEpisodeImportRequest] {
        try ensureDirectory(fileManager: fileManager)
        let files = try fileManager.contentsOfDirectory(
            at: directoryURL,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        )
        let decoder = JSONDecoder()
        return files
            .filter { $0.pathExtension == "json" }
            .compactMap { try? decoder.decode(
                SharedEpisodeImportRequest.self,
                from: Data(contentsOf: $0)
            ) }
            .sorted {
                if $0.createdAt == $1.createdAt {
                    return $0.id.uuidString < $1.id.uuidString
                }
                return $0.createdAt < $1.createdAt
            }
    }

    func remove(
        _ request: SharedEpisodeImportRequest,
        fileManager: FileManager = .default
    ) throws {
        let url = fileURL(for: request.id)
        guard fileManager.fileExists(atPath: url.path) else { return }
        try fileManager.removeItem(at: url)
    }

    /// Overwrites the persisted copy of `request` in place (same id), so a
    /// failed import can record its attempt count instead of being deleted.
    func update(
        _ request: SharedEpisodeImportRequest,
        fileManager: FileManager = .default
    ) throws {
        try ensureDirectory(fileManager: fileManager)
        let data = try JSONEncoder().encode(request)
        try data.write(to: fileURL(for: request.id), options: .atomic)
    }

    private func ensureDirectory(fileManager: FileManager) throws {
        try fileManager.createDirectory(
            at: directoryURL,
            withIntermediateDirectories: true
        )
    }

    private func fileURL(for id: UUID) -> URL {
        directoryURL.appending(path: "\(id.uuidString).json")
    }
}
