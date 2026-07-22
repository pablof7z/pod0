import Foundation
import Pod0Core

struct CoreDownloadNativeStore: @unchecked Sendable {
    enum StoreError: Error, Equatable {
        case invalidResumeKey
        case invalidArtifactKey
        case invalidStagedFile
        case coreStoreUnavailable
    }

    private let rootURL: URL
    private let fileManager: FileManager

    init(rootURL: URL? = nil, fileManager: FileManager = .default) {
        self.fileManager = fileManager
        self.rootURL = rootURL ?? Self.defaultRoot(fileManager: fileManager)
    }

    func stagedFile(for attemptID: DownloadAttemptId) -> URL? {
        let url = stagedURL(for: attemptID)
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]),
              values.isRegularFile == true,
              let count = values.fileSize,
              count > 0
        else { return nil }
        return url
    }

    func stage(_ source: URL, for attemptID: DownloadAttemptId) throws -> (URL, UInt64) {
        let destination = stagedURL(for: attemptID)
        try fileManager.createDirectory(
            at: destination.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        if fileManager.fileExists(atPath: destination.path) {
            try fileManager.removeItem(at: destination)
        }
        try fileManager.moveItem(at: source, to: destination)
        let values = try destination.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard values.isRegularFile == true, let count = values.fileSize, count > 0 else {
            try? fileManager.removeItem(at: destination)
            throw StoreError.invalidStagedFile
        }
        return (destination, UInt64(count))
    }

    func resumeKey(for attemptID: DownloadAttemptId) -> String {
        "v1/\(stableKey(attemptID)).resume"
    }

    func resumeData(for key: String?) -> Data? {
        guard let url = try? resumeURL(for: key) else { return nil }
        return try? Data(contentsOf: url, options: .mappedIfSafe)
    }

    func saveResumeData(_ data: Data?, for attemptID: DownloadAttemptId) {
        guard let data, !data.isEmpty else { return }
        let key = resumeKey(for: attemptID)
        guard let url = try? resumeURL(for: key) else { return }
        do {
            try fileManager.createDirectory(
                at: url.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try data.write(to: url, options: .atomic)
        } catch {
            return
        }
    }

    func importLegacyResumeData(_ data: Data, for attemptID: DownloadAttemptId) throws {
        guard !data.isEmpty else { return }
        let url = try resumeURL(for: resumeKey(for: attemptID))
        try fileManager.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try data.write(to: url, options: .atomic)
    }

    func artifactURL(
        coreStoreURL: URL?,
        artifactKey: String,
        expectedByteCount: UInt64? = nil
    ) -> URL? {
        guard let url = try? resolvedArtifactURL(
            coreStoreURL: coreStoreURL,
            artifactKey: artifactKey
        ),
        let values = try? url.resourceValues(
            forKeys: [.isSymbolicLinkKey, .isRegularFileKey, .fileSizeKey]
        ),
        values.isSymbolicLink != true,
        values.isRegularFile == true,
        let size = values.fileSize,
        size > 0,
        expectedByteCount.map({ UInt64(size) == $0 }) ?? true
        else { return nil }
        return url
    }

    func removeNativeFiles(for attemptID: DownloadAttemptId) {
        try? fileManager.removeItem(at: stagedURL(for: attemptID))
        let key = resumeKey(for: attemptID)
        if let resume = try? resumeURL(for: key) {
            try? fileManager.removeItem(at: resume)
        }
    }

    func removeArtifact(coreStoreURL: URL?, artifactKey: String) throws {
        let url = try resolvedArtifactURL(
            coreStoreURL: coreStoreURL,
            artifactKey: artifactKey
        )
        guard fileManager.fileExists(atPath: url.path) else { return }
        let values = try url.resourceValues(forKeys: [.isSymbolicLinkKey, .isRegularFileKey])
        guard values.isSymbolicLink != true, values.isRegularFile == true else {
            throw StoreError.invalidArtifactKey
        }
        try fileManager.removeItem(at: url)
    }

    private func resolvedArtifactURL(
        coreStoreURL: URL?,
        artifactKey: String
    ) throws -> URL {
        guard let coreStoreURL else { throw StoreError.coreStoreUnavailable }
        let components = artifactKey.split(separator: "/", omittingEmptySubsequences: false)
        guard components.count == 2,
              components[0] == "v1",
              !components[1].isEmpty,
              !components[1].contains(".."),
              !components[1].contains("\\")
        else { throw StoreError.invalidArtifactKey }
        let root = coreStoreURL.deletingLastPathComponent().appendingPathComponent(
            coreStoreURL.lastPathComponent + ".downloads",
            isDirectory: true
        )
        let url = root.appendingPathComponent(artifactKey, isDirectory: false)
        guard url.standardizedFileURL.path.hasPrefix(root.standardizedFileURL.path + "/") else {
            throw StoreError.invalidArtifactKey
        }
        return url
    }

    private func stagedURL(for attemptID: DownloadAttemptId) -> URL {
        rootURL.appendingPathComponent("staged", isDirectory: true)
            .appendingPathComponent(stableKey(attemptID) + ".media", isDirectory: false)
    }

    private func resumeURL(for key: String?) throws -> URL {
        guard let key else { throw StoreError.invalidResumeKey }
        let components = key.split(separator: "/", omittingEmptySubsequences: false)
        guard components.count == 2,
              components[0] == "v1",
              components[1].hasSuffix(".resume"),
              components[1].count == 39,
              components[1].dropLast(7).allSatisfy({ $0.isHexDigit })
        else { throw StoreError.invalidResumeKey }
        return rootURL.appendingPathComponent("resume", isDirectory: true)
            .appendingPathComponent(String(components[1]), isDirectory: false)
    }

    private func stableKey(_ attemptID: DownloadAttemptId) -> String {
        String(format: "%016llx%016llx", attemptID.high, attemptID.low)
    }

    private static func defaultRoot(fileManager: FileManager) -> URL {
        let base = (try? fileManager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )) ?? fileManager.temporaryDirectory
        return base.appendingPathComponent("podcastr/core-download-host-v1", isDirectory: true)
    }
}
