import XCTest
@testable import Podcastr

/// Projection parity guard for the #561 settings seam.
///
/// Verifies that each of the 12 kernel-owned settings fields that were
/// previously dispatched Swift→kernel but never projected back are now
/// correctly reflected in `AppState.settings` after a
/// `projectSnapshotDerivedState` call.
///
/// If a field is projected correctly, the kernel is the single source of
/// truth and cold-relaunch / cross-update stale-value bugs cannot occur.
@MainActor
final class SettingsProjectionParityTests: XCTestCase {

    private var store: AppStateStore!
    private var storeFileURL: URL!

    override func setUp() async throws {
        (store, storeFileURL) = AppStateTestSupport.makeIsolatedStore()
    }

    override func tearDown() async throws {
        AppStateTestSupport.disposeIsolatedStore(at: storeFileURL)
        store = nil
    }

    // MARK: - Round-trip: kernel SettingsSnapshot → projectSnapshotDerivedState → AppState.settings

    func testAllNewlyProjectedFieldsRoundTrip() {
        // Build a SettingsSnapshot with non-default values for every field
        // added in the #561 seam fix.
        var ks = SettingsSnapshot()
        // blossomServerURL: default "https://blossom.primal.net"
        ks.blossomServerURL = "https://blossom.example.com"
        // youtubeExtractorURL: default nil
        ks.youtubeExtractorURL = "https://yt.example.com/extract"
        // autoDeleteDownloadsAfterPlayed: default false
        ks.autoDeleteDownloadsAfterPlayed = true
        // autoIngestPublisherTranscripts: default true
        ks.autoIngestPublisherTranscripts = false
        // autoFallbackToScribe: default true
        ks.autoFallbackToScribe = false
        // notifyOnNewEpisodes: default true
        ks.notifyOnNewEpisodes = false
        // nostrEnabled: default false
        ks.nostrEnabled = true
        // nostrRelayURL: default ""
        ks.nostrRelayURL = "wss://relay.example.com"
        // nostrProfileName: default ""
        ks.nostrProfileName = "TestAgent"
        // nostrProfileAbout: default ""
        ks.nostrProfileAbout = "Test bio"
        // nostrProfilePicture: default ""
        ks.nostrProfilePicture = "https://example.com/pic.jpg"
        // nostrPublicKeyHex: default nil — kernel-derived from Keychain
        ks.nostrPublicKeyHex = "aabb1122"

        var update = PodcastUpdate()
        update.settings = ks

        var appState = AppState()
        store.projectSnapshotDerivedState(into: &appState, snapshot: update)

        XCTAssertEqual(appState.settings.blossomServerURL, "https://blossom.example.com",
                       "blossomServerURL must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.youtubeExtractorURL, "https://yt.example.com/extract",
                       "youtubeExtractorURL must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.autoDeleteDownloadsAfterPlayed, true,
                       "autoDeleteDownloadsAfterPlayed must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.autoIngestPublisherTranscripts, false,
                       "autoIngestPublisherTranscripts must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.autoFallbackToScribe, false,
                       "autoFallbackToScribe must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.notifyOnNewEpisodes, false,
                       "notifyOnNewEpisodes must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.nostrEnabled, true,
                       "nostrEnabled must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.nostrRelayURL, "wss://relay.example.com",
                       "nostrRelayURL must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.nostrProfileName, "TestAgent",
                       "nostrProfileName must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.nostrProfileAbout, "Test bio",
                       "nostrProfileAbout must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.nostrProfilePicture, "https://example.com/pic.jpg",
                       "nostrProfilePicture must be projected from kernel snapshot")
        XCTAssertEqual(appState.settings.nostrPublicKeyHex, "aabb1122",
                       "nostrPublicKeyHex must be projected from kernel snapshot (read-only, kernel-derived)")
    }

    /// Verify that a nil youtubeExtractorURL projects back as nil (not a stale
    /// non-nil Swift-side default).
    func testYoutubeExtractorURLNilProjectedCorrectly() {
        var ks = SettingsSnapshot()
        ks.youtubeExtractorURL = nil

        var update = PodcastUpdate()
        update.settings = ks

        var appState = AppState()
        // Pre-seed a non-nil value to confirm the projection clears it.
        appState.settings.youtubeExtractorURL = "https://old.example.com"
        store.projectSnapshotDerivedState(into: &appState, snapshot: update)

        XCTAssertNil(appState.settings.youtubeExtractorURL,
                     "nil kernel youtubeExtractorURL must clear a pre-existing Swift value")
    }

    /// Verify that a nil nostrPublicKeyHex from the kernel (no key in Keychain)
    /// projects back as nil.
    func testNostrPublicKeyHexNilProjectedCorrectly() {
        var ks = SettingsSnapshot()
        ks.nostrPublicKeyHex = nil

        var update = PodcastUpdate()
        update.settings = ks

        var appState = AppState()
        appState.settings.nostrPublicKeyHex = "stale-key"
        store.projectSnapshotDerivedState(into: &appState, snapshot: update)

        XCTAssertNil(appState.settings.nostrPublicKeyHex,
                     "nil kernel nostrPublicKeyHex must clear a pre-existing Swift value")
    }

    func testLocalNotesProjectFromKernelSnapshot() {
        let episodeID = UUID()
        let friendID = UUID()
        let parentNoteID = UUID()

        var update = PodcastUpdate()
        update.notes = [
            NoteSummary(
                id: UUID().uuidString,
                text: "Episode note",
                kind: "reflection",
                target: NoteTargetSummary(
                    type: "episode",
                    episodeId: episodeID.uuidString,
                    positionSecs: 42.5
                ),
                createdAt: 123,
                deleted: false,
                author: "user"
            ),
            NoteSummary(
                id: UUID().uuidString,
                text: "Friend note",
                kind: "free",
                target: NoteTargetSummary(type: "friend", friendId: friendID.uuidString),
                createdAt: 456,
                deleted: true,
                author: "agent"
            ),
            NoteSummary(
                id: UUID().uuidString,
                text: "Nested note",
                kind: "systemEvent",
                target: NoteTargetSummary(type: "note", noteId: parentNoteID.uuidString),
                createdAt: 789,
                deleted: false,
                author: "agent"
            )
        ]

        var appState = AppState()
        appState.notes = [Note(text: "stale")]
        store.projectSnapshotDerivedState(into: &appState, snapshot: update)

        XCTAssertEqual(appState.notes.map(\.text), ["Episode note", "Friend note", "Nested note"])
        XCTAssertEqual(appState.notes[0].kind, .reflection)
        XCTAssertEqual(appState.notes[0].createdAt, Date(timeIntervalSince1970: 123))
        if case .episode(let id, let positionSeconds) = appState.notes[0].target {
            XCTAssertEqual(id, episodeID)
            XCTAssertEqual(positionSeconds, 42.5)
        } else {
            XCTFail("Episode note target must project as Anchor.episode")
        }
        XCTAssertTrue(appState.notes[1].deleted)
        XCTAssertEqual(appState.notes[1].author, .agent)
        if case .friend(let id) = appState.notes[1].target {
            XCTAssertEqual(id, friendID)
        } else {
            XCTFail("Friend note target must project as Anchor.friend")
        }
        if case .note(let id) = appState.notes[2].target {
            XCTAssertEqual(id, parentNoteID)
        } else {
            XCTFail("Nested note target must project as Anchor.note")
        }
    }

    func testEmptyKernelNotesProjectionClearsSwiftNotes() {
        var update = PodcastUpdate()
        update.notes = []

        var appState = AppState()
        appState.notes = [Note(text: "stale")]
        store.projectSnapshotDerivedState(into: &appState, snapshot: update)

        XCTAssertTrue(appState.notes.isEmpty)
    }

    func testFriendsProjectFromKernelSnapshot() {
        let friendID = UUID()
        var update = PodcastUpdate()
        update.friends = [
            FriendSummary(
                id: friendID.uuidString,
                displayName: "Alice",
                pubkeyHex: "aabbcc",
                addedAt: 123,
                avatarUrl: "https://example.com/alice.png",
                about: "Builds shows"
            )
        ]

        var appState = AppState()
        appState.friends = [Friend(displayName: "Stale", identifier: "old")]
        store.projectSnapshotDerivedState(into: &appState, snapshot: update)

        XCTAssertEqual(appState.friends.count, 1)
        XCTAssertEqual(appState.friends[0].id, friendID)
        XCTAssertEqual(appState.friends[0].displayName, "Alice")
        XCTAssertEqual(appState.friends[0].identifier, "aabbcc")
        XCTAssertEqual(appState.friends[0].addedAt, Date(timeIntervalSince1970: 123))
        XCTAssertEqual(appState.friends[0].avatarURL, "https://example.com/alice.png")
        XCTAssertEqual(appState.friends[0].about, "Builds shows")
    }

    func testEmptyKernelFriendsProjectionClearsSwiftFriends() {
        var update = PodcastUpdate()
        update.friends = []

        var appState = AppState()
        appState.friends = [Friend(displayName: "Stale", identifier: "old")]
        store.projectSnapshotDerivedState(into: &appState, snapshot: update)

        XCTAssertTrue(appState.friends.isEmpty)
    }
}
