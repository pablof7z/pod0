import XCTest
@testable import Podcastr

/// Pins the `ElevenLabsScribeClient` multipart encoding to the OpenAPI spec
/// at `https://api.elevenlabs.io/openapi.json`. Without these tests, the
/// encoder can quietly drift back into the bug that broke every transcription
/// in production for months: passing the publisher's HTTPS enclosure URL into
/// the multipart `file` field via `Data(contentsOf:)`, which silently
/// downloaded the entire MP3 onto the actor and timed out.
final class ScribeRequestEncodingTests: XCTestCase {

    // MARK: - Local file → multipart `file` field

    /// When the audio is a `file://` URL, the multipart body must include
    /// a binary `file` part with the bytes, plus the filename, plus a
    /// `Content-Type` (audio/...). It must NOT include a `source_url` field.
    func testMultipartBodyForLocalFileEncodesFilePart() throws {
        let tmpDir = FileManager.default.temporaryDirectory
        let tmpURL = tmpDir.appendingPathComponent("scribe-test-\(UUID()).mp3")
        let payload = Data([0xFF, 0xFB, 0x90, 0x44]) // mp3-ish magic bytes
        try payload.write(to: tmpURL)
        defer { try? FileManager.default.removeItem(at: tmpURL) }

        let audio = try ElevenLabsScribeClient.audioField(for: tmpURL)
        let body = try ElevenLabsScribeClient.multipartBody(
            boundary: "TestBoundary",
            modelID: "scribe_v2",
            languageHint: "en",
            audio: audio
        )
        let bodyString = String(data: body, encoding: .isoLatin1) ?? ""

        XCTAssertTrue(bodyString.contains("name=\"model_id\""),
                      "model_id field is required by /v1/speech-to-text")
        XCTAssertTrue(bodyString.contains("scribe_v2"),
                      "model_id must be one of {scribe_v1, scribe_v2}")
        XCTAssertTrue(bodyString.contains("name=\"file\"; filename=\"\(tmpURL.lastPathComponent)\""),
                      "binary audio must go in the `file` field, not `audio` or anything else")
        XCTAssertTrue(bodyString.contains("Content-Type: audio/mpeg"),
                      "mp3 file should be tagged as audio/mpeg in the part headers")
        XCTAssertTrue(bodyString.contains("name=\"timestamps_granularity\""))
        XCTAssertTrue(bodyString.contains("name=\"diarize\""))
        XCTAssertTrue(bodyString.contains("name=\"tag_audio_events\""))
        XCTAssertTrue(bodyString.contains("name=\"language_code\""))
        XCTAssertTrue(bodyString.contains("\r\nen\r\n"),
                      "language_code value should be the ISO-639-1 hint")
        XCTAssertFalse(bodyString.contains("name=\"source_url\""),
                       "must NOT send source_url when we have a local file")
        XCTAssertTrue(bodyString.hasSuffix("--TestBoundary--\r\n"),
                      "multipart body must terminate with the closing boundary")

        // The mp3 magic bytes must be embedded verbatim somewhere in the body
        // — proves we're sending bytes, not a string representation of the
        // file URL (the production bug).
        XCTAssertTrue(body.range(of: payload) != nil,
                      "multipart body must contain the actual audio bytes")
    }

    // MARK: - Remote URL → multipart `source_url` field

    /// When the audio is an HTTPS URL, the multipart body must include the
    /// URL as a TEXT field named `source_url`, NOT as a binary `file` part.
    /// This is the path used when the episode hasn't been downloaded — we
    /// hand the URL to ElevenLabs and let it fetch server-side.
    func testMultipartBodyForRemoteURLUsesSourceURL() throws {
        let remote = URL(string: "https://traffic.megaphone.fm/HSW1234567890.mp3")!
        let audio = try ElevenLabsScribeClient.audioField(for: remote)

        let body = try ElevenLabsScribeClient.multipartBody(
            boundary: "B2",
            modelID: "scribe_v2",
            languageHint: nil,
            audio: audio
        )
        let bodyString = String(data: body, encoding: .utf8) ?? ""

        XCTAssertTrue(bodyString.contains("name=\"source_url\""),
                      "remote URL must go in the `source_url` text field")
        XCTAssertTrue(bodyString.contains(remote.absoluteString),
                      "source_url value must be the literal URL string")
        XCTAssertFalse(bodyString.contains("name=\"file\""),
                       "must NOT also send a binary `file` field — exactly one of file/source_url is allowed")
        XCTAssertFalse(bodyString.contains("name=\"language_code\""),
                       "language_code must be omitted entirely when no hint provided (not sent as empty)")
    }

    // MARK: - Audio field discriminator

    /// The picker must reject anything that isn't a real local file or a
    /// real HTTP(S) URL. Otherwise we'd silently encode garbage as a
    /// `source_url` and get a 422 from the server.
    func testAudioFieldRejectsNonExistentLocalFile() {
        let bogus = URL(fileURLWithPath: "/var/tmp/this-file-definitely-does-not-exist-\(UUID())")
        XCTAssertThrowsError(try ElevenLabsScribeClient.audioField(for: bogus)) { error in
            guard case ElevenLabsScribeClient.ScribeError.invalidAudioURL = error else {
                XCTFail("Expected .invalidAudioURL, got \(error)")
                return
            }
        }
    }

    func testAudioFieldRejectsUnsupportedScheme() {
        let weird = URL(string: "ftp://example.com/file.mp3")!
        XCTAssertThrowsError(try ElevenLabsScribeClient.audioField(for: weird))
    }

    // MARK: - Request shape end-to-end (via stub URLProtocol)

    /// Captures the actual `URLRequest` that `submit` issues — proves the
    /// endpoint, headers, timeout, and body all match the spec at the
    /// transport layer, not just at the helper layer.
    func testSubmitIssuesPOSTToCorrectEndpointWithLongTimeout() async throws {
        let stub = ScribeStubProtocol.self
        stub.reset()
        stub.responseStatus = 200
        stub.responseBody = #"{"language_code":"en","text":"Hello world","words":[{"text":"Hello","start":0.0,"end":0.5,"type":"word","speaker_id":"spk_0"}]}"#
            .data(using: .utf8)!

        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [stub] + (config.protocolClasses ?? [])
        let session = URLSession(configuration: config)

        let client = ElevenLabsScribeClient(
            baseURL: URL(string: "https://api.elevenlabs.io")!,
            modelID: "scribe_v2",
            session: session,
            credential: { "test-key-xyz" }
        )

        let remote = URL(string: "https://traffic.megaphone.fm/episode.mp3")!
        let job = try await client.submit(audioURL: remote, episodeID: UUID(), languageHint: nil)

        XCTAssertNotNil(job.inlineResult, "submit must populate inlineResult on the sync path")
        XCTAssertEqual(job.inlineResult?.text, "Hello world")
        XCTAssertEqual(job.inlineResult?.words?.count, 1)

        let captured = try XCTUnwrap(stub.lastRequest, "ScribeStubProtocol must capture the request")
        XCTAssertEqual(captured.url?.absoluteString, "https://api.elevenlabs.io/v1/speech-to-text")
        XCTAssertEqual(captured.httpMethod, "POST")
        XCTAssertEqual(captured.value(forHTTPHeaderField: "xi-api-key"), "test-key-xyz")
        let contentType = captured.value(forHTTPHeaderField: "Content-Type") ?? ""
        XCTAssertTrue(contentType.hasPrefix("multipart/form-data; boundary="),
                      "Content-Type must let URLSession see the boundary; got \(contentType)")
        XCTAssertGreaterThanOrEqual(captured.timeoutInterval, ElevenLabsScribeClient.requestTimeout,
                                    "Scribe is slow — request must use the long timeout, not the 60s default")

        let body = try XCTUnwrap(stub.lastBody, "captured body must be present")
        let s = String(data: body, encoding: .utf8) ?? ""
        XCTAssertTrue(s.contains("name=\"source_url\""))
        XCTAssertTrue(s.contains(remote.absoluteString))
        XCTAssertTrue(s.contains("name=\"model_id\""))
        XCTAssertTrue(s.contains("scribe_v2"))
    }

    /// A 401 must surface as `.http(status: 401, ...)` so the user gets the
    /// "ElevenLabs rejected your API key" message, not a silent empty
    /// transcript. (The old code path would `try?` decode error JSON as a
    /// transcript and return zero segments — which the user saw as "Scribe
    /// did nothing.")
    func testSubmitSurfacesNon2xxAsHTTPError() async throws {
        let stub = ScribeStubProtocol.self
        stub.reset()
        stub.responseStatus = 401
        stub.responseBody = #"{"detail":"Invalid API key"}"#.data(using: .utf8)!

        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [stub] + (config.protocolClasses ?? [])
        let session = URLSession(configuration: config)

        let client = ElevenLabsScribeClient(
            baseURL: URL(string: "https://api.elevenlabs.io")!,
            modelID: "scribe_v2",
            session: session,
            credential: { "bad-key" }
        )

        do {
            _ = try await client.submit(
                audioURL: URL(string: "https://example.com/x.mp3")!,
                episodeID: UUID(),
                languageHint: nil
            )
            XCTFail("Expected throw on 401")
        } catch let ElevenLabsScribeClient.ScribeError.http(status, _) {
            XCTAssertEqual(status, 401)
        } catch {
            XCTFail("Expected ScribeError.http, got \(error)")
        }
    }

    /// Missing API key must be detected before any network call so the user
    /// gets the right error and we don't waste a network round-trip.
    func testSubmitThrowsMissingAPIKeyWithoutNetwork() async throws {
        let client = ElevenLabsScribeClient(
            baseURL: URL(string: "https://api.elevenlabs.io")!,
            modelID: "scribe_v2",
            session: .shared,
            credential: { nil }
        )
        do {
            _ = try await client.submit(
                audioURL: URL(string: "https://example.com/x.mp3")!,
                episodeID: UUID(),
                languageHint: nil
            )
            XCTFail("Expected missingAPIKey")
        } catch ElevenLabsScribeClient.ScribeError.missingAPIKey {
            // expected
        } catch {
            XCTFail("Expected ScribeError.missingAPIKey, got \(error)")
        }
    }
}

// MARK: - Stub URLProtocol

/// Captures outgoing `URLRequest`s + the uploaded body, then returns a
/// canned response. Lives in test bundle only.
final class ScribeStubProtocol: URLProtocol, @unchecked Sendable {

    nonisolated(unsafe) static var lastRequest: URLRequest?
    nonisolated(unsafe) static var lastBody: Data?
    nonisolated(unsafe) static var responseStatus: Int = 200
    nonisolated(unsafe) static var responseBody: Data = Data()

    static func reset() {
        lastRequest = nil
        lastBody = nil
        responseStatus = 200
        responseBody = Data()
    }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        // `URLSession.upload(for:from:)` puts the body on `httpBodyStream`
        // rather than `httpBody`, so we must drain the stream to capture it.
        var captured = request
        if let stream = request.httpBodyStream {
            stream.open()
            defer { stream.close() }
            var data = Data()
            let bufferSize = 16 * 1024
            let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: bufferSize)
            defer { buffer.deallocate() }
            while stream.hasBytesAvailable {
                let read = stream.read(buffer, maxLength: bufferSize)
                if read <= 0 { break }
                data.append(buffer, count: read)
            }
            Self.lastBody = data
            captured.httpBody = data
        } else if let body = request.httpBody {
            Self.lastBody = body
        }
        Self.lastRequest = captured

        let response = HTTPURLResponse(
            url: request.url ?? URL(string: "https://stub.test/")!,
            statusCode: Self.responseStatus,
            httpVersion: "HTTP/1.1",
            headerFields: ["Content-Type": "application/json"]
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.responseBody)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}
