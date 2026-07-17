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
    }

    func testEpisodeBeginsWithStableMissingEvidenceOnly() {
        XCTAssertEqual(store.episode(id: episode.id)?.downloadState, .notDownloaded)
        XCTAssertEqual(store.episode(id: episode.id)?.transcriptState, TranscriptState.none)
    }
}
