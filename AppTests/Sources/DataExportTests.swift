import Foundation
import XCTest
@testable import Podcastr

final class DataExportTests: XCTestCase {

    func testDataExportRedactsLegacyOpenRouterKey() throws {
        var state = AppState()
        state.settings.legacyOpenRouterAPIKey = "sk-or-v1-leaked-secret"

        let payload = DataExport.makePayload(from: state)
        let data = try DataExport.encode(payload)
        let json = String(data: data, encoding: .utf8) ?? ""

        XCTAssertFalse(json.contains("sk-or-v1-leaked-secret"))
        XCTAssertNil(payload.state.settings.legacyOpenRouterAPIKey)
    }

    func testDataExportRoundTripsCoreRecords() throws {
        var state = AppState()
        let podcast = makeSubscription(title: "Round Trip Show")
        state.podcasts.append(podcast)
        state.subscriptions.append(PodcastSubscription(podcastID: podcast.id))
        state.notes.append(Note(text: "Sample note"))
        state.friends.append(Friend(displayName: "Alice", identifier: "alice_id"))

        let data = try DataExport.encode(DataExport.makePayload(from: state))

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let decoded = try decoder.decode(DataExport.Payload.self, from: data)

        XCTAssertEqual(decoded.schemaVersion, DataExport.currentSchemaVersion)
        XCTAssertEqual(decoded.state.podcasts.first?.title, "Round Trip Show")
        XCTAssertEqual(decoded.state.notes.first?.text, "Sample note")
        XCTAssertEqual(decoded.state.friends.first?.displayName, "Alice")
    }

    func testDataExportStatsExcludeDeletedNotes() {
        var state = AppState()
        let liveNote = Note(text: "Live note")
        var deletedNote = Note(text: "Tombstone note")
        deletedNote.deleted = true
        state.notes = [liveNote, deletedNote]

        let stats = DataExport.stats(for: state)

        XCTAssertEqual(stats.notes, 1)
    }

    func testDataExportSuggestedFilenameShape() {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd-HHmm"
        f.timeZone = TimeZone(identifier: "UTC")
        f.locale = Locale(identifier: "en_US_POSIX")
        let date = f.date(from: "2026-05-05-1430")!

        let name = DataExport.suggestedFilename(at: date)
        XCTAssertEqual(name, "Podcastr-Export-2026-05-05-1430.json")
    }

    func testDataExportWriteCreatesReadableFile() throws {
        var state = AppState()
        let podcast = makeSubscription(title: "Persisted")
        state.podcasts.append(podcast)
        state.subscriptions.append(PodcastSubscription(podcastID: podcast.id))

        let url = try DataExport.writeExport(of: state)
        defer { try? FileManager.default.removeItem(at: url) }

        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
        let data = try Data(contentsOf: url)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let decoded = try decoder.decode(DataExport.Payload.self, from: data)
        XCTAssertEqual(decoded.state.podcasts.first?.title, "Persisted")
    }

    private func makeSubscription(
        feedURL: URL = URL(string: "https://example.com/\(UUID().uuidString).xml")!,
        title: String = "Test Show"
    ) -> Podcast {
        Podcast(feedURL: feedURL, title: title)
    }
}
