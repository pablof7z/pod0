import Darwin
import Foundation
import Pod0Core
import XCTest
@testable import Podcastr

final class CoreRecallPerformanceTests: XCTestCase {
    func testRepresentativeHybridCandidateRetrievalMeetsLocalBudget() async throws {
        let spanCount = environmentInteger("POD0_RECALL_BENCHMARK_SPANS", default: 5_000)
        let dimensions = environmentInteger("POD0_RECALL_BENCHMARK_DIMENSIONS", default: 1_024)
        let sampleCount = environmentInteger("POD0_RECALL_BENCHMARK_SAMPLES", default: 20)
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("pod0-native-recall-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let fileURL = directory.appendingPathComponent("recall.sqlite")
        let embedder = PerformanceRecallEmbedder(dimensions: dimensions)
        let index = try VectorIndex(
            embedder: embedder,
            fileURL: fileURL,
            dimensions: dimensions
        )

        let rebuildStarted = ContinuousClock.now
        var remaining = spanCount
        var episodeOffset = 0
        while remaining > 0 {
            let count = min(100, remaining)
            let rebuilt = try await index.rebuildCoreRecallIndex(
                spans: makeSpans(episodeOffset: episodeOffset, count: count)
            )
            XCTAssertEqual(rebuilt, UInt32(count))
            remaining -= count
            episodeOffset += 1
        }
        let rebuildDuration = ContinuousClock.now - rebuildStarted

        let coldIndex = try VectorIndex(
            embedder: embedder,
            fileURL: fileURL,
            dimensions: dimensions
        )
        let query = [Float](repeating: 0, count: dimensions).replacingFirst(with: 1)
        let coldStarted = ContinuousClock.now
        var candidates = try await retrieveCandidates(index: coldIndex, query: query)
        let coldDuration = ContinuousClock.now - coldStarted
        var samples: [Double] = []
        for _ in 0..<sampleCount {
            let started = ContinuousClock.now
            candidates = try await retrieveCandidates(index: coldIndex, query: query)
            samples.append(milliseconds(ContinuousClock.now - started))
        }
        samples.sort()

        let cancellationStarted = ContinuousClock.now
        let cancelledTask = Task {
            try await retrieveCandidates(index: coldIndex, query: query)
        }
        cancelledTask.cancel()
        let cancellationHonored: Bool
        do {
            _ = try await cancelledTask.value
            cancellationHonored = false
        } catch is CancellationError {
            cancellationHonored = true
        }
        let cancellationDuration = ContinuousClock.now - cancellationStarted
        let result = NativeRecallBenchmark(
            backend: "swift-sqlite-vec",
            sqliteVecVersion: "v0.1.6",
            spans: spanCount,
            dimensions: dimensions,
            samples: sampleCount,
            rebuildMilliseconds: milliseconds(rebuildDuration),
            coldQueryMilliseconds: milliseconds(coldDuration),
            warmQueryP50Milliseconds: percentile(samples, 50),
            warmQueryP95Milliseconds: percentile(samples, 95),
            cancellationResponseMilliseconds: milliseconds(cancellationDuration),
            cancellationHonored: cancellationHonored,
            candidateCount: candidates.count,
            maximumCandidateCount: 40,
            indexBytes: try directorySize(directory),
            executableBytes: executableSize(),
            peakResidentBytes: peakResidentBytes(),
            privateTextLeavesProcess: false
        )
        let encoded = try JSONEncoder().encode(result)
        print("POD0_RECALL_BENCHMARK \(String(decoding: encoded, as: UTF8.self))")

        XCTAssertFalse(candidates.isEmpty)
        XCTAssertLessThanOrEqual(candidates.count, 40)
        if spanCount == 5_000 && dimensions == 1_024 {
            XCTAssertLessThan(result.warmQueryP95Milliseconds, 250)
            XCTAssertLessThan(result.rebuildMilliseconds, 30_000)
        }
    }

    private func makeSpans(episodeOffset: Int, count: Int) -> [CoreRecallIndexSpan] {
        let episodeNumber = UInt64(episodeOffset + 1)
        return (0..<count).map { spanOffset in
            CoreRecallIndexSpan(
                spanID: EvidenceSpanId(high: episodeNumber, low: UInt64(spanOffset + 1)),
                generationID: EvidenceGenerationId(high: 43, low: episodeNumber),
                episodeID: EpisodeId(high: 42, low: episodeNumber),
                podcastID: PodcastId(high: 41, low: 1),
                text: spanOffset == 50
                    ? "needle evidence for durable podcast recall"
                    : "background discussion \(spanOffset) in episode \(episodeOffset)"
            )
        }
    }

}

private func retrieveCandidates(
    index: VectorIndex,
    query: [Float]
) async throws -> [RecallCandidateObservation] {
    try await index.retrieveCoreRecallCandidates(
        queryVector: query,
        lexicalQuery: "needle evidence",
        scope: .library,
        maximumVectorCandidates: 20,
        maximumLexicalCandidates: 20,
        maximumTotalCandidates: 40
    )
}

private struct PerformanceRecallEmbedder: EmbeddingsClient {
    let dimensions: Int

    func embed(_ texts: [String]) async throws -> [[Float]] {
        texts.enumerated().map { offset, text in
            var values = [Float](repeating: 0, count: dimensions)
            values[text.contains("needle") ? 0 : (offset + 1) % dimensions] = 1
            return values
        }
    }
}

private struct NativeRecallBenchmark: Encodable {
    let backend: String
    let sqliteVecVersion: String
    let spans: Int
    let dimensions: Int
    let samples: Int
    let rebuildMilliseconds: Double
    let coldQueryMilliseconds: Double
    let warmQueryP50Milliseconds: Double
    let warmQueryP95Milliseconds: Double
    let cancellationResponseMilliseconds: Double
    let cancellationHonored: Bool
    let candidateCount: Int
    let maximumCandidateCount: Int
    let indexBytes: UInt64
    let executableBytes: UInt64
    let peakResidentBytes: UInt64
    let privateTextLeavesProcess: Bool
}

private func environmentInteger(_ key: String, default value: Int) -> Int {
    ProcessInfo.processInfo.environment[key].flatMap(Int.init).map { max(1, $0) } ?? value
}

private func milliseconds(_ duration: Duration) -> Double {
    let parts = duration.components
    return Double(parts.seconds) * 1_000
        + Double(parts.attoseconds) / 1_000_000_000_000_000
}

private func percentile(_ samples: [Double], _ percentile: Int) -> Double {
    let index = max(0, (samples.count - 1) * percentile / 100)
    return samples[index]
}

private func directorySize(_ directory: URL) throws -> UInt64 {
    try FileManager.default.contentsOfDirectory(
        at: directory,
        includingPropertiesForKeys: [.fileSizeKey]
    ).reduce(0) { total, url in
        total + UInt64(try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0)
    }
}

private func executableSize() -> UInt64 {
    guard let url = Bundle.main.executableURL,
          let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize else { return 0 }
    return UInt64(size)
}

private func peakResidentBytes() -> UInt64 {
    var usage = rusage()
    guard getrusage(RUSAGE_SELF, &usage) == 0 else { return 0 }
    return UInt64(usage.ru_maxrss)
}

private extension Array where Element == Float {
    func replacingFirst(with value: Float) -> Self {
        var copy = self
        copy[0] = value
        return copy
    }
}
