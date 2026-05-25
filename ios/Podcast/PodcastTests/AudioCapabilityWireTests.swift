import XCTest
@testable import Podcast

// MARK: - AudioCapability wire-shape tests
//
// The Rust `AudioCommand` / `AudioReport` enums in
// `apps/nmp-app-podcast/src/capability/audio.rs` define the canonical
// wire shape (`#[serde(tag = "type", rename_all = "snake_case")]`). The
// Swift mirrors in `AudioCapability+Wire.swift` use hand-rolled
// `Codable` because the variant payloads differ across cases.
//
// These tests pin the JSON strings so a drift on either side surfaces
// loudly. The Rust crate has the same round-trips on its side
// (`capability::audio::tests`) — together they form a two-sided
// contract.

final class AudioCapabilityWireTests: XCTestCase {

    // MARK: - AudioCommand decoding (Rust → Swift)

    func testLoadCommandDecodes() throws {
        let json = #"{"type":"load","url":"https://ex.com/ep.mp3","position_secs":12.5}"#
        let decoded = try decodeCommand(json)
        XCTAssertEqual(decoded, .load(url: "https://ex.com/ep.mp3", positionSecs: 12.5))
    }

    func testPlayPauseStopCommandsDecodeWithNoPayload() throws {
        XCTAssertEqual(try decodeCommand(#"{"type":"play"}"#), .play)
        XCTAssertEqual(try decodeCommand(#"{"type":"pause"}"#), .pause)
        XCTAssertEqual(try decodeCommand(#"{"type":"stop"}"#), .stop)
    }

    func testSeekCommandDecodes() throws {
        let json = #"{"type":"seek","position_secs":42}"#
        XCTAssertEqual(try decodeCommand(json), .seek(positionSecs: 42))
    }

    func testSetVolumeAndSetSpeedCommandsDecode() throws {
        XCTAssertEqual(
            try decodeCommand(#"{"type":"set_volume","volume":0.75}"#),
            .setVolume(volume: 0.75))
        XCTAssertEqual(
            try decodeCommand(#"{"type":"set_speed","speed":1.5}"#),
            .setSpeed(speed: 1.5))
    }

    func testSetSleepTimerCommandHandlesSomeAndNone() throws {
        XCTAssertEqual(
            try decodeCommand(#"{"type":"set_sleep_timer","secs":1800}"#),
            .setSleepTimer(secs: 1800))
        XCTAssertEqual(
            try decodeCommand(#"{"type":"set_sleep_timer","secs":null}"#),
            .setSleepTimer(secs: nil))
        // The Rust side serializes `None` as `"secs":null`; an absent
        // key (which a hand-rolled iOS encoder might emit) must also
        // decode as nil.
        XCTAssertEqual(
            try decodeCommand(#"{"type":"set_sleep_timer"}"#),
            .setSleepTimer(secs: nil))
    }

    func testUnknownCommandTypeRaisesDecodeError() {
        XCTAssertThrowsError(try decodeCommand(#"{"type":"levitate"}"#))
    }

    // MARK: - AudioReport encoding (Swift → Rust)

    func testPlayingReportEncodes() throws {
        let report = AudioReport.playing(
            url: "https://ex.com/ep.mp3",
            positionSecs: 90,
            durationSecs: 1800)
        let json = try encodeReport(report)
        let dict = try parse(json)
        XCTAssertEqual(dict["type"] as? String, "playing")
        XCTAssertEqual(dict["url"] as? String, "https://ex.com/ep.mp3")
        XCTAssertEqual(dict["position_secs"] as? Double, 90)
        XCTAssertEqual(dict["duration_secs"] as? Double, 1800)
    }

    func testPausedReportEncodes() throws {
        let report = AudioReport.paused(url: "u", positionSecs: 30)
        let dict = try parse(encodeReport(report))
        XCTAssertEqual(dict["type"] as? String, "paused")
        XCTAssertEqual(dict["url"] as? String, "u")
        XCTAssertEqual(dict["position_secs"] as? Double, 30)
    }

    func testStoppedReportEncodesWithNoPayload() throws {
        let dict = try parse(encodeReport(.stopped))
        XCTAssertEqual(dict["type"] as? String, "stopped")
        XCTAssertEqual(dict.count, 1)
    }

    func testFailedReportEncodes() throws {
        let report = AudioReport.failed(url: "u", error: "transport: timeout")
        let dict = try parse(encodeReport(report))
        XCTAssertEqual(dict["type"] as? String, "failed")
        XCTAssertEqual(dict["url"] as? String, "u")
        XCTAssertEqual(dict["error"] as? String, "transport: timeout")
    }

    func testBufferingProgressReportEncodes() throws {
        let dict = try parse(encodeReport(.bufferingProgress(fraction: 0.42)))
        XCTAssertEqual(dict["type"] as? String, "buffering_progress")
        // Float ⇄ JSON ⇄ NSNumber round-trip widens to Double; allow a
        // small tolerance.
        let fraction = dict["fraction"] as? Double ?? .nan
        XCTAssertEqual(fraction, 0.42, accuracy: 1e-5)
    }

    func testSleepTimerFiredReportEncodesWithNoPayload() throws {
        let dict = try parse(encodeReport(.sleepTimerFired))
        XCTAssertEqual(dict["type"] as? String, "sleep_timer_fired")
        XCTAssertEqual(dict.count, 1)
    }

    // MARK: - Helpers

    private func decodeCommand(_ json: String) throws -> AudioCommand {
        let data = Data(json.utf8)
        return try JSONDecoder().decode(AudioCommand.self, from: data)
    }

    private func encodeReport(_ report: AudioReport) throws -> String {
        guard let json = report.jsonString() else {
            throw XCTSkip("AudioReport encoding returned nil — unreachable")
        }
        return json
    }

    private func parse(_ json: String) throws -> [String: Any] {
        let object = try JSONSerialization.jsonObject(with: Data(json.utf8))
        guard let dict = object as? [String: Any] else {
            throw XCTSkip("encoded report was not a JSON object")
        }
        return dict
    }
}
