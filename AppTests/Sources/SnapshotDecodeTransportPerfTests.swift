import XCTest
@testable import Podcastr

/// Measures the Swift side of the FFI snapshot transport: decoding the full
/// library `PodcastUpdate` JSON the kernel hands across `nmp_app_podcast_snapshot`
/// on every content tick. Uses the EXACT decoder configuration
/// `PodcastHandle.podcastSnapshot()` uses (`.convertFromSnakeCase`), so the
/// number is the real on-device/on-sim cost the shell pays per content change.
///
/// Run on the simulator:
///   xcodebuild test -scheme Podcastr -destination 'platform=iOS Simulator,name=...' \
///     -only-testing:PodcastrTests/SnapshotDecodeTransportPerfTests
@MainActor
final class SnapshotDecodeTransportPerfTests: XCTestCase {

    /// Realistic show-notes prose (~640 bytes), mirroring the Rust harness.
    private static let description = """
    In this episode we sit down with our guest to unpack the week's biggest \
    stories, dig into the research behind the headlines, and answer listener \
    questions from the mailbag. We cover the new findings, what they mean for \
    you, and where the experts disagree. Plus: a lightning round, a few tangents, \
    and our picks of the week. Show notes, links, and the full transcript are \
    available on our website. This episode is brought to you by our sponsors — \
    visit the links in the description to support the show.
    """

    private static let summary = """
    The hosts interview a guest about recent developments, covering the key \
    findings and their practical implications. They debate points of expert \
    disagreement and close with a lightning round and weekly picks.
    """

    /// Build a snake_case library JSON payload byte-shaped like the Rust
    /// `build_podcast_update` output for `numShows × epsPerShow` episodes.
    private func makePayload(numShows: Int, epsPerShow: Int) -> String {
        let desc = Self.description.replacingOccurrences(of: "\"", with: "\\\"")
        let summ = Self.summary.replacingOccurrences(of: "\"", with: "\\\"")
        var shows: [String] = []
        shows.reserveCapacity(numShows)
        for s in 0..<numShows {
            let showID = String(format: "f0e1d2c3-b4a5-4968-8778-%012x", s)
            var eps: [String] = []
            eps.reserveCapacity(epsPerShow)
            for i in 0..<epsPerShow {
                let n = s * epsPerShow + i
                let epID = String(format: "a1b2c3d4-e5f6-4a7b-8c9d-%012x", n)
                let summaryField = (n % 3 == 0) ? ",\"summary\":\"\(summ)\"" : ""
                eps.append("""
                {"id":"\(epID)","title":"Episode \(n): A Reasonably Long Human-Readable Episode Title",\
                "podcast_id":"\(showID)","podcast_title":"The Reasonably Named Podcast Number \(s)",\
                "duration_secs":\(3600 + n),"artwork_url":"https://cdn.example.com/artwork/\(showID)/episode-\(n)-1400x1400.jpg",\
                "published_at":\(1_700_000_000 + n * 86_400),\
                "enclosure_url":"https://traffic.example.com/podcast/\(showID)/episode-\(n).mp3?token=abc123",\
                "description":"\(desc)","played":\(n % 4 == 0 ? "true" : "false")\(summaryField)}
                """)
            }
            shows.append("""
            {"id":"\(showID)","title":"The Reasonably Named Podcast Number \(s)",\
            "episode_count":\(epsPerShow),"unplayed_count":\(epsPerShow * 3 / 4),\
            "artwork_url":"https://cdn.example.com/shows/\(s)/cover-3000x3000.jpg",\
            "feed_url":"https://feeds.example.com/show-\(s)/rss.xml",\
            "author":"A Reasonably Named Production Company, LLC","description":"\(desc)",\
            "episodes":[\(eps.joined(separator: ","))]}
            """)
        }
        return "{\"running\":true,\"rev\":1,\"schema_version\":1,\"library\":[\(shows.joined(separator: ","))]}"
    }

    /// The exact decoder `podcastSnapshot()` builds.
    private func snapshotDecoder() -> JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }

    func testFullLibraryDecodeCost() throws {
        for (shows, per) in [(20, 50), (20, 180), (20, 500)] {
            let json = makePayload(numShows: shows, epsPerShow: per)
            let data = Data(json.utf8)
            let decoder = snapshotDecoder()

            // Correctness: it decodes to the real type with the right shape.
            let decoded = try decoder.decode(PodcastUpdate.self, from: data)
            let total = decoded.library.reduce(0) { $0 + $1.episodes.count }
            XCTAssertEqual(total, shows * per)

            // Median of several runs (Swift JSONDecoder, the real path).
            var samples: [Double] = []
            for _ in 0..<7 {
                let t = CFAbsoluteTimeGetCurrent()
                _ = try decoder.decode(PodcastUpdate.self, from: data)
                samples.append((CFAbsoluteTimeGetCurrent() - t) * 1000.0)
            }
            samples.sort()
            let median = samples[samples.count / 2]
            print(String(
                format: "DECODE  shows=%d eps=%-6d payload=%7.1f KB  JSONDecoder median=%6.2f ms",
                shows, shows * per, Double(data.count) / 1024.0, median))
        }
    }

    /// XCTest-metric variant for the 3,600-episode worst case so the number
    /// shows up in the test report's performance metrics, not just stdout.
    func testFullLibraryDecodeMetric() throws {
        let json = makePayload(numShows: 20, epsPerShow: 180)
        let data = Data(json.utf8)
        let decoder = snapshotDecoder()
        measure {
            _ = try? decoder.decode(PodcastUpdate.self, from: data)
        }
    }
}
