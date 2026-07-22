import Foundation

struct LegacyStagedDownloadOutput: Sendable {
    let inputVersion: String
    let contentHash: String
    let byteCount: Int64
    let fileURL: URL
    let createdAt: Date
}

/// Quarantined, read-mostly adapter used only by the one-shot Rust cutover.
/// It cannot schedule, select, delete, or publish a download.
struct LegacyDownloadSourceStore: Sendable {
    static let shared = LegacyDownloadSourceStore()

    let rootURL: URL

    init(rootURL: URL? = nil) {
        if let rootURL {
            self.rootURL = rootURL
            return
        }
        let support = (try? FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )) ?? FileManager.default.temporaryDirectory
        self.rootURL = support
            .appendingPathComponent("podcastr", isDirectory: true)
            .appendingPathComponent("downloads", isDirectory: true)
    }

    func writeResumeData(_ data: Data, episodeID: UUID) throws {
        try FileManager.default.createDirectory(at: rootURL, withIntermediateDirectories: true)
        try data.write(to: resumeURL(episodeID), options: .atomic)
    }

    func loadResumeData(episodeID: UUID) -> Data? {
        try? Data(contentsOf: resumeURL(episodeID), options: .mappedIfSafe)
    }

    func recoverableStagedOutput(
        episodeID: UUID,
        inputVersion: String
    ) -> LegacyStagedDownloadOutput? {
        let directory = rootURL.appendingPathComponent("attempts", isDirectory: true)
            .appendingPathComponent(episodeID.uuidString, isDirectory: true)
        let manifests = (try? FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        )) ?? []
        return manifests.compactMap { url -> LegacyStagedDownloadOutput? in
            guard url.pathExtension == "json",
                  let data = try? Data(contentsOf: url),
                  let manifest = try? Self.decoder.decode(Manifest.self, from: data),
                  manifest.episodeID == episodeID,
                  manifest.inputVersion == inputVersion else { return nil }
            let file = directory.appendingPathComponent(manifest.fileName)
            guard let bytes = try? Data(contentsOf: file, options: .mappedIfSafe),
                  Int64(bytes.count) == manifest.byteCount,
                  ArtifactRepository.hash(bytes) == manifest.contentHash
            else { return nil }
            return LegacyStagedDownloadOutput(
                inputVersion: manifest.inputVersion,
                contentHash: manifest.contentHash,
                byteCount: manifest.byteCount,
                fileURL: file,
                createdAt: manifest.createdAt
            )
        }.sorted { $0.createdAt > $1.createdAt }.first
    }

    private func resumeURL(_ episodeID: UUID) -> URL {
        rootURL.appendingPathComponent("\(episodeID.uuidString).resume")
    }

    private struct Manifest: Codable {
        let episodeID: UUID
        let inputVersion: String
        let contentHash: String
        let byteCount: Int64
        let fileName: String
        let createdAt: Date
    }

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
