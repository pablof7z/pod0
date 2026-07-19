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
            segments: [.init(start: 0, end: 1, text: "Performance fixture")]
        )
        let client = try XCTUnwrap(store.sharedLibrary)
        _ = try client.submitTranscriptObservation(
            transcript,
            context: TranscriptObservationContext(
                podcastID: episode.podcastID,
                sourceRevision: DesiredStatePlanner.audioVersion(episode),
                sourcePayloadDigest: ArtifactRepository.hash(Data("performance-fixture".utf8)),
                provider: nil
            )
        )
        client.attach(store: store)
    }
}
