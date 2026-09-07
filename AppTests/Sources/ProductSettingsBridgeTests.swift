import Pod0Core
import XCTest
@testable import Podcastr

final class ProductSettingsBridgeTests: XCTestCase {
    @MainActor
    func testSettingsInteractionCommitsTypedRustStateAndRedactsNativeMetadata() throws {
        let made = AppStateTestSupport.makeIsolatedStore()
        defer { AppStateTestSupport.disposeIsolatedStore(at: made.fileURL) }
        let before = try XCTUnwrap(made.store.productSettingsProjection)
        var candidate = made.store.state.settings
        candidate.autoSkipAds.toggle()

        made.store.updateSettings(candidate)

        let committed = try XCTUnwrap(made.store.productSettingsProjection)
        XCTAssertGreaterThan(committed.revision.value, before.revision.value)
        XCTAssertEqual(committed.values.autoSkipAds, candidate.autoSkipAds)
        let metadata = made.store.persistence.metadataState(from: made.store.state)
        XCTAssertEqual(metadata.settings.autoSkipAds, Settings().autoSkipAds)
    }

    func testIntentDiffNamesOnlyChangedProductTargets() throws {
        let before = try ProductSettingsBridge.values(from: Settings())
        var candidate = Settings()
        candidate.autoSkipAds.toggle()
        candidate.skipForwardSeconds = 45

        let intents = try ProductSettingsBridge.intents(from: before, to: candidate)

        XCTAssertEqual(intents.count, 2)
        XCTAssertTrue(intents.contains(.setPlaybackToggle(
            setting: .autoSkipAds,
            enabled: candidate.autoSkipAds
        )))
        XCTAssertTrue(intents.contains(.setSkipIntervals(
            forwardSeconds: 45,
            backwardSeconds: UInt16(candidate.skipBackwardSeconds)
        )))
    }

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
