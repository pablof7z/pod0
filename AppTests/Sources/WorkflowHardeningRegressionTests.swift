import Foundation
import XCTest
@testable import Podcastr

final class WorkflowRepairRegressionTests: XCTestCase {
    func testLegacyTranscriptArtifactCannotSatisfyRustTranscriptPlanning() {
        let episode = Episode(
            podcastID: UUID(),
            guid: "legacy-transcript-artifact",
            title: "Legacy transcript artifact",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/legacy-transcript.mp3")!
        )
        let inputVersion = DesiredStatePlanner.audioVersion(episode)
        let legacyArtifact = ArtifactRecord(
            kind: .transcript,
            subjectID: episode.id,
            inputVersion: inputVersion,
            outputVersion: "legacy-output",
            contentHash: "legacy-hash",
            location: "/tmp/legacy-transcript.json",
            origin: "legacy",
            schemaVersion: 1,
            integrity: .available,
            verifiedAt: Date()
        )

        let jobs = DesiredStatePlanner().plan(.init(
            episodes: [episode],
            settings: Settings(),
            artifacts: [legacyArtifact],
            transcripts: [],
            transcriptDesiredEpisodeIDs: [episode.id],
            scheduledTasks: [],
            now: Date()
        ))

        XCTAssertEqual(jobs.map(\.kind), [.transcriptIngest])
    }
}
