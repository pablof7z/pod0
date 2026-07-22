import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class LiveCoreTranscriptTransportTests: XCTestCase {
    override func setUp() {
        super.setUp()
        ChapterProviderStubProtocol.reset()
    }

    override func tearDown() {
        ChapterProviderStubProtocol.reset()
        super.tearDown()
    }

    func testPublisherFetchIsBoundedAndReturnsOnlyParsedObservation() async throws {
        ChapterProviderStubProtocol.responseHeaders = ["Content-Type": "text/vtt"]
        ChapterProviderStubProtocol.responseBody = Data("""
        WEBVTT

        00:00:00.000 --> 00:00:01.000
        Bounded publisher transcript
        """.utf8)
        let session = makeSession()
        let transport = LiveCoreTranscriptTransport(
            session: session,
            assemblyAI: AssemblyAITranscriptClient(
                baseURL: URL(string: "https://assembly.example.test")!,
                session: session,
                credential: { nil }
            )
        )

        let observation = try await transport.execute(.fetchPublisher(
            context: context,
            sourceUrl: "https://publisher.example.test/transcript.vtt",
            mimeHint: "text/vtt",
            maximumResponseBytes: 4_096
        ))

        guard case .completed(let transcript, nil, let status) = observation else {
            return XCTFail("Expected parsed publisher observation")
        }
        XCTAssertEqual(status, "completed")
        XCTAssertEqual(transcript.source, .publisher)
        XCTAssertEqual(transcript.segments.map(\.text), ["Bounded publisher transcript"])
        XCTAssertEqual(ChapterProviderStubProtocol.requestCount, 1)
        session.invalidateAndCancel()
    }

    func testPublisherFetchRejectsResponseBeyondRustBound() async throws {
        ChapterProviderStubProtocol.responseHeaders = ["Content-Type": "text/vtt"]
        ChapterProviderStubProtocol.responseBody = Data(repeating: 65, count: 128)
        let session = makeSession()
        let transport = LiveCoreTranscriptTransport(
            session: session,
            assemblyAI: AssemblyAITranscriptClient(
                baseURL: URL(string: "https://assembly.example.test")!,
                session: session,
                credential: { nil }
            )
        )

        do {
            _ = try await transport.execute(.fetchPublisher(
                context: context,
                sourceUrl: "https://publisher.example.test/transcript.vtt",
                mimeHint: "text/vtt",
                maximumResponseBytes: 32
            ))
            XCTFail("Expected response bound failure")
        } catch {
            XCTAssertEqual(error as? CoreTranscriptTransportError, .responseTooLarge)
        }
        XCTAssertEqual(ChapterProviderStubProtocol.requestCount, 1)
        session.invalidateAndCancel()
    }

    private var context: TranscriptCapabilityContext {
        TranscriptCapabilityContext(
            episodeId: EpisodeId(high: 1, low: 2),
            podcastId: PodcastId(high: 3, low: 4),
            sourceRevision: "publisher-v1"
        )
    }

    private func makeSession() -> URLSession {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ChapterProviderStubProtocol.self]
        return URLSession(configuration: configuration)
    }
}
