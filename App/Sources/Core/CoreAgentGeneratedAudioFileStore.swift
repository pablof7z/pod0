import CryptoKit
import Foundation
import Pod0Core

actor CoreAgentGeneratedAudioFileStore {
    enum StoreError: Error {
        case invalidTarget
        case missingRecoveryArtifact
        case emptyArtifact
        case artifactTooLarge
    }

    private let explicitDirectory: URL?

    init(directory: URL? = nil) {
        explicitDirectory = directory
    }

    func stage(
        target: AgentGeneratedAudioTarget,
        mode: AgentCapabilityExecutionMode,
        script: String,
        voiceID: String,
        tts: any TTSClientProtocol
    ) async throws -> AgentGeneratedAudioEvidence {
        let directory = try resolvedDirectory()
        let finalURL = try Self.artifactURL(target.artifactId, in: directory)
        if FileManager.default.fileExists(atPath: finalURL.path) {
            return try evidence(at: finalURL, target: target)
        }
        guard mode == .perform else {
            throw StoreError.missingRecoveryArtifact
        }
        guard tts.isConfigured else {
            throw ElevenLabsTTSError.missingAPIKey
        }

        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true
        )
        let temporaryURL = directory.appendingPathComponent(
            ".\(UUID().uuidString).partial",
            isDirectory: false
        )
        guard FileManager.default.createFile(atPath: temporaryURL.path, contents: nil) else {
            throw StoreError.invalidTarget
        }
        do {
            let handle = try FileHandle(forWritingTo: temporaryURL)
            defer { try? handle.close() }
            var byteCount: UInt64 = 0
            for try await chunk in tts.synthesizeStream(text: script, voiceID: voiceID) {
                try Task.checkCancellation()
                guard !chunk.isEmpty else { continue }
                let next = byteCount + UInt64(chunk.count)
                guard next <= target.maximumBytes else {
                    throw StoreError.artifactTooLarge
                }
                try handle.write(contentsOf: chunk)
                byteCount = next
            }
            guard byteCount > 0 else { throw StoreError.emptyArtifact }
            try handle.synchronize()
            try handle.close()
            if FileManager.default.fileExists(atPath: finalURL.path) {
                try FileManager.default.removeItem(at: temporaryURL)
            } else {
                try FileManager.default.moveItem(at: temporaryURL, to: finalURL)
            }
            return try evidence(at: finalURL, target: target)
        } catch {
            try? FileManager.default.removeItem(at: temporaryURL)
            throw error
        }
    }

    nonisolated static func currentURL(for persistedURL: URL) throws -> URL {
        try audioDirectory().appendingPathComponent(
            persistedURL.lastPathComponent,
            isDirectory: false
        )
    }

    private func resolvedDirectory() throws -> URL {
        if let explicitDirectory { return explicitDirectory }
        return try Self.audioDirectory()
    }

    private static func audioDirectory() throws -> URL {
        let support = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        return support
            .appendingPathComponent("podcastr", isDirectory: true)
            .appendingPathComponent("agent-episodes", isDirectory: true)
    }

    private static func artifactURL(
        _ artifactID: GeneratedArtifactId,
        in directory: URL
    ) throws -> URL {
        guard let uuid = artifactID.uuid else { throw StoreError.invalidTarget }
        return directory.appendingPathComponent("\(uuid.uuidString).mp3", isDirectory: false)
    }

    private func evidence(
        at url: URL,
        target: AgentGeneratedAudioTarget
    ) throws -> AgentGeneratedAudioEvidence {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        var byteCount: UInt64 = 0
        while let data = try handle.read(upToCount: 64 * 1_024), !data.isEmpty {
            byteCount += UInt64(data.count)
            guard byteCount <= target.maximumBytes else {
                throw StoreError.artifactTooLarge
            }
            hasher.update(data: data)
        }
        guard byteCount > 0 else { throw StoreError.emptyArtifact }
        let hexadecimal = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        guard let digest = ContentDigest(hexadecimal: hexadecimal) else {
            throw StoreError.invalidTarget
        }
        return AgentGeneratedAudioEvidence(
            artifactId: target.artifactId,
            fileUrl: url.absoluteString,
            mediaType: "audio/mpeg",
            byteCount: byteCount,
            contentDigest: digest,
            durationMilliseconds: nil
        )
    }
}
