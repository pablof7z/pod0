import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

@MainActor
final class CoreAgentCapabilityExecutorTests: XCTestCase {
    func testPauseExecutesExactNativePrimitive() async {
        let engine = AudioEngine()
        engine.load(makeEpisode())
        let executor = LiveCoreAgentCapabilityExecutor(engine: engine)

        let outcome = await executor.execute(request(.noArguments(tool: .pausePlayback)))

        XCTAssertEqual(outcome, .succeeded(boundedResult: #"{"paused":true}"#))
        XCTAssertEqual(engine.state, .paused)
    }

    func testRateExecutesExactNativePrimitive() async {
        let engine = AudioEngine()
        engine.load(makeEpisode())
        let executor = LiveCoreAgentCapabilityExecutor(engine: engine)

        let outcome = await executor.execute(request(.setPlaybackRate(permille: 1_250)))

        XCTAssertEqual(
            outcome,
            .succeeded(boundedResult: #"{"rate_permille":1250}"#)
        )
        XCTAssertEqual(engine.rate, 1.25)
    }

    func testCapabilityFailsClosedWithoutLoadedMedia() async {
        let executor = LiveCoreAgentCapabilityExecutor(engine: AudioEngine())

        let outcome = await executor.execute(request(.noArguments(tool: .pausePlayback)))

        XCTAssertEqual(outcome, .failed(safeDetail: "Playback media is unavailable"))
    }

    func testGeneratedAudioIsStagedAtTheRustAssignedArtifactPath() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("core-agent-audio-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let tts = StubAgentTTS(chunks: [Data("audio-".utf8), Data("bytes".utf8)])
        let store = CoreAgentGeneratedAudioFileStore(directory: directory)
        let target = AgentGeneratedAudioTarget(
            artifactId: GeneratedArtifactId(high: 21, low: 22),
            maximumBytes: 1_024
        )
        let executor = LiveCoreAgentCapabilityExecutor(
            engine: AudioEngine(),
            tts: tts,
            generatedAudioStore: store
        )

        let outcome = await executor.execute(request(
            .generateTtsEpisode(
                podcastId: nil,
                title: "Briefing",
                script: "One useful idea.",
                voiceId: "calm"
            ),
            target: target
        ))

        guard case .generatedAudioStaged(let evidence) = outcome else {
            return XCTFail("Expected staged audio evidence")
        }
        XCTAssertEqual(evidence.artifactId, target.artifactId)
        XCTAssertEqual(evidence.byteCount, 11)
        XCTAssertEqual(evidence.mediaType, "audio/mpeg")
        XCTAssertTrue(try XCTUnwrap(URL(string: evidence.fileUrl)).isFileURL)
        XCTAssertEqual(tts.callCount, 1)
    }

    func testRecoveryAdoptsExistingAudioWithoutRepeatingTTS() async {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("core-agent-recovery-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let tts = StubAgentTTS(chunks: [Data("existing audio".utf8)])
        let store = CoreAgentGeneratedAudioFileStore(directory: directory)
        let target = AgentGeneratedAudioTarget(
            artifactId: GeneratedArtifactId(high: 31, low: 32),
            maximumBytes: 1_024
        )
        let action = AgentToolAction.generateTtsEpisode(
            podcastId: nil,
            title: "Recovered",
            script: "Do not synthesize twice.",
            voiceId: nil
        )
        let executor = LiveCoreAgentCapabilityExecutor(
            engine: AudioEngine(),
            tts: tts,
            generatedAudioStore: store
        )
        _ = await executor.execute(request(action, target: target))
        let callsAfterPerform = tts.callCount

        let recovered = await executor.execute(request(
            action,
            target: target,
            mode: .recoverExisting
        ))

        guard case .generatedAudioStaged = recovered else {
            return XCTFail("Expected existing audio evidence")
        }
        XCTAssertEqual(tts.callCount, callsAfterPerform)
    }

    func testMissingRecoveryArtifactStaysAmbiguousWithoutCallingTTS() async {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("core-agent-missing-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let tts = StubAgentTTS(chunks: [Data("must not run".utf8)])
        let executor = LiveCoreAgentCapabilityExecutor(
            engine: AudioEngine(),
            tts: tts,
            generatedAudioStore: CoreAgentGeneratedAudioFileStore(directory: directory)
        )
        let target = AgentGeneratedAudioTarget(
            artifactId: GeneratedArtifactId(high: 41, low: 42),
            maximumBytes: 1_024
        )

        let outcome = await executor.execute(request(
            .generateTtsEpisode(
                podcastId: nil,
                title: "Missing",
                script: "Do not retry.",
                voiceId: nil
            ),
            target: target,
            mode: .recoverExisting
        ))

        XCTAssertEqual(outcome, .outcomeAmbiguous)
        XCTAssertEqual(tts.callCount, 0)
    }

    private func request(
        _ action: AgentToolAction,
        target: AgentGeneratedAudioTarget? = nil,
        mode: AgentCapabilityExecutionMode = .perform
    ) -> AgentCapabilityRequest {
        AgentCapabilityRequest(
            turnId: AgentTurnId(high: 1, low: 2),
            proposalId: AgentProposalId(high: 3, low: 4),
            proposalDigest: ContentDigest(word0: 5, word1: 6, word2: 7, word3: 8),
            executionFenceId: AgentExecutionFenceId(high: 9, low: 10),
            executionMode: mode,
            generatedAudioTarget: target,
            action: action
        )
    }

    private func makeEpisode() -> Episode {
        let id = UUID()
        return Episode(
            id: id,
            podcastID: UUID(),
            guid: "agent-capability-\(id.uuidString)",
            title: "Agent Capability Episode",
            pubDate: Date(),
            duration: 600,
            enclosureURL: URL(string: "https://cdn.example.test/\(id.uuidString).mp3")!
        )
    }
}

private final class StubAgentTTS: TTSClientProtocol, @unchecked Sendable {
    private let lock = NSLock()
    private let chunks: [Data]
    private var calls = 0

    init(chunks: [Data]) {
        self.chunks = chunks
    }

    var isConfigured: Bool { true }

    var callCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return calls
    }

    func synthesizeStream(
        text _: String,
        voiceID _: String
    ) -> AsyncThrowingStream<Data, Error> {
        lock.lock()
        calls += 1
        lock.unlock()
        return AsyncThrowingStream { continuation in
            for chunk in chunks { continuation.yield(chunk) }
            continuation.finish()
        }
    }
}
