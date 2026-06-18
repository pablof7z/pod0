import Foundation
@testable import Podcastr

// MARK: - AppStateTestSupport
//
// Test-only helpers for spinning up an `AppStateStore` whose persistence is
// fully isolated from the production App Group container.
//
// The bug this guards against: tests used to construct `AppStateStore()` with
// no arguments, which fell through to the shared App Group `UserDefaults`
// (`group.com.podcastr.app`). Anything a test wrote — `Test Show`, `Episode
// e1`, etc. — survived the test run and showed up the next time the actual
// app launched on the simulator.
//
// `makeIsolatedStore()` builds a `Persistence` bound to a unique temp state
// URL plus its SQLite episode sidecar, so:
//
//   - Each test instance starts from a clean slate (no cross-test bleed).
//   - Nothing the tests write touches the real App Group container.
//   - The temp file is removed via `FileManager.removeItem` in `tearDown`.
//
// The storage primitive intentionally matches production (`Data.write` for
// metadata plus SQLite for episodes inside an App-Group-style container).
enum AppStateTestSupport {

    /// Builds an `AppStateStore` backed by a unique temp file so the test
    /// never touches the production App Group storage.
    ///
    /// - Parameters:
    ///   - fileURL: Backing file. Defaults to a unique temp path (clean slate);
    ///     pass an explicit URL when a test needs to share storage between
    ///     instances (e.g. round-tripping persistence across two
    ///     `AppStateStore` lifetimes).
    ///   - reset: When `true` (the default for a fresh unique URL), removes
    ///     any pre-existing file at `fileURL` before constructing the
    ///     store. Set to `false` when reopening over an existing file —
    ///     otherwise the test will load a fresh `AppState` instead of the
    ///     state the previous instance just wrote.
    @MainActor
    static func makeIsolatedStore(
        fileURL: URL = AppStateTestSupport.uniqueTempFileURL(),
        reset: Bool = true
    ) -> (store: AppStateStore, fileURL: URL) {
        if reset {
            // Belt-and-suspenders: clear anything a previous (crashed) test
            // run may have left at this exact path.
            try? FileManager.default.removeItem(at: fileURL)
        }
        let persistence = Persistence(fileURL: fileURL)
        let store = AppStateStore(
            persistence: persistence
        )
        return (store, fileURL)
    }

    /// Removes a temp file created by `makeIsolatedStore`. Safe to call
    /// from `tearDown`; idempotent if the file is already gone.
    static func disposeIsolatedStore(at fileURL: URL) {
        Persistence(fileURL: fileURL).reset()
    }

    /// Wipes the production App Group state file. Use only as a one-shot
    /// repair from a development build that previously corrupted the
    /// shared container (e.g. the test-leak bug being fixed in this branch).
    static func resetPersistedState() {
        Persistence.shared.reset()
    }

    /// Generates a unique JSON file path inside `NSTemporaryDirectory()`.
    /// Tests should let `makeIsolatedStore()` default to this so each test
    /// gets its own path; pass an explicit URL only when a test needs to
    /// share storage between two `AppStateStore` instances (e.g. the
    /// across-instance round-trip regression test).
    static func uniqueTempFileURL() -> URL {
        URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
            .appendingPathComponent("podcastr.tests", isDirectory: true)
            .appendingPathComponent("\(UUID().uuidString).json", isDirectory: false)
    }
}

// MARK: - AppStateStore test-only seeding helpers
//
// Rust owns all durable library writes (subscribe, upsert podcast, etc.)
// and the kernel is unavailable in unit tests. These extensions expose
// direct-mutation helpers that live ONLY in the AppTests target so tests
// can seed fixtures without a live kernel.
//
// Production code MUST NOT call these methods; they exist solely to
// replace the pre-autosnip `upsertPodcast` / `addSubscription` methods
// that were removed when library ownership moved to Rust.

extension AppStateStore {

    /// Test-only: insert or replace a podcast row in `state.podcasts`,
    /// bypassing the Rust kernel. Returns the podcast for chaining.
    @discardableResult
    func upsertPodcast(_ podcast: Podcast) -> Podcast {
        if let idx = state.podcasts.firstIndex(where: { $0.id == podcast.id }) {
            state.podcasts[idx] = podcast
        } else {
            state.podcasts.append(podcast)
        }
        return podcast
    }

    /// Test-only: add a `PodcastSubscription` row for `podcastID` in
    /// `state.subscriptions`, bypassing the Rust kernel.
    /// Returns `true` when newly added, `false` when already present
    /// (mirrors the pre-migration `addSubscription` return value).
    @discardableResult
    func addSubscription(podcastID: UUID) -> Bool {
        guard !state.subscriptions.contains(where: { $0.podcastID == podcastID }) else {
            return false
        }
        state.subscriptions.append(PodcastSubscription(podcastID: podcastID))
        return true
    }
}
