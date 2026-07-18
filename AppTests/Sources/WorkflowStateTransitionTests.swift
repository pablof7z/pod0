import Foundation
import XCTest
@testable import Podcastr

@MainActor
final class WorkflowStateTransitionTests: XCTestCase {
    private var stateURL: URL!
    private var rootURL: URL!
    private var store: AppStateStore!
    private var episode: Episode!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        stateURL = made.fileURL
        store = made.store
        rootURL = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: rootURL, withIntermediateDirectories: true)
        episode = Episode(
            podcastID: UUID(), guid: "transition", title: "Transition",
            pubDate: Date(), enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
        store.upsertEpisodes([episode], forPodcast: episode.podcastID)
    }

    override func tearDown() async throws {
        if let stateURL { AppStateTestSupport.disposeIsolatedStore(at: stateURL) }
        if let rootURL { try? FileManager.default.removeItem(at: rootURL) }
        episode = nil
        store = nil
        rootURL = nil
        stateURL = nil
        try await super.tearDown()
    }

    func testVerifiedDownloadRecoveryCanSkipProgressAndInvalidatesProjectionCentrally() throws {
        let file = rootURL.appendingPathComponent("audio.mp3")
        let data = Data("verified-audio".utf8)
        try data.write(to: file)
        let evidence = DownloadArtifactEvidence(
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            contentHash: ArtifactRepository.hash(data),
            fileURL: file,
            byteCount: Int64(data.count)
        )

        XCTAssertEqual(
            store.applyDownloadEvent(.artifactRecovered(evidence), episodeID: episode.id),
            .applied
        )
        XCTAssertTrue(store.hasDownloadedByShow.contains(episode.podcastID))
        XCTAssertEqual(
            store.applyDownloadEvent(.artifactRecovered(evidence), episodeID: episode.id),
            .noOp
        )
        XCTAssertEqual(
            store.applyDownloadEvent(
                .artifactInvalidated(inputVersion: "stale"),
                episodeID: episode.id
            ),
            .stale
        )
        XCTAssertTrue(store.hasDownloadedByShow.contains(episode.podcastID))
        XCTAssertEqual(
            store.applyDownloadEvent(
                .artifactInvalidated(inputVersion: evidence.inputVersion),
                episodeID: episode.id
            ),
            .applied
        )
        XCTAssertFalse(store.hasDownloadedByShow.contains(episode.podcastID))
        XCTAssertEqual(
            store.applyDownloadEvent(.userRemoved, episodeID: episode.id),
            .noOp
        )
    }

    func testDownloadRecoveryRejectsMissingEvidenceAndStaleVersion() {
        let missing = DownloadArtifactEvidence(
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            contentHash: "missing",
            fileURL: rootURL.appendingPathComponent("missing.mp3"),
            byteCount: 10
        )
        guard case .rejected = store.applyDownloadEvent(
            .artifactRecovered(missing), episodeID: episode.id
        ) else { return XCTFail("Missing file must be rejected") }

        let stale = DownloadArtifactEvidence(
            inputVersion: "old-audio", contentHash: "irrelevant",
            fileURL: missing.fileURL, byteCount: 0
        )
        XCTAssertEqual(
            store.applyDownloadEvent(.artifactRecovered(stale), episodeID: episode.id),
            .stale
        )

        let file = rootURL.appendingPathComponent("wrong-download.mp3")
        let data = Data("download".utf8)
        try? data.write(to: file)
        for evidence in [
            DownloadArtifactEvidence(
                inputVersion: DesiredStatePlanner.audioVersion(episode),
                contentHash: ArtifactRepository.hash(data),
                fileURL: file,
                byteCount: Int64(data.count + 1)
            ),
            DownloadArtifactEvidence(
                inputVersion: DesiredStatePlanner.audioVersion(episode),
                contentHash: "wrong-hash",
                fileURL: file,
                byteCount: Int64(data.count)
            ),
        ] {
            guard case .rejected = store.applyDownloadEvent(
                .artifactCommitted(evidence),
                episodeID: episode.id
            ) else { return XCTFail("Invalid download evidence must be rejected") }
        }
    }

    func testTranscriptAdoptionRequiresParseableCurrentArtifact() throws {
        let transcript = Transcript(
            episodeID: episode.id, language: "en-US", source: .publisher,
            segments: [.init(start: 0, end: 1, text: "Hello")]
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(transcript)
        let file = rootURL.appendingPathComponent("transcript.json")
        try data.write(to: file)
        let evidence = TranscriptArtifactEvidence(
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            contentHash: ArtifactRepository.hash(data),
            fileURL: file,
            source: .publisher
        )

        XCTAssertEqual(
            store.applyTranscriptEvent(.artifactAdopted(evidence), episodeID: episode.id),
            .applied
        )
        XCTAssertTrue(store.hasTranscribedByShow.contains(episode.podcastID))
        XCTAssertEqual(
            store.applyTranscriptEvent(.artifactAdopted(evidence), episodeID: episode.id),
            .noOp
        )

        var corrupt = evidence
        corrupt = .init(
            inputVersion: corrupt.inputVersion,
            contentHash: "wrong",
            fileURL: corrupt.fileURL,
            source: corrupt.source
        )
        guard case .rejected = store.applyTranscriptEvent(
            .artifactAdopted(corrupt), episodeID: episode.id
        ) else { return XCTFail("Hash mismatch must be rejected") }

        XCTAssertEqual(
            store.applyTranscriptEvent(
                .artifactInvalidated(inputVersion: "stale"),
                episodeID: episode.id
            ),
            .stale
        )
        XCTAssertTrue(store.hasTranscribedByShow.contains(episode.podcastID))
        XCTAssertEqual(
            store.applyTranscriptEvent(
                .artifactInvalidated(inputVersion: evidence.inputVersion),
                episodeID: episode.id
            ),
            .applied
        )
        XCTAssertFalse(store.hasTranscribedByShow.contains(episode.podcastID))
        XCTAssertEqual(
            store.applyTranscriptEvent(
                .artifactInvalidated(inputVersion: evidence.inputVersion),
                episodeID: episode.id
            ),
            .noOp
        )
    }

    func testTranscriptRejectsStaleMissingUnparseableAndWrongEpisodeEvidence() throws {
        let inputVersion = DesiredStatePlanner.audioVersion(episode)
        let missing = TranscriptArtifactEvidence(
            inputVersion: inputVersion,
            contentHash: "missing",
            fileURL: rootURL.appendingPathComponent("missing.json"),
            source: .publisher
        )
        guard case .rejected = store.applyTranscriptEvent(
            .artifactCommitted(missing),
            episodeID: episode.id
        ) else { return XCTFail("Missing transcript must be rejected") }

        let invalidURL = rootURL.appendingPathComponent("invalid.json")
        try Data("not-json".utf8).write(to: invalidURL)
        let invalidData = try Data(contentsOf: invalidURL)
        let invalid = TranscriptArtifactEvidence(
            inputVersion: inputVersion,
            contentHash: ArtifactRepository.hash(invalidData),
            fileURL: invalidURL,
            source: .publisher
        )
        guard case .rejected = store.applyTranscriptEvent(
            .artifactAdopted(invalid),
            episodeID: episode.id
        ) else { return XCTFail("Unparseable transcript must be rejected") }

        let wrongEpisodeTranscript = Transcript(
            episodeID: UUID(),
            language: "en-US",
            source: .publisher,
            segments: []
        )
        let wrongData = try encodedTranscript(wrongEpisodeTranscript)
        let wrongURL = rootURL.appendingPathComponent("wrong-episode.json")
        try wrongData.write(to: wrongURL)
        let wrong = TranscriptArtifactEvidence(
            inputVersion: inputVersion,
            contentHash: ArtifactRepository.hash(wrongData),
            fileURL: wrongURL,
            source: .publisher
        )
        guard case .rejected = store.applyTranscriptEvent(
            .artifactCommitted(wrong),
            episodeID: episode.id
        ) else { return XCTFail("Wrong-episode transcript must be rejected") }

        XCTAssertEqual(
            store.applyTranscriptEvent(.artifactCommitted(.init(
                inputVersion: "stale",
                contentHash: "irrelevant",
                fileURL: wrongURL,
                source: .publisher
            )), episodeID: episode.id),
            .stale
        )
    }

    func testAudioInputChangeClearsStableArtifactProjections() throws {
        let downloadURL = rootURL.appendingPathComponent("audio-current.mp3")
        let audioData = Data("audio".utf8)
        try audioData.write(to: downloadURL)
        XCTAssertEqual(store.applyDownloadEvent(.artifactCommitted(.init(
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            contentHash: ArtifactRepository.hash(audioData),
            fileURL: downloadURL,
            byteCount: Int64(audioData.count)
        )), episodeID: episode.id), .applied)

        let transcript = Transcript(
            episodeID: episode.id,
            language: "en-US",
            source: .publisher,
            segments: []
        )
        let transcriptData = try encodedTranscript(transcript)
        let transcriptURL = rootURL.appendingPathComponent("current-transcript.json")
        try transcriptData.write(to: transcriptURL)
        XCTAssertEqual(store.applyTranscriptEvent(.artifactCommitted(.init(
            inputVersion: DesiredStatePlanner.audioVersion(episode),
            contentHash: ArtifactRepository.hash(transcriptData),
            fileURL: transcriptURL,
            source: .publisher
        )), episodeID: episode.id), .applied)

        var refreshed = episode!
        refreshed.enclosureURL = URL(string: "https://example.com/replaced.mp3")!
        store.upsertEpisodes([refreshed], forPodcast: refreshed.podcastID)

        XCTAssertEqual(store.episode(id: episode.id)?.downloadState, .notDownloaded)
        XCTAssertEqual(store.episode(id: episode.id)?.transcriptState, .some(.none))
        XCTAssertFalse(store.hasDownloadedByShow.contains(episode.podcastID))
        XCTAssertFalse(store.hasTranscribedByShow.contains(episode.podcastID))
    }

    func testCompatibilitySettersRejectUnselectedEvidenceAndMissingEpisode() throws {
        let downloadURL = rootURL.appendingPathComponent("unselected.mp3")
        try Data("unselected".utf8).write(to: downloadURL)
        guard case .rejected = store.setEpisodeDownloadState(
            episode.id,
            state: .downloaded(
                localFileURL: downloadURL,
                byteCount: Int64(Data("unselected".utf8).count)
            )
        ) else { return XCTFail("Unselected download evidence must be rejected") }
        guard case .rejected = store.setEpisodeTranscriptState(
            episode.id,
            state: .ready(source: .publisher)
        ) else { return XCTFail("Unselected transcript evidence must be rejected") }
        guard case .rejected = store.applyDownloadEvent(
            .userRemoved,
            episodeID: UUID()
        ) else { return XCTFail("Unknown episode must be rejected") }
    }

    func testEpisodeBeginsWithStableMissingEvidenceOnly() {
        XCTAssertEqual(store.episode(id: episode.id)?.downloadState, .notDownloaded)
        XCTAssertEqual(store.episode(id: episode.id)?.transcriptState, TranscriptState.none)
    }

    private func encodedTranscript(_ transcript: Transcript) throws -> Data {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.sortedKeys]
        return try encoder.encode(transcript)
    }
}
