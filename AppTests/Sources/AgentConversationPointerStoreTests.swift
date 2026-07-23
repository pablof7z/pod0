import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class AgentConversationPointerStoreTests: XCTestCase {
    func testRoundTripAndClear() {
        let suiteName = "AgentConversationPointerStoreTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = AgentConversationPointerStore(defaults: defaults)
        let conversationID = ConversationId(high: 11, low: 29)

        XCTAssertNil(store.load())
        store.save(conversationID)
        XCTAssertEqual(store.load(), conversationID)
        store.save(nil)
        XCTAssertNil(store.load())
    }
}
