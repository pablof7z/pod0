import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class LiveCoreTranscriptProviderRoutingTests: XCTestCase {
    private let episodeID = UUID(uuidString: "11111111-2222-3333-4444-555555555555")!

    func testAssemblySubmissionReturnsIdentityWithoutReadingStatus() async throws {
        let clients = makeClients()
        let transport = LiveCoreTranscriptTransport(providers: clients.values)

        let observation = try await transport.execute(submit(provider: .assemblyAi))

        guard case .providerAccepted(let externalID, let status) = observation else {
            return XCTFail("Expected provider acceptance")
        }
        XCTAssertEqual(externalID, "assembly-operation")
        XCTAssertEqual(status, "queued")
        let counts = await clients.assembly.counts()
        XCTAssertEqual(counts, .init(submits: 1, observations: 0))
    }

    func testAssemblyRecoveryPerformsExactlyOneStatusRead() async throws {
        let clients = makeClients(assemblyStatus: .pending(status: "processing"))
        let transport = LiveCoreTranscriptTransport(providers: clients.values)

        let observation = try await transport.execute(recover(
            provider: .assemblyAi,
            externalID: "assembly-operation"
        ))

        guard case .providerPending(let status, nil) = observation else {
            return XCTFail("Expected pending provider observation")
        }
        XCTAssertEqual(status, "processing")
        let counts = await clients.assembly.counts()
        XCTAssertEqual(counts, .init(submits: 0, observations: 1))
    }

    func testElevenLabsRecoveryReadsExistingIdentityWithoutSubmitting() async throws {
        let clients = makeClients()
        let transport = LiveCoreTranscriptTransport(providers: clients.values)

        let observation = try await transport.execute(recover(
            provider: .elevenLabsScribe,
            externalID: "scribe-existing"
        ))

        guard case .completed(let transcript, let externalID, _) = observation else {
            return XCTFail("Expected recovered transcript")
        }
        XCTAssertEqual(transcript.source, .scribeV1)
        XCTAssertEqual(externalID, "scribe-existing")
        let counts = await clients.scribe.counts()
        let readID = await clients.scribe.lastReadID()
        XCTAssertEqual(counts, .init(submits: 0, reads: 1))
        XCTAssertEqual(readID, "scribe-existing")
    }

    func testSynchronousAndLocalProvidersReturnOneBoundedCompletion() async throws {
        let clients = makeClients()
        let transport = LiveCoreTranscriptTransport(providers: clients.values)

        let scribe = try await transport.execute(submit(provider: .elevenLabsScribe))
        let whisper = try await transport.execute(submit(provider: .openRouterWhisper))
        let apple = try await transport.execute(.transcribeLocal(
            context: context,
            attemptId: TranscriptAttemptId(high: 7, low: 8),
            audioUrl: "file:///tmp/episode.m4a",
            locale: "en-US"
        ))

        XCTAssertEqual(source(scribe), .scribeV1)
        XCTAssertEqual(source(whisper), .whisper)
        XCTAssertEqual(source(apple), .onDevice)
        let scribeCounts = await clients.scribe.counts()
        let openRouterCalls = await clients.openRouter.callCount()
        let appleCalls = await clients.apple.callCount()
        XCTAssertEqual(scribeCounts, .init(submits: 1, reads: 1))
        XCTAssertEqual(openRouterCalls, 1)
        XCTAssertEqual(appleCalls, 1)
    }

    func testProviderCompletionIsRejectedWhenItExceedsRustBound() async {
        let clients = makeClients(text: String(repeating: "x", count: 2_048))
        let transport = LiveCoreTranscriptTransport(providers: clients.values)

        do {
            _ = try await transport.execute(submit(
                provider: .openRouterWhisper,
                maximumResponseBytes: 32
            ))
            XCTFail("Expected response bound failure")
        } catch {
            XCTAssertEqual(error as? CoreTranscriptTransportError, .responseTooLarge)
        }
        let calls = await clients.openRouter.callCount()
        XCTAssertEqual(calls, 1)
    }

    private func submit(
        provider: TranscriptProvider,
        maximumResponseBytes: UInt64 = 1_000_000
    ) -> TranscriptCapabilityRequest {
        .submitProvider(
            context: context,
            attemptId: TranscriptAttemptId(high: 1, low: 2),
            submissionFenceId: TranscriptSubmissionFenceId(high: 3, low: 4),
            provider: provider,
            model: "test-model",
            audioUrl: "https://example.test/audio.mp3",
            maximumResponseBytes: maximumResponseBytes
        )
    }

    private func recover(
        provider: TranscriptProvider,
        externalID: String
    ) -> TranscriptCapabilityRequest {
        .recoverProvider(
            context: context,
            attemptId: TranscriptAttemptId(high: 1, low: 2),
            submissionFenceId: TranscriptSubmissionFenceId(high: 3, low: 4),
            provider: provider,
            model: "test-model",
            externalOperationId: externalID,
            providerStatus: "processing",
            maximumResponseBytes: 1_000_000
        )
    }

    private var context: TranscriptCapabilityContext {
        TranscriptCapabilityContext(
            episodeId: EpisodeId(uuid: episodeID),
            podcastId: PodcastId(high: 5, low: 6),
            sourceRevision: "audio-v1"
        )
    }

    private func source(
        _ observation: CoreTranscriptTransportObservation
    ) -> Podcastr.TranscriptSource? {
        guard case .completed(let transcript, _, _) = observation else { return nil }
        return transcript.source
    }

    private func makeClients(
        assemblyStatus: AssemblyAIStatusObservation? = nil,
        text: String = "Bounded transcript"
    ) -> ProviderFixtures {
        let assembly = StubAssemblyAIClient(
            status: assemblyStatus ?? .completed(transcript(source: .assemblyAI, text: text))
        )
        let scribe = StubElevenLabsClient(
            transcript: transcript(source: .scribeV1, text: text)
        )
        let openRouter = StubOpenRouterClient(
            transcript: transcript(source: .whisper, text: text)
        )
        let apple = StubAppleSpeechClient(
            transcript: transcript(source: .onDevice, text: text)
        )
        return ProviderFixtures(
            values: CoreTranscriptProviderClients(
                assemblyAI: assembly,
                elevenLabs: { _ in scribe },
                openRouter: { _ in openRouter },
                appleSpeech: apple
            ),
            assembly: assembly,
            scribe: scribe,
            openRouter: openRouter,
            apple: apple
        )
    }

    private func transcript(source: Podcastr.TranscriptSource, text: String) -> Transcript {
        Transcript(
            episodeID: episodeID,
            language: "en",
            source: source,
            segments: [Segment(start: 0, end: 1, text: text)],
            generatedAt: Date(timeIntervalSince1970: 1_700_000_000)
        )
    }
}

private struct ProviderFixtures {
    let values: CoreTranscriptProviderClients
    let assembly: StubAssemblyAIClient
    let scribe: StubElevenLabsClient
    let openRouter: StubOpenRouterClient
    let apple: StubAppleSpeechClient
}

private actor StubAssemblyAIClient: CoreAssemblyAITranscribing {
    struct Counts: Equatable { let submits: Int; let observations: Int }
    let status: AssemblyAIStatusObservation
    private var submitCount = 0
    private var observationCount = 0

    init(status: AssemblyAIStatusObservation) {
        self.status = status
    }

    func submit(
        audioURL _: URL,
        episodeID: UUID,
        speechModels _: [String],
        speakerLabels _: Bool,
        languageDetection _: Bool,
        languageHint _: String?
    ) -> AssemblyAIJob {
        submitCount += 1
        return AssemblyAIJob(
            transcriptID: "assembly-operation",
            episodeID: episodeID,
            createdAt: Date(timeIntervalSince1970: 1_700_000_000),
            languageHint: nil,
            speechModels: ["test-model"]
        )
    }

    func observe(
        _: AssemblyAIJob,
        maximumResponseBytes _: UInt64
    ) -> AssemblyAIStatusObservation {
        observationCount += 1
        return status
    }

    func counts() -> Counts { Counts(submits: submitCount, observations: observationCount) }
}

private actor StubElevenLabsClient: CoreElevenLabsTranscribing {
    struct Counts: Equatable { let submits: Int; let reads: Int }
    let transcript: Transcript
    private var submitCount = 0
    private var readCount = 0
    private var readID: String?

    init(transcript: Transcript) {
        self.transcript = transcript
    }

    func submit(audioURL _: URL, episodeID: UUID, languageHint _: String?) -> ScribeJob {
        submitCount += 1
        return ScribeJob(
            requestID: "scribe-created",
            episodeID: episodeID,
            createdAt: Date(timeIntervalSince1970: 1_700_000_000),
            languageHint: nil,
            inlineResult: nil
        )
    }

    func result(for job: ScribeJob) -> Transcript {
        readCount += 1
        readID = job.requestID
        return transcript
    }

    func counts() -> Counts { Counts(submits: submitCount, reads: readCount) }
    func lastReadID() -> String? { readID }
}

private actor StubOpenRouterClient: CoreOpenRouterTranscribing {
    let transcript: Transcript
    private var calls = 0

    init(transcript: Transcript) {
        self.transcript = transcript
    }

    func transcribe(audioURL _: URL, episodeID _: UUID, languageHint _: String?) -> Transcript {
        calls += 1
        return transcript
    }

    func callCount() -> Int { calls }
}

private actor StubAppleSpeechClient: CoreAppleSpeechTranscribing {
    let transcript: Transcript
    private var calls = 0

    init(transcript: Transcript) {
        self.transcript = transcript
    }

    func transcribe(audioFileURL _: URL, episodeID _: UUID, languageHint _: String?) -> Transcript {
        calls += 1
        return transcript
    }

    func callCount() -> Int { calls }
}
