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
            eventID: "event-77",
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

    func testRemoveAllClearsEveryTargetAndPersistsTheReset() throws {
        let suite = "EpisodeCommentReceiptStoreTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = UserDefaultsEpisodeCommentReceiptStore(defaults: defaults)
        store.save(PendingEpisodeCommentReceipt(
            receiptID: 1,
            target: .episode(guid: "one"),
            eventID: nil,
            submittedAt: Date()
        ))
        store.save(PendingEpisodeCommentReceipt(
            receiptID: 2,
            target: .episode(guid: "two"),
            eventID: "event-two",
            submittedAt: Date()
        ))

        store.removeAll()
        let reopened = UserDefaultsEpisodeCommentReceiptStore(defaults: defaults)

        XCTAssertTrue(reopened.records(for: .episode(guid: "one")).isEmpty)
        XCTAssertTrue(reopened.records(for: .episode(guid: "two")).isEmpty)
    }
}
