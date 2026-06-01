import XCTest
@testable import Podcastr

/// Guards the kernel → Swift decode of the kernel-owned STT fallback policy
/// projection. The STT provider fallback policy lives in the Rust kernel; the
/// resolved value rides the snapshot as `effective_stt_provider`. This is the
/// FIRST consumer of a *computed* kernel-mirror string field, so the
/// `.convertFromSnakeCase` decode path (used by `KernelBridge`) must be proven
/// — a key mismatch would silently default `effectiveSttProvider` to
/// `"apple_native"` and turn the whole feature into a no-op that still builds
/// and passes every other test.
///
/// Note: the sibling `sttProvider` kernel-mirror CodingKey carries an explicit
/// snake_case raw value that is `.convertFromSnakeCase`-incompatible, so it does
/// NOT decode from the kernel snapshot — but it is unused (callers read the
/// Swift-side `Settings` struct, not this projection), so it is out of scope
/// here and deliberately not asserted on.
final class EffectiveSTTProviderDecodeTests: XCTestCase {

    /// Decode `SettingsSnapshot` exactly the way `KernelBridge` does.
    private func decode(_ json: String) throws -> SettingsSnapshot {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(SettingsSnapshot.self, from: Data(json.utf8))
    }

    func testEffectiveSttProviderDecodesFromSnakeCaseKey() throws {
        let json = """
        {"stt_provider":"elevenlabs_scribe","effective_stt_provider":"elevenlabs_scribe","effective_stt_provider_requires_key":true}
        """
        let settings = try decode(json)
        XCTAssertEqual(
            settings.effectiveSttProvider, "elevenlabs_scribe",
            "effective_stt_provider must decode through .convertFromSnakeCase — if this is apple_native the CodingKey did not match the converted key"
        )
        XCTAssertTrue(settings.effectiveSttProviderRequiresKey)
    }

    func testEffectiveSttProviderFallbackValueDecodes() throws {
        // Kernel resolved a keyless cloud selection down to apple_native.
        let json = """
        {"stt_provider":"assemblyai","effective_stt_provider":"apple_native","effective_stt_provider_requires_key":false}
        """
        let settings = try decode(json)
        XCTAssertEqual(settings.effectiveSttProvider, "apple_native")
        XCTAssertFalse(settings.effectiveSttProviderRequiresKey)
    }

    func testMissingFieldDefaultsToAppleNative() throws {
        // Older kernel snapshots without the field must default safely.
        let settings = try decode("{\"stt_provider\":\"openrouter_whisper\"}")
        XCTAssertEqual(settings.effectiveSttProvider, "apple_native")
        XCTAssertFalse(settings.effectiveSttProviderRequiresKey)
    }
}
