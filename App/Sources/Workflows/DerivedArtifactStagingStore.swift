import Foundation

struct ChapterCompilationOutput: Codable, Sendable, Equatable {
    enum ChapterOrigin: String, Codable, Sendable {
        case generated
        case publisher
        case publisherEnriched
    }

    let chapters: [Episode.Chapter]
    let ads: [Episode.AdSegment]
    let chapterOrigin: ChapterOrigin
}

struct VerifiedChapterArtifacts: Sendable {
    let output: ChapterCompilationOutput
    let chaptersData: Data
    let adsData: Data
    let chaptersHash: String
    let adsHash: String
}

/// Attempt-scoped staging for derived filesystem outputs. Stale attempts may
/// leave an unreferenced file, but cannot overwrite an immutable selected one.
struct DerivedArtifactStagingStore: Sendable {
    static let shared = DerivedArtifactStagingStore()

    private struct ChapterManifest: Codable {
        let episodeID: UUID
        let inputVersion: String
        let leaseToken: UUID
        let output: ChapterCompilationOutput
    }

    let rootURL: URL

    init(rootURL: URL? = nil) {
        if let rootURL {
            self.rootURL = rootURL
        } else {
            let support = (try? FileManager.default.url(
                for: .applicationSupportDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            )) ?? FileManager.default.temporaryDirectory
            self.rootURL = support
                .appendingPathComponent("podcastr", isDirectory: true)
                .appendingPathComponent("workflow-artifacts", isDirectory: true)
        }
    }

    @discardableResult
    func stageChapters(
        _ output: ChapterCompilationOutput,
        episodeID: UUID,
        inputVersion: String,
        leaseToken: UUID
    ) throws -> String {
        let manifest = ChapterManifest(
            episodeID: episodeID,
            inputVersion: inputVersion,
            leaseToken: leaseToken,
            output: output
        )
        let data = try Self.encoder.encode(manifest)
        let url = chapterAttemptURL(episodeID: episodeID, leaseToken: leaseToken)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        try data.write(to: url, options: .atomic)
        return ArtifactRepository.hash(data)
    }

    func verifiedChapters(
        episodeID: UUID,
        inputVersion: String,
        leaseToken: UUID,
        manifestHash: String
    ) -> VerifiedChapterArtifacts? {
        let url = chapterAttemptURL(episodeID: episodeID, leaseToken: leaseToken)
        guard let data = try? Data(contentsOf: url),
              ArtifactRepository.hash(data) == manifestHash,
              let manifest = try? Self.decoder.decode(ChapterManifest.self, from: data),
              manifest.episodeID == episodeID,
              manifest.inputVersion == inputVersion,
              manifest.leaseToken == leaseToken,
              let chaptersData = try? Self.encoder.encode(manifest.output.chapters),
              let adsData = try? Self.encoder.encode(manifest.output.ads) else { return nil }
        return VerifiedChapterArtifacts(
            output: manifest.output,
            chaptersData: chaptersData,
            adsData: adsData,
            chaptersHash: ArtifactRepository.hash(chaptersData),
            adsHash: ArtifactRepository.hash(adsData)
        )
    }

    func promote(
        _ verified: VerifiedChapterArtifacts,
        episodeID: UUID
    ) throws -> (chapters: URL, ads: URL) {
        let chapters = immutableURL(
            kind: "chapters", episodeID: episodeID, hash: verified.chaptersHash
        )
        let ads = immutableURL(kind: "ads", episodeID: episodeID, hash: verified.adsHash)
        try writeOnce(verified.chaptersData, to: chapters)
        try writeOnce(verified.adsData, to: ads)
        return (chapters, ads)
    }

    func loadChapters(at url: URL) -> [Episode.Chapter]? {
        load([Episode.Chapter].self, at: url)
    }

    func loadAds(at url: URL) -> [Episode.AdSegment]? {
        load([Episode.AdSegment].self, at: url)
    }

    func adoptPublisherChapters(
        _ chapters: [Episode.Chapter],
        episodeID: UUID
    ) throws -> (url: URL, contentHash: String) {
        let data = try Self.encoder.encode(chapters)
        let hash = ArtifactRepository.hash(data)
        let url = immutableURL(kind: "chapters", episodeID: episodeID, hash: hash)
        try writeOnce(data, to: url)
        return (url, hash)
    }

    private func load<T: Decodable>(_ type: T.Type, at url: URL) -> T? {
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? Self.decoder.decode(type, from: data)
    }

    private func chapterAttemptURL(episodeID: UUID, leaseToken: UUID) -> URL {
        rootURL.appendingPathComponent("attempts/chapters", isDirectory: true)
            .appendingPathComponent(episodeID.uuidString, isDirectory: true)
            .appendingPathComponent("\(leaseToken.uuidString).json")
    }

    private func immutableURL(kind: String, episodeID: UUID, hash: String) -> URL {
        rootURL.appendingPathComponent(kind, isDirectory: true)
            .appendingPathComponent(episodeID.uuidString, isDirectory: true)
            .appendingPathComponent("\(hash).json")
    }

    private func writeOnce(_ data: Data, to url: URL) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true
        )
        guard !FileManager.default.fileExists(atPath: url.path) else { return }
        do { try data.write(to: url, options: .withoutOverwriting) }
        catch CocoaError.fileWriteFileExists { return }
    }

    private static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    private static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}
