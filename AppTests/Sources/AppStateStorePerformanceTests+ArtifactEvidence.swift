import XCTest
@testable import Podcastr

extension AppStateStorePerformanceTests {
    func installDownloadEvidence(for episode: Episode) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("projection-\(episode.id.uuidString).mp3")
        let data = Data(repeating: 0x5a, count: 100)
        try data.write(to: url, options: .atomic)
        let hash = ArtifactRepository.hash(data)
        try ArtifactRepository(fileURL: store.persistence.episodeStore.fileURL).adopt(
            ArtifactRecord(
                kind: .downloadFile,
                subjectID: episode.id,
                inputVersion: DesiredStatePlanner.audioVersion(episode),
                outputVersion: hash,
                contentHash: hash,
                location: url.path,
                origin: "test",
                schemaVersion: 1,
                integrity: .available,
                verifiedAt: Date()
            )
        )
        downloadEvidenceURLs.append(url)
        return url
    }

    func installTranscriptEvidence(
        for episode: Episode,
        source: TranscriptSource
    ) throws {
        let transcript = Transcript(
            episodeID: episode.id,
            language: "en-US",
            source: source,
            segments: []
        )
        try TranscriptStore.shared.save(transcript)
        let url = TranscriptStore.shared.fileURL(for: episode.id)
        let data = try XCTUnwrap(TranscriptStore.shared.verifiedData(
            at: url,
            episodeID: episode.id
        ))
        let projectedSource: TranscriptState.Source = switch source {
        case .publisher: .publisher
        case .scribeV1: .scribe
        case .whisper: .whisper
        case .onDevice: .onDevice
        case .assemblyAI: .assemblyAI
        }
        let hash = ArtifactRepository.hash(data)
        try ArtifactRepository(fileURL: store.persistence.episodeStore.fileURL).adopt(
            ArtifactRecord(
                kind: .transcript,
                subjectID: episode.id,
                inputVersion: DesiredStatePlanner.audioVersion(episode),
                outputVersion: hash,
                contentHash: hash,
                location: url.path,
                origin: projectedSource.rawValue,
                schemaVersion: 1,
                integrity: .available,
                verifiedAt: Date()
            )
        )
        transcriptEvidenceIDs.append(episode.id)
    }
}
