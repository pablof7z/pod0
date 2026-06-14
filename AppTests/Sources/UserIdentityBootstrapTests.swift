import Foundation
import XCTest
@testable import Podcastr

// MARK: - UserIdentityBootstrapTests
//
// Covers two bugs that produce a permanent "No Identity" state:
//
//  Bug 1 — Fresh-install auto-generation:
//    When the kernel delivers a snapshot with no active account (fresh install
//    or data-reset), `applyKernelIdentity` must fire exactly one keygen so the
//    user never lands in a permanent "No Identity" limbo. Subsequent nil ticks
//    must not repeat the request.
//
//  Bug 2 — "Generate Key Pair" button silently does nothing:
//    `generateKey()` dispatches `createNewAccount` to the kernel FFI but never
//    requested a snapshot pull. Without a pull the new pubkey only surfaces if
//    a *push* frame arrives; on a cold/slow kernel path that push may be
//    delayed, so `hasIdentity` never flips. Fix: call
//    `kernel?.requestSnapshotPull()` after the FFI dispatch. Same for `importNsec`.
//
// NOTE: `dispatchKernelKeygen()` bypasses `dispatchToKernel(namespace:body:)` and
// calls the kernel FFI directly, so KernelDispatchRecorder cannot intercept it.
// Instead we use the purpose-built `_keygenCallRecorder` and `_pullCallRecorder`
// seams on UserIdentityStore.

@MainActor
final class UserIdentityBootstrapTests: XCTestCase {

    private var storeFileURL: URL!
    private var store: AppStateStore!
    private var identity: UserIdentityStore!
    private var keygenCallCount = 0
    private var pullCallCount = 0

    override func setUp() async throws {
        try await super.setUp()
        let made = await AppStateTestSupport.makeIsolatedStore()
        storeFileURL = made.fileURL
        store = made.store
        identity = store.identity
        keygenCallCount = 0
        pullCallCount = 0
        identity._keygenCallRecorder = { [weak self] in
            self?.keygenCallCount += 1
        }
        identity._pullCallRecorder = { [weak self] in
            self?.pullCallCount += 1
        }
    }

    override func tearDown() async throws {
        identity._clearActiveAccountForTesting()
        identity._keygenCallRecorder = nil
        identity._pullCallRecorder = nil
        if let storeFileURL {
            AppStateTestSupport.disposeIsolatedStore(at: storeFileURL)
        }
        store = nil
        storeFileURL = nil
        identity = nil
        try await super.tearDown()
    }

    // MARK: - Bug 1: Auto-generation on fresh install / data reset

    /// The very first kernel tick with no active account must dispatch keygen
    /// so fresh-install users are not stuck at "No Identity" forever.
    func testFirstNilIdentityTickTriggersAutoKeygen() {
        identity.applyKernelIdentity(
            handshake: nil,
            activeNpub: nil,
            pubkeyHex: nil,
            isRemoteSigner: false
        )
        XCTAssertEqual(keygenCallCount, 1, "First nil tick must dispatch exactly one keygen.")
    }

    /// Subsequent nil ticks must NOT re-dispatch — one keygen is enough.
    func testRepeatedNilTicksDispatchKeygenOnlyOnce() {
        for _ in 0 ..< 5 {
            identity.applyKernelIdentity(
                handshake: nil, activeNpub: nil, pubkeyHex: nil, isRemoteSigner: false)
        }
        XCTAssertEqual(keygenCallCount, 1, "Only the first nil tick should dispatch keygen.")
    }

    /// An identity tick with a real pubkey must NEVER trigger auto-keygen;
    /// doing so would clobber an existing account on every cold start.
    func testIdentityTickWithExistingPubkeyDoesNotAutoKeygen() {
        let pubkey = String(repeating: "a", count: 64)
        identity.applyKernelIdentity(
            handshake: nil,
            activeNpub: "npub1test",
            pubkeyHex: pubkey,
            isRemoteSigner: false
        )
        XCTAssertEqual(keygenCallCount, 0, "Must not generate a new key when an account already exists.")
    }

    /// After the auto-keygen fires, a subsequent tick with a real pubkey must
    /// flip `hasIdentity` to true (simulates the kernel round-trip completing).
    func testAutoKeygenFollowedByKernelResponseSetsHasIdentity() {
        // Step 1: first nil tick fires keygen.
        identity.applyKernelIdentity(
            handshake: nil, activeNpub: nil, pubkeyHex: nil, isRemoteSigner: false)
        XCTAssertEqual(keygenCallCount, 1)

        // Step 2: kernel responds with the new account (simulated).
        let newPubkey = String(repeating: "b", count: 64)
        identity.applyKernelIdentity(
            handshake: nil,
            activeNpub: "npub1newkey",
            pubkeyHex: newPubkey,
            isRemoteSigner: false
        )

        XCTAssertTrue(identity.hasIdentity, "Identity must be set after the kernel response arrives.")
        XCTAssertEqual(identity.publicKeyHex, newPubkey)
    }

    /// After `clearIdentity()`, the next nil tick must re-allow auto-keygen
    /// so a sign-out + sign-in flow isn't permanently locked out.
    func testClearIdentityResetsAutoKeygenGuard() {
        // Fire first auto-keygen.
        identity.applyKernelIdentity(
            handshake: nil, activeNpub: nil, pubkeyHex: nil, isRemoteSigner: false)
        XCTAssertEqual(keygenCallCount, 1)

        // Real clearIdentity() must reset _autoKeygenDispatched (kernel is nil
        // in tests so clearIdentityInKernel dispatches silently no-op).
        identity.clearIdentity()
        keygenCallCount = 0

        // The next nil tick must re-trigger auto-keygen.
        identity.applyKernelIdentity(
            handshake: nil, activeNpub: nil, pubkeyHex: nil, isRemoteSigner: false)
        XCTAssertEqual(
            keygenCallCount, 1,
            "After clearIdentity, the next nil tick must re-trigger auto-keygen.")
    }

    // MARK: - Bug 2: generateKey and importNsec must request a snapshot pull

    /// `generateKey()` must dispatch a keygen AND request a snapshot pull.
    /// Without the pull, the new pubkey only surfaces if a push frame arrives
    /// in time — which is not guaranteed on a cold/slow kernel.
    func testGenerateKeyDispatchesKeygenAndRequestsPull() throws {
        XCTAssertFalse(identity.hasIdentity, "Precondition: no identity before test.")
        try identity.generateKey()
        XCTAssertEqual(keygenCallCount, 1, "generateKey must dispatch keygen.")
        XCTAssertEqual(pullCallCount, 1, "generateKey must request a snapshot pull.")
    }

    /// `importNsec` must also request a snapshot pull so the imported
    /// pubkey surfaces reactively without waiting for a push frame.
    func testImportNsecRequestsSnapshotPull() {
        let fakeNsec = "nsec1vl029mgpspedva04g90vltkh6fvh240zqtv9k3lvlrc3a0z4pfc8vqfeg"
        try? identity.importNsec(fakeNsec)
        XCTAssertEqual(pullCallCount, 1, "importNsec must request a snapshot pull.")
    }

    // MARK: - Remote signer pairing state

    func testRemoteSignerConnectStaysPendingUntilRemoteAccountArrives() async throws {
        identity._remoteSignerConnectTimeoutNanoseconds = 500_000_000

        await identity.connectRemoteSigner(uri: "bunker://example.test?relay=wss://relay.example.test&secret=abc")

        XCTAssertEqual(identity.remoteSignerState, .connecting)

        // A nil tick can arrive while the NIP-46 broker is still pairing.
        // It must not auto-generate a local key or clear the spinner.
        identity.applyKernelIdentity(
            handshake: nil,
            activeNpub: nil,
            pubkeyHex: nil,
            isRemoteSigner: false
        )
        XCTAssertEqual(identity.remoteSignerState, .connecting)
        XCTAssertEqual(keygenCallCount, 0, "Remote-signer pairing must suppress fresh-install auto-keygen.")

        // A stale local-key tick is also possible during the same window.
        // It must not downgrade the remote signer attempt to idle.
        let localPubkey = String(repeating: "c", count: 64)
        identity.applyKernelIdentity(
            handshake: nil,
            activeNpub: "npub1local",
            pubkeyHex: localPubkey,
            isRemoteSigner: false
        )
        XCTAssertEqual(identity.remoteSignerState, .connecting)

        let remotePubkey = String(repeating: "d", count: 64)
        identity.applyKernelIdentity(
            handshake: nil,
            activeNpub: "npub1remote",
            pubkeyHex: remotePubkey,
            isRemoteSigner: true
        )

        XCTAssertEqual(identity.remoteSignerState, .connected(remotePubkey))
        try await Task.sleep(nanoseconds: 600_000_000)
        XCTAssertEqual(identity.remoteSignerState, .connected(remotePubkey), "Terminal success must cancel the timeout.")
    }

    func testRemoteSignerConnectTimesOutWithoutTerminalKernelState() async throws {
        identity._remoteSignerConnectTimeoutNanoseconds = 20_000_000

        await identity.connectRemoteSigner(uri: "bunker://example.test?relay=wss://relay.example.test&secret=abc")

        XCTAssertEqual(identity.remoteSignerState, .connecting)
        let message = try await waitForRemoteSignerFailure()
        XCTAssertEqual(message, "Remote signer connection timed out.")
    }

    private func waitForRemoteSignerFailure(
        timeoutNanoseconds: UInt64 = 500_000_000
    ) async throws -> String {
        let deadline = Date().addingTimeInterval(Double(timeoutNanoseconds) / 1_000_000_000)
        while Date() < deadline {
            if case .failed(let message) = identity.remoteSignerState {
                return message
            }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTFail("Timed out waiting for remote signer failure.")
        return ""
    }
}
