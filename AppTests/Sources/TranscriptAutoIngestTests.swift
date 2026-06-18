import XCTest
@testable import Podcastr

/// Coverage for `TranscriptIngestService.evaluateAutoIngest(newEpisodeIDs:)`
/// — the feed-refresh-triggered batch path that wires
/// `Settings.autoIngestPublisherTranscripts` to actual ingestion — and the
/// load-bearing Settings defaults.
///
/// The candidate-selection branching (`autoIngestCandidates`) moved to the
/// Rust kernel as part of the Rust-ownership migration and is covered by
/// `cargo test -p nmp-app-podcast` (`ffi/transcript_plan.rs`). The Swift side
/// now only owns the empty-input fast paths and the settings defaults asserted
/// here.
@MainActor
final class TranscriptAutoIngestTests: XCTestCase {

    // MARK: - Empty-input fast paths

    func testNoOpForEmptyIDList() {
        // `evaluateAutoIngest` should bail before any state lookup when
        // there are no IDs to consider.
        let service = TranscriptIngestService()
        // No throw, no crash, no async side effects: the call is sync.
        service.evaluateAutoIngest(newEpisodeIDs: [])
    }

    func testNoOpWhenStoreUnattached() {
        // No store is attached in test contexts. The helper logs a warning
        // and returns rather than crashing.
        let service = TranscriptIngestService()
        service.evaluateAutoIngest(newEpisodeIDs: [UUID(), UUID()])
    }

    // MARK: - Settings default

    func testAutoIngestDefaultsOn() {
        // The agent layer (RAG, wiki) only works once transcripts exist.
        // Defaulting off would leave most users with no transcripts despite
        // the feeds shipping them — the very bug this helper exists to fix.
        let s = Settings()
        XCTAssertTrue(s.autoIngestPublisherTranscripts)
        // Scribe-fallback default is also load-bearing for the cross-episode
        // RAG story — most indie podcasts don't ship publisher transcripts.
        XCTAssertTrue(s.autoFallbackToScribe)
        // The STT provider defaults to Apple on-device so keyless installs get
        // transcription out of the box.
        XCTAssertEqual(s.sttProvider, .appleNative)
    }
}
