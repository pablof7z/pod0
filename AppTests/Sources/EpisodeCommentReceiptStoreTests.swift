import Foundation
import XCTest
@testable import Podcastr

final class EpisodeCommentReceiptStoreTests: XCTestCase {
    func testReceiptIndexSurvivesStoreRecreation() throws {
        let suite = "EpisodeCommentReceiptStoreTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let target = CommentTarget.episode(guid: "episode")
        let record = PendingEpisodeCommentReceipt(
            receiptID: 77,
            target: target,
            content: "Durable",
            submittedAt: Date(timeIntervalSince1970: 123)
        )

        UserDefaultsEpisodeCommentReceiptStore(defaults: defaults).save(record)
        let reopened = UserDefaultsEpisodeCommentReceiptStore(defaults: defaults)

        XCTAssertEqual(reopened.records(for: target), [record])
        reopened.remove(receiptID: 77)
        XCTAssertTrue(reopened.records(for: target).isEmpty)
    }

    func testCorruptIndexFailsClosed() throws {
        let suite = "EpisodeCommentReceiptStoreTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        defaults.set(Data("not-json".utf8), forKey: "test-receipts")

        let store = UserDefaultsEpisodeCommentReceiptStore(
            defaults: defaults,
            key: "test-receipts"
        )

        XCTAssertTrue(store.records(for: .episode(guid: "episode")).isEmpty)
    }
}
