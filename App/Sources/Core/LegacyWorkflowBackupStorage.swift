import Darwin
import Foundation

/// Shared durability primitive for immutable, content-qualified workflow
/// migration manifests. Publication is no-clobber and flushes both the file
/// and directory entry before any source-row deletion may begin.
enum LegacyWorkflowBackupStorage {
    static func publish<Manifest: Codable & Equatable>(
        _ manifest: Manifest,
        to root: URL,
        destinationName: String,
        matchingPrefix: String,
        temporaryPrefix: String,
        validate: (Manifest) throws -> Void
    ) throws {
        try validate(manifest)
        let destination = root.appendingPathComponent(destinationName, isDirectory: false)
        if let existing: Manifest = try load(
            from: root,
            matchingPrefix: matchingPrefix,
            required: false,
            validate: validate
        ) {
            guard existing == manifest else {
                throw LegacyChapterWorkflowBackupError.backupConflict
            }
            try synchronizePublishedFile(at: destination, in: root)
            return
        }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let manifestData = try encodedData(manifest)
        let data = try encodedData(LegacyWorkflowBackupEnvelope(
            manifest: manifest,
            integrityDigest: ArtifactRepository.hash(manifestData)
        ))
        let temporary = root.appendingPathComponent(
            ".\(temporaryPrefix)-\(UUID().uuidString).tmp",
            isDirectory: false
        )
        defer { try? FileManager.default.removeItem(at: temporary) }
        try data.write(to: temporary, options: .atomic)
        do {
            try FileManager.default.linkItem(at: temporary, to: destination)
        } catch {
            if let existing: Manifest = try? load(
                from: root,
                matchingPrefix: matchingPrefix,
                validate: validate
            ), existing == manifest {
                try synchronizePublishedFile(at: destination, in: root)
                return
            }
            throw LegacyChapterWorkflowBackupError.backupConflict
        }
        try synchronizePublishedFile(at: destination, in: root)
        let verified: Manifest? = try load(
            from: root,
            matchingPrefix: matchingPrefix,
            validate: validate
        )
        guard verified == manifest else {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
    }

    static func load<Manifest: Codable>(
        from root: URL,
        matchingPrefix: String,
        required: Bool = true,
        validate: (Manifest) throws -> Void
    ) throws -> Manifest? {
        let files = (try? FileManager.default.contentsOfDirectory(
            at: root,
            includingPropertiesForKeys: nil
        ))?.filter {
            $0.lastPathComponent.hasPrefix(matchingPrefix) && $0.pathExtension == "json"
        } ?? []
        guard files.count <= 1 else {
            throw LegacyChapterWorkflowBackupError.backupConflict
        }
        guard let file = files.first else {
            if required { throw LegacyChapterWorkflowBackupError.backupMissing }
            return nil
        }
        let envelope: LegacyWorkflowBackupEnvelope<Manifest>
        do {
            envelope = try JSONDecoder().decode(
                LegacyWorkflowBackupEnvelope<Manifest>.self,
                from: Data(contentsOf: file)
            )
        } catch {
            throw LegacyChapterWorkflowBackupError.invalidBackup
        }
        guard ArtifactRepository.hash(try encodedData(envelope.manifest))
                == envelope.integrityDigest
        else { throw LegacyChapterWorkflowBackupError.invalidBackup }
        try validate(envelope.manifest)
        return envelope.manifest
    }

    static func encodedData<T: Encodable>(_ value: T) throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(value)
    }
}

private extension LegacyWorkflowBackupStorage {
    static func synchronizePublishedFile(at file: URL, in root: URL) throws {
        try synchronize(file, requestFullSync: true)
        try synchronize(root, requestFullSync: false)
    }

    static func synchronize(_ url: URL, requestFullSync: Bool) throws {
        let descriptor = Darwin.open(url.path, O_RDONLY)
        guard descriptor >= 0 else {
            throw LegacyChapterWorkflowBackupError.durabilityFailed
        }
        defer { _ = Darwin.close(descriptor) }
        if requestFullSync, Darwin.fcntl(descriptor, F_FULLFSYNC) == 0 { return }
        guard Darwin.fsync(descriptor) == 0 else {
            throw LegacyChapterWorkflowBackupError.durabilityFailed
        }
    }
}

private struct LegacyWorkflowBackupEnvelope<Manifest: Codable>: Codable {
    let manifest: Manifest
    let integrityDigest: String
}
