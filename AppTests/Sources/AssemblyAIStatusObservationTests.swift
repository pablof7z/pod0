import Foundation
import XCTest
@testable import Podcastr

final class AssemblyAIStatusObservationTests: XCTestCase {
    override func setUp() {
        super.setUp()
        ChapterProviderStubProtocol.reset()
    }

    override func tearDown() {
        ChapterProviderStubProtocol.reset()
        super.tearDown()
    }

    func testObservePerformsOneStatusReadAndReturnsPending() async throws {
        ChapterProviderStubProtocol.responseBody = Data(
            #"{"id":"operation-1","status":"processing"}"#.utf8
        )
        let session = makeSession()
        let client = AssemblyAITranscriptClient(
            baseURL: URL(string: "https://assembly.example.test")!,
            session: session,
            credential: { "authorized-key" }
        )

        let observation = try await client.observe(job(), maximumResponseBytes: 4_096)

        guard case .pending(let status) = observation else {
            return XCTFail("Expected one pending observation")
        }
        XCTAssertEqual(status, "processing")
        XCTAssertEqual(ChapterProviderStubProtocol.requestCount, 1)
        XCTAssertEqual(ChapterProviderStubProtocol.lastRequest?.httpMethod, "GET")
        XCTAssertEqual(
            ChapterProviderStubProtocol.lastRequest?.url?.path,
            "/v2/transcript/operation-1"
        )
        session.invalidateAndCancel()
    }

    func testObserveMapsCompletedPayloadWithoutAnotherRequest() async throws {
        ChapterProviderStubProtocol.responseBody = Data(#"""
        {
          "id":"operation-1",
          "status":"completed",
          "language_code":"en",
          "utterances":[{
            "start":0,
            "end":1000,
            "text":"Bounded result",
            "speaker":"A",
            "words":[]
          }]
        }
        """#.utf8)
        let session = makeSession()
        let client = AssemblyAITranscriptClient(
            baseURL: URL(string: "https://assembly.example.test")!,
            session: session,
            credential: { "authorized-key" }
        )

        let observation = try await client.observe(job(), maximumResponseBytes: 16_384)

        guard case .completed(let transcript) = observation else {
            return XCTFail("Expected completed transcript")
        }
        XCTAssertEqual(transcript.episodeID, job().episodeID)
        XCTAssertEqual(transcript.segments.map(\.text), ["Bounded result"])
        XCTAssertEqual(ChapterProviderStubProtocol.requestCount, 1)
        session.invalidateAndCancel()
    }

    func testObserveRejectsOversizedResponseAndMissingCredential() async {
        ChapterProviderStubProtocol.responseBody = Data("{}".utf8)
        let session = makeSession()
        let client = AssemblyAITranscriptClient(
            baseURL: URL(string: "https://assembly.example.test")!,
            session: session,
            credential: { "authorized-key" }
        )
        do {
            _ = try await client.observe(job(), maximumResponseBytes: 1)
            XCTFail("Expected bounded response failure")
        } catch {
            XCTAssertEqual(error as? CoreTranscriptTransportError, .responseTooLarge)
        }

        let missing = AssemblyAITranscriptClient(
            baseURL: URL(string: "https://assembly.example.test")!,
            session: session,
            credential: { nil }
        )
        do {
            _ = try await missing.observe(job(), maximumResponseBytes: 4_096)
            XCTFail("Expected missing credential failure")
        } catch AssemblyAITranscriptClient.TranscribeError.missingAPIKey {
            // Expected raw credential absence.
        } catch {
            XCTFail("Expected missing credential, got \(type(of: error))")
        }
        XCTAssertEqual(ChapterProviderStubProtocol.requestCount, 1)
        session.invalidateAndCancel()
    }

    private func job() -> AssemblyAIJob {
        AssemblyAIJob(
            transcriptID: "operation-1",
            episodeID: UUID(uuidString: "11111111-2222-3333-4444-555555555555")!,
            createdAt: Date(timeIntervalSince1970: 1_700_000_000),
            languageHint: nil,
            speechModels: ["universal-3-pro"]
        )
    }

    private func makeSession() -> URLSession {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ChapterProviderStubProtocol.self]
        return URLSession(configuration: configuration)
    }
}
