import Foundation
import XCTest
@testable import Podcastr

/// Admission policy for transient chapter projections.
///
/// The reported bug: an episode's chapters silently failed to load whenever
/// eight other scopes were already held. The player then rendered its
/// "no chapters" placeholder over an episode whose chapters existed, were
/// selected, and were flagged for the table of contents — and because that
/// episode also had an in-flight chapter workflow, the placeholder read
/// "Compiling chapters". Nothing retried, so it persisted for the session.
final class ChapterProjectionScopesTests: XCTestCase {
    private let capacity = 8

    /// The bug, stated directly: being at capacity must never mean an episode
    /// goes unloaded. Capacity is a budget to reallocate, not a reason to
    /// silently answer "no".
    func testAnEpisodeIsAdmittedEvenWhenCapacityIsAlreadyFull() {
        var scopes = ChapterProjectionScopes(capacity: capacity)
        let held = (0 ..< capacity).map { _ in UUID() }
        for id in held { XCTAssertEqual(scopes.retain(id), .load(evicting: nil)) }
        let playing = UUID()

        let admission = scopes.retain(playing)

        XCTAssertEqual(admission, .load(evicting: held[0]))
        XCTAssertTrue(scopes.isRetained(playing))
        XCTAssertEqual(scopes.count, capacity, "The budget itself must still hold")
    }

    /// Eviction takes the coldest scope, not an arbitrary one.
    func testEvictionTakesTheLeastRecentlyRetainedScope() {
        var scopes = ChapterProjectionScopes(capacity: 3)
        let (first, second, third) = (UUID(), UUID(), UUID())
        for id in [first, second, third] { _ = scopes.retain(id) }
        // Touching `first` again must make `second` the coldest.
        _ = scopes.retain(first)
        _ = scopes.release(first)

        let admission = scopes.retain(UUID())

        XCTAssertEqual(admission, .load(evicting: second))
        XCTAssertFalse(scopes.isRetained(second))
        XCTAssertTrue(scopes.isRetained(first))
    }

    /// Re-retaining an episode already held is free: no load, no eviction.
    func testRetainingAnAlreadyHeldEpisodeNeitherLoadsNorEvicts() {
        var scopes = ChapterProjectionScopes(capacity: 2)
        let id = UUID()
        XCTAssertEqual(scopes.retain(id), .load(evicting: nil))

        XCTAssertEqual(scopes.retain(id), .alreadyRetained)
        XCTAssertEqual(scopes.count, 1)
    }

    /// Releasing returns the budget so the next episode needs no eviction.
    func testReleasingFreesCapacityWithoutEvictingAnyone() {
        var scopes = ChapterProjectionScopes(capacity: 2)
        let (first, second) = (UUID(), UUID())
        _ = scopes.retain(first)
        _ = scopes.retain(second)

        XCTAssertTrue(scopes.release(first))

        XCTAssertEqual(scopes.retain(UUID()), .load(evicting: nil))
    }

    /// Several views may hold the same episode; only the last release drops it.
    func testAScopeSurvivesUntilEveryHolderReleasesIt() {
        var scopes = ChapterProjectionScopes(capacity: 4)
        let id = UUID()
        _ = scopes.retain(id)
        _ = scopes.retain(id)

        XCTAssertFalse(scopes.release(id), "One holder remains")
        XCTAssertTrue(scopes.isRetained(id))
        XCTAssertTrue(scopes.release(id), "Final holder released")
        XCTAssertFalse(scopes.isRetained(id))
    }

    /// Releasing something never retained must not corrupt the budget.
    func testReleasingAnUnknownEpisodeIsInert() {
        var scopes = ChapterProjectionScopes(capacity: 2)
        let id = UUID()
        _ = scopes.retain(id)

        XCTAssertFalse(scopes.release(UUID()))

        XCTAssertEqual(scopes.count, 1)
        XCTAssertTrue(scopes.isRetained(id))
    }

    /// An evicted scope is genuinely gone, so re-requesting it reloads rather
    /// than assuming a snapshot is still cached.
    func testAnEvictedEpisodeReloadsWhenRequestedAgain() {
        var scopes = ChapterProjectionScopes(capacity: 1)
        let (first, second) = (UUID(), UUID())
        _ = scopes.retain(first)
        XCTAssertEqual(scopes.retain(second), .load(evicting: first))

        XCTAssertEqual(scopes.retain(first), .load(evicting: second))
    }
}
