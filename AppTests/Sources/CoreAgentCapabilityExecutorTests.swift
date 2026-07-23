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

        let outcome = await executor.execute(.noArguments(tool: .pausePlayback))

        XCTAssertEqual(outcome, .succeeded(boundedResult: #"{"paused":true}"#))
        XCTAssertEqual(engine.state, .paused)
    }

    func testRateExecutesExactNativePrimitive() async {
        let engine = AudioEngine()
        engine.load(makeEpisode())
        let executor = LiveCoreAgentCapabilityExecutor(engine: engine)

        let outcome = await executor.execute(.setPlaybackRate(permille: 1_250))

        XCTAssertEqual(
            outcome,
            .succeeded(boundedResult: #"{"rate_permille":1250}"#)
        )
        XCTAssertEqual(engine.rate, 1.25)
    }

    func testCapabilityFailsClosedWithoutLoadedMedia() async {
        let executor = LiveCoreAgentCapabilityExecutor(engine: AudioEngine())

        let outcome = await executor.execute(.noArguments(tool: .pausePlayback))

        XCTAssertEqual(outcome, .failed(safeDetail: "Playback media is unavailable"))
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
