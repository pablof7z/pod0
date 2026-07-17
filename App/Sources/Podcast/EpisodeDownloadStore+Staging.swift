import Foundation

struct StagedDownloadOutput: Sendable, Equatable {
    let jobID: UUID
    let episodeID: UUID
    let inputVersion: String
    let contentHash: String
    let byteCount: Int64
    let fileURL: URL
    let manifestURL: URL
    let createdAt: Date
}

extension EpisodeDownloadStore {
    func stage(
        _ source: URL,
        episode: Episode,
        jobID: UUID,
        inputVersion: String,
        now: Date = Date()
    ) throws -> StagedDownloadOutput {
        let data = try Data(contentsOf: source, options: .mappedIfSafe)
        let hash = ArtifactRepository.hash(data)
        let directory = attemptsDirectory(for: episode.id)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let stem = "\(jobID.uuidString)-\(hash)"
        let file = directory.appendingPathComponent(
            "\(stem).\(fileExtension(for: episode))"
        )
        if FileManager.default.fileExists(atPath: file.path) {
            try? FileManager.default.removeItem(at: source)
        } else {
            try FileManager.default.moveItem(at: source, to: file)
        }
        let manifestURL = directory.appendingPathComponent("\(stem).json")
        let manifest = DownloadAttemptManifest(
            jobID: jobID,
            episodeID: episode.id,
            inputVersion: inputVersion,
            contentHash: hash,
            byteCount: Int64(data.count),
            fileName: file.lastPathComponent,
            createdAt: now
        )
        let encoded = try Self.stagingEncoder.encode(manifest)
        try encoded.write(to: manifestURL, options: .atomic)
        return output(manifest, manifestURL: manifestURL)
    }

    func verifiedStagedOutput(
        episodeID: UUID,
        jobID: UUID,
        inputVersion: String,
        contentHash: String? = nil
    ) -> StagedDownloadOutput? {
        recoverableStagedOutputs(episodeID: episodeID)
            .first {
                $0.jobID == jobID
                    && $0.inputVersion == inputVersion
                    && (contentHash == nil || $0.contentHash == contentHash)
            }
    }

    func recoverableStagedOutput(
        episodeID: UUID,
        inputVersion: String
    ) -> StagedDownloadOutput? {
        recoverableStagedOutputs(episodeID: episodeID)
            .first { $0.inputVersion == inputVersion }
    }

    func promote(_ staged: StagedDownloadOutput, episode: Episode) throws -> URL {
        guard staged.episodeID == episode.id,
              let verified = verified(staged) else {
            throw CocoaError(.fileReadCorruptFile)
        }
        let directory = rootURL
            .appendingPathComponent("artifacts", isDirectory: true)
            .appendingPathComponent(episode.id.uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let selected = directory.appendingPathComponent(
            "\(verified.contentHash).\(fileExtension(for: episode))"
        )
        if !FileManager.default.fileExists(atPath: selected.path) {
            try FileManager.default.copyItem(at: verified.fileURL, to: selected)
        }
        return selected
    }

    func discard(_ staged: StagedDownloadOutput) {
        try? FileManager.default.removeItem(at: staged.fileURL)
        try? FileManager.default.removeItem(at: staged.manifestURL)
    }

    private func recoverableStagedOutputs(episodeID: UUID) -> [StagedDownloadOutput] {
        let directory = attemptsDirectory(for: episodeID)
        let urls = (try? FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        )) ?? []
        return urls.filter { $0.pathExtension == "json" }.compactMap { manifestURL in
            guard let data = try? Data(contentsOf: manifestURL),
                  let manifest = try? Self.stagingDecoder.decode(
                    DownloadAttemptManifest.self, from: data
                  ),
                  manifest.episodeID == episodeID else { return nil }
            return verified(output(manifest, manifestURL: manifestURL))
        }.sorted { $0.createdAt > $1.createdAt }
    }

    private func verified(_ output: StagedDownloadOutput) -> StagedDownloadOutput? {
        guard let data = try? Data(contentsOf: output.fileURL, options: .mappedIfSafe),
              Int64(data.count) == output.byteCount,
              ArtifactRepository.hash(data) == output.contentHash else { return nil }
        return output
    }

    private func output(
        _ manifest: DownloadAttemptManifest,
        manifestURL: URL
    ) -> StagedDownloadOutput {
        StagedDownloadOutput(
            jobID: manifest.jobID,
            episodeID: manifest.episodeID,
            inputVersion: manifest.inputVersion,
            contentHash: manifest.contentHash,
            byteCount: manifest.byteCount,
            fileURL: manifestURL.deletingLastPathComponent()
                .appendingPathComponent(manifest.fileName),
            manifestURL: manifestURL,
            createdAt: manifest.createdAt
        )
    }

    private func attemptsDirectory(for episodeID: UUID) -> URL {
        rootURL.appendingPathComponent("attempts", isDirectory: true)
            .appendingPathComponent(episodeID.uuidString, isDirectory: true)
    }

    private static let stagingEncoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return encoder
    }()

    private static let stagingDecoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()
}

private struct DownloadAttemptManifest: Codable {
    let jobID: UUID
    let episodeID: UUID
    let inputVersion: String
    let contentHash: String
    let byteCount: Int64
    let fileName: String
    let createdAt: Date
}
