import AVFoundation
import XCTest
@testable import Podcastr

final class UITestSeederTests: XCTestCase {
    private var temporaryDirectories: [URL] = []

    override func tearDownWithError() throws {
        for directory in temporaryDirectories {
            try? FileManager.default.removeItem(at: directory)
        }
        temporaryDirectories.removeAll()
        try super.tearDownWithError()
    }

    func testBundledEpisodeDurationMatchesSeededMetadata() async throws {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "test-episode", withExtension: "mp3"))
        let asset = AVURLAsset(url: url)
        let mediaDuration = try await asset.load(.duration)
        let duration = mediaDuration.seconds

        XCTAssertEqual(duration, UITestSeeder.primaryEpisodeDurationSecs, accuracy: 1.0)
        XCTAssertGreaterThanOrEqual(duration, 4 * 60)
    }

    func testInstallSeededEpisodeAudioOverwritesStaleDownload() throws {
        let dir = makeTemporaryDirectory()
        let source = dir.appendingPathComponent("fresh.mp3")
        let dest = dir.appendingPathComponent("Downloads/stale.mp3")
        let fresh = Data("fresh-audio".utf8)
        let stale = Data("stale-audio".utf8)
        try FileManager.default.createDirectory(at: dest.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fresh.write(to: source)
        try stale.write(to: dest)

        let byteCount = UITestSeeder.installSeededEpisodeAudio(from: source, to: dest)

        XCTAssertEqual(byteCount, Int64(fresh.count))
        XCTAssertEqual(try Data(contentsOf: dest), fresh)
    }

    func testSeededDownloadURLUsesCanonicalDownloadStorePath() throws {
        let episodeID = try XCTUnwrap(UUID(uuidString: "A1A1FFFF-0001-0002-0001-000000000001"))
        let sourceURL = try XCTUnwrap(URL(string: "https://example.com/audio/default.mp3"))
        let episode = Episode(
            id: episodeID,
            podcastID: UUID(),
            guid: "seeded-episode",
            title: "Seeded Episode",
            pubDate: Date(timeIntervalSince1970: 0),
            enclosureURL: sourceURL,
            enclosureMimeType: "audio/mpeg"
        )

        let seededURL = UITestSeeder.seededDownloadURL(
            episodeID: episodeID.uuidString,
            sourceURL: sourceURL
        )
        let canonicalURL = EpisodeDownloadStore.shared.localFileURL(for: episode)

        XCTAssertEqual(seededURL.standardizedFileURL.path, canonicalURL.standardizedFileURL.path)
        XCTAssertTrue(seededURL.path.contains("/Downloads/"))
        XCTAssertFalse(seededURL.path.localizedCaseInsensitiveContains("/podcastr/downloads/"))
    }

    private func makeTemporaryDirectory() -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        temporaryDirectories.append(dir)
        return dir
    }
}
