import UIKit
@testable import Podcastr
import XCTest

@MainActor
final class AppShortcutRoutingTests: XCTestCase {
    private let bundleIdentifier = "io.f7z.podcast"

    func testKnownShortcutTypesMapToExistingDeepLinks() {
        XCTAssertEqual(
            AppDelegate.deepLinkURL(
                for: shortcut("open-agent"),
                bundleIdentifier: bundleIdentifier
            ),
            URL(string: "podcastr://agent")
        )
        XCTAssertEqual(
            AppDelegate.deepLinkURL(
                for: shortcut("settings"),
                bundleIdentifier: bundleIdentifier
            ),
            URL(string: "podcastr://settings")
        )
    }

    func testColdDeliveryStagesURLForRootView() {
        let delegate = AppDelegate()

        XCTAssertTrue(delegate.handleShortcut(
            shortcut("open-agent"),
            delivery: .pending,
            bundleIdentifier: bundleIdentifier
        ))
        XCTAssertEqual(delegate.pendingShortcutURL, URL(string: "podcastr://agent"))
    }

    func testWarmDeliveryPostsTheSameMappedURL() {
        let delegate = AppDelegate()
        let center = NotificationCenter()
        let received = LockedURL()
        let token = center.addObserver(
            forName: AppDelegate.shortcutURLNotification,
            object: nil,
            queue: nil
        ) { note in
            received.set(note.object as? URL)
        }
        defer { center.removeObserver(token) }

        XCTAssertTrue(delegate.handleShortcut(
            shortcut("settings"),
            delivery: .notification,
            bundleIdentifier: bundleIdentifier,
            notificationCenter: center
        ))
        XCTAssertEqual(received.value, URL(string: "podcastr://settings"))
    }

    func testUnknownShortcutFailsWithoutNavigation() {
        let delegate = AppDelegate()
        XCTAssertFalse(delegate.handleShortcut(
            shortcut("unknown"),
            delivery: .pending,
            bundleIdentifier: bundleIdentifier
        ))
        XCTAssertNil(delegate.pendingShortcutURL)
    }

    private func shortcut(_ suffix: String) -> UIApplicationShortcutItem {
        UIApplicationShortcutItem(
            type: "\(bundleIdentifier).\(suffix)",
            localizedTitle: suffix
        )
    }
}

private final class LockedURL: @unchecked Sendable {
    private let lock = NSLock()
    private var storedValue: URL?

    var value: URL? {
        lock.withLock { storedValue }
    }

    func set(_ value: URL?) {
        lock.withLock { storedValue = value }
    }
}
