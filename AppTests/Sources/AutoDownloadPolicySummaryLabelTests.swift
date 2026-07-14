import XCTest
@testable import Podcastr

/// Coverage for `AutoDownloadPolicy.summaryLabel` — the compact badge
/// surfaced on Settings → Subscriptions list rows. The label is the only
/// place users can see at a glance which subscriptions are pulling bytes
/// automatically; the format must stay readable as it grows.
final class AutoDownloadPolicySummaryLabelTests: XCTestCase {

    func testOffReturnsNilSoCallersHideTheBadge() {
        let p = AutoDownloadPolicy(mode: .off, wifiOnly: true)
        XCTAssertNil(p.summaryLabel)
    }

    func testOffWithWifiOffStillReturnsNil() {
        // Wi-Fi guard is irrelevant when nothing is downloading; the label
        // is suppressed entirely so the row stays clean.
        let p = AutoDownloadPolicy(mode: .off, wifiOnly: false)
        XCTAssertNil(p.summaryLabel)
    }

    func testLatestNFormatsCount() {
        let p = AutoDownloadPolicy(mode: .latestN(3), wifiOnly: false)
        XCTAssertEqual(p.summaryLabel, "Latest 3")
    }

    func testLatestNWithWifiAppendsConstraint() {
        let p = AutoDownloadPolicy(mode: .latestN(5), wifiOnly: true)
        XCTAssertEqual(p.summaryLabel, "Latest 5 · Wi-Fi only")
    }

    func testAllNewBaseLabel() {
        let p = AutoDownloadPolicy(mode: .allNew, wifiOnly: false)
        XCTAssertEqual(p.summaryLabel, "All new")
    }

    func testAllNewWithWifiAppendsConstraint() {
        let p = AutoDownloadPolicy(mode: .allNew, wifiOnly: true)
        XCTAssertEqual(p.summaryLabel, "All new · Wi-Fi only")
    }

    func testLatestZeroIsStillDisplayedHonestly() {
        // Edge case: zero means "auto-download is on but matches nothing"
        // — a quirky configuration but the label shouldn't lie about it.
        let p = AutoDownloadPolicy(mode: .latestN(0), wifiOnly: false)
        XCTAssertEqual(p.summaryLabel, "Latest 0")
    }
}
