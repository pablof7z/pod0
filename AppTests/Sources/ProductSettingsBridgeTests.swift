import Pod0Core
import XCTest
@testable import Podcastr

final class ProductSettingsBridgeTests: XCTestCase {
    func testPopulatedPortableSettingsRoundTripWithoutCredentialMetadata() throws {
        var legacy = Settings()
        legacy.agentDisplayName = "Migrated Agent"
        legacy.defaultPlaybackRate = 1.75
        legacy.skipForwardSeconds = 45
        legacy.autoSkipAds = true
        legacy.openRouterCredentialSource = .manual
        legacy.openRouterBYOKKeyID = "device-local"

        let values = try ProductSettingsBridge.values(from: legacy)
        XCTAssertEqual(values.agentDisplayName, "Migrated Agent")
        XCTAssertEqual(values.defaultPlaybackRateMilli, 1_750)
        XCTAssertEqual(values.skipForwardSeconds, 45)
        XCTAssertTrue(values.autoSkipAds)

        let projected = ProductSettingsBridge.applying(values, to: legacy)
        XCTAssertEqual(projected.agentDisplayName, "Migrated Agent")
        XCTAssertEqual(projected.openRouterCredentialSource, .manual)
        XCTAssertEqual(projected.openRouterBYOKKeyID, "device-local")
    }

    func testNativePersistenceRedactsPortableValuesButKeepsCredentialHandles() {
        var settings = Settings()
        settings.agentDisplayName = "Rust-owned"
        settings.skipForwardSeconds = 45
        settings.openRouterCredentialSource = .byok
        settings.openRouterBYOKKeyID = "opaque-handle"

        let metadata = ProductSettingsBridge.nativeMetadataOnly(from: settings)

        XCTAssertEqual(metadata.agentDisplayName, "")
        XCTAssertEqual(metadata.skipForwardSeconds, 30)
        XCTAssertEqual(metadata.openRouterCredentialSource, .byok)
        XCTAssertEqual(metadata.openRouterBYOKKeyID, "opaque-handle")
    }
}
