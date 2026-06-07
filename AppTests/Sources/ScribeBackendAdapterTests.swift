import XCTest
@testable import Podcastr

/// Pins the Swift boundary for Rust-owned ElevenLabs Scribe transport. The
/// provider HTTP request is now tested in Rust; Swift only decodes the shared
/// envelope payload, maps stable backend error kinds, logs usage metadata, and
/// adapts Scribe words into the app's transcript model.
final class ScribeBackendAdapterTests: XCTestCase {

    func testDecodesRustScribeResultAndBuildsTranscript() throws {
        let data = """
        {
          "language_code": "en",
          "text": "Hello world",
          "words": [
            {"text":"Hello","start":0.0,"end":0.5,"type":"word","speaker_id":"spk_0"},
            {"text":"world","start":0.6,"end":1.0,"type":"word","speaker_id":"spk_0"}
          ],
          "model": "scribe_v2",
          "duration": 1.0,
          "latency_ms": 42
        }
        """.data(using: .utf8)!

        let raw = try JSONDecoder().decode(ScribeRawResult.self, from: data)
        XCTAssertEqual(raw.model, "scribe_v2")
        XCTAssertEqual(raw.duration, 1.0)
        XCTAssertEqual(raw.latencyMs, 42)

        let episodeID = UUID()
        let transcript = Transcript.fromScribeRaw(raw, episodeID: episodeID, languageHint: nil)

        XCTAssertEqual(transcript.episodeID, episodeID)
        XCTAssertEqual(transcript.language, "en")
        XCTAssertEqual(transcript.source, .scribeV1)
        XCTAssertEqual(transcript.segments.count, 1)
        XCTAssertEqual(transcript.segments.first?.text, "Hello world")
        XCTAssertEqual(transcript.segments.first?.words?.count, 2)
        XCTAssertEqual(transcript.speakers.count, 1)
    }

    func testAdapterDropsSpacingWordsAndSplitsOnSpeakerChange() {
        let raw = ScribeRawResult(
            language_code: nil,
            text: "ignored",
            words: [
                ScribeWord(text: "One", start: 0.0, end: 0.2, type: "word", speaker_id: "A"),
                ScribeWord(text: " ", start: 0.2, end: 0.2, type: "spacing", speaker_id: "A"),
                ScribeWord(text: "Two", start: 0.3, end: 0.5, type: "word", speaker_id: "B"),
            ],
            model: "scribe_v1",
            duration: 0.5,
            latencyMs: 10
        )

        let transcript = Transcript.fromScribeRaw(raw, episodeID: UUID(), languageHint: "es")

        XCTAssertEqual(transcript.language, "es")
        XCTAssertEqual(transcript.segments.map(\.text), ["One", "Two"])
        XCTAssertEqual(transcript.segments.flatMap { $0.words ?? [] }.map(\.text), ["One", "Two"])
        XCTAssertEqual(transcript.speakers.count, 2)
    }

    func testBackendErrorKindsMapToUserFacingErrorCases() {
        XCTAssertMissingAPIKey(
            ElevenLabsScribeClient.scribeError(from: .init(kind: "missing_api_key", message: nil, statusCode: nil))
        )
        XCTAssertInvalidAudioURL(
            ElevenLabsScribeClient.scribeError(from: .init(kind: "invalid_audio_url", message: nil, statusCode: nil))
        )
        XCTAssertTimedOut(
            ElevenLabsScribeClient.scribeError(from: .init(kind: "timed_out", message: nil, statusCode: nil))
        )
        XCTAssertKernelUnavailable(
            ElevenLabsScribeClient.scribeError(from: .init(kind: "store_unavailable", message: nil, statusCode: nil))
        )
    }

    func testBackendHTTPKindsPreserveStatusCode() {
        let invalid = ElevenLabsScribeClient.scribeError(
            from: .init(kind: "invalid_key", message: "bad key", statusCode: 401)
        )
        guard case .http(let invalidStatus, let invalidBody) = invalid else {
            return XCTFail("Expected HTTP error, got \(invalid)")
        }
        XCTAssertEqual(invalidStatus, 401)
        XCTAssertEqual(invalidBody, "bad key")

        let limited = ElevenLabsScribeClient.scribeError(
            from: .init(kind: "rate_limited", message: "slow down", statusCode: 429)
        )
        guard case .http(let limitedStatus, _) = limited else {
            return XCTFail("Expected HTTP error, got \(limited)")
        }
        XCTAssertEqual(limitedStatus, 429)
    }

    private func XCTAssertMissingAPIKey(
        _ error: ElevenLabsScribeClient.ScribeError,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .missingAPIKey = error else {
            return XCTFail("Expected missingAPIKey, got \(error)", file: file, line: line)
        }
    }

    private func XCTAssertInvalidAudioURL(
        _ error: ElevenLabsScribeClient.ScribeError,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .invalidAudioURL = error else {
            return XCTFail("Expected invalidAudioURL, got \(error)", file: file, line: line)
        }
    }

    private func XCTAssertTimedOut(
        _ error: ElevenLabsScribeClient.ScribeError,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .timedOut = error else {
            return XCTFail("Expected timedOut, got \(error)", file: file, line: line)
        }
    }

    private func XCTAssertKernelUnavailable(
        _ error: ElevenLabsScribeClient.ScribeError,
        file: StaticString = #filePath,
        line: UInt = #line
    ) {
        guard case .kernelUnavailable = error else {
            return XCTFail("Expected kernelUnavailable, got \(error)", file: file, line: line)
        }
    }
}
