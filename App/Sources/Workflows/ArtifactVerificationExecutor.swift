import CryptoKit
import Foundation

enum ArtifactFileIntegrity: String, Sendable, Equatable {
    case available
    case missing
    case unreadable
    case sizeMismatch
    case hashMismatch
    case changedDuringRead
    case cancelled
}

struct ArtifactFileIdentity: Hashable, Sendable {
    let fileSystemNumber: UInt64?
    let fileNumber: UInt64?
    let byteCount: Int64
    let modificationTime: TimeInterval
}

struct ArtifactVerificationRequest: Sendable, Equatable {
    let artifactID: String
    let location: URL
    let expectedHash: String?
    let expectedSize: Int64?
    let schemaVersion: Int
    let cancellationID: UUID
}

struct ArtifactVerificationResult: Sendable, Equatable {
    let artifactID: String
    let cancellationID: UUID
    let observedHash: String?
    let observedSize: Int64?
    let integrity: ArtifactFileIntegrity
    let verifiedAt: Date?
    let fileIdentity: ArtifactFileIdentity?

    var isAvailable: Bool { integrity == .available }
}

/// Serializes file evidence work away from `@MainActor`. Verification streams
/// bounded chunks, checks cancellation between chunks, and caches only exact
/// immutable file identities that have already passed integrity checks.
actor ArtifactVerificationExecutor {
    static let shared = ArtifactVerificationExecutor()

    private struct CacheKey: Hashable {
        let location: String
        let expectedHash: String?
        let expectedSize: Int64?
        let schemaVersion: Int
        let identity: ArtifactFileIdentity
    }

    private let fileManager: FileManager
    private let now: @Sendable () -> Date
    private let onFileAccess: @Sendable () -> Void
    private let onFingerprintComplete: @Sendable (URL) -> Void
    private let maxCacheEntries: Int
    private var cache: [CacheKey: ArtifactVerificationResult] = [:]

    init(
        fileManager: FileManager = .default,
        now: @escaping @Sendable () -> Date = Date.init,
        onFileAccess: @escaping @Sendable () -> Void = {},
        onFingerprintComplete: @escaping @Sendable (URL) -> Void = { _ in },
        maxCacheEntries: Int = 256
    ) {
        self.fileManager = fileManager
        self.now = now
        self.onFileAccess = onFileAccess
        self.onFingerprintComplete = onFingerprintComplete
        self.maxCacheEntries = max(1, maxCacheEntries)
    }

    func verify(_ request: ArtifactVerificationRequest) -> ArtifactVerificationResult {
        if Task.isCancelled { return result(request, integrity: .cancelled) }
        guard let before = identity(at: request.location) else {
            let integrity: ArtifactFileIntegrity = fileManager.fileExists(
                atPath: request.location.path
            ) ? .unreadable : .missing
            return result(request, integrity: integrity)
        }
        let key = CacheKey(
            location: request.location.standardizedFileURL.path,
            expectedHash: request.expectedHash,
            expectedSize: request.expectedSize,
            schemaVersion: request.schemaVersion,
            identity: before
        )
        if let cached = cache[key] {
            return ArtifactVerificationResult(
                artifactID: request.artifactID,
                cancellationID: request.cancellationID,
                observedHash: cached.observedHash,
                observedSize: cached.observedSize,
                integrity: .available,
                verifiedAt: cached.verifiedAt,
                fileIdentity: before
            )
        }

        onFileAccess()
        let observation: (hash: String, size: Int64)
        do {
            observation = try fingerprint(request.location)
        } catch is CancellationError {
            return result(request, integrity: .cancelled)
        } catch {
            return result(request, integrity: .unreadable)
        }
        onFingerprintComplete(request.location)
        let after = identity(at: request.location)
        guard after == before else {
            return result(
                request, hash: observation.hash, size: observation.size,
                integrity: .changedDuringRead, identity: after
            )
        }
        guard let after else { return result(request, integrity: .changedDuringRead) }
        let integrity: ArtifactFileIntegrity
        if let expectedSize = request.expectedSize, expectedSize != observation.size {
            integrity = .sizeMismatch
        } else if let expectedHash = request.expectedHash, expectedHash != observation.hash {
            integrity = .hashMismatch
        } else {
            integrity = .available
        }
        let verified = result(
            request, hash: observation.hash, size: observation.size,
            integrity: integrity, identity: after
        )
        if verified.isAvailable {
            if cache.count >= maxCacheEntries, let evicted = cache.keys.first {
                cache[evicted] = nil
            }
            cache[key] = verified
        }
        return verified
    }

    private func fingerprint(_ url: URL) throws -> (hash: String, size: Int64) {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        var byteCount: Int64 = 0
        while true {
            try Task.checkCancellation()
            guard let data = try handle.read(upToCount: 1_048_576), !data.isEmpty else { break }
            hasher.update(data: data)
            byteCount += Int64(data.count)
        }
        try Task.checkCancellation()
        let hash = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        return (hash, byteCount)
    }

    private func identity(at url: URL) -> ArtifactFileIdentity? {
        guard let attributes = try? fileManager.attributesOfItem(atPath: url.path),
              let size = attributes[.size] as? NSNumber,
              let modified = attributes[.modificationDate] as? Date else { return nil }
        return ArtifactFileIdentity(
            fileSystemNumber: (attributes[.systemNumber] as? NSNumber)?.uint64Value,
            fileNumber: (attributes[.systemFileNumber] as? NSNumber)?.uint64Value,
            byteCount: size.int64Value,
            modificationTime: modified.timeIntervalSinceReferenceDate
        )
    }

    private func result(
        _ request: ArtifactVerificationRequest,
        hash: String? = nil,
        size: Int64? = nil,
        integrity: ArtifactFileIntegrity,
        identity: ArtifactFileIdentity? = nil
    ) -> ArtifactVerificationResult {
        ArtifactVerificationResult(
            artifactID: request.artifactID,
            cancellationID: request.cancellationID,
            observedHash: hash,
            observedSize: size,
            integrity: integrity,
            verifiedAt: integrity == .available ? now() : nil,
            fileIdentity: identity
        )
    }
}
