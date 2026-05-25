// Compat shim — replaced at M1 exit when nmp-signer-broker drives identity state.
//
// The migrated Identity views were authored against the legacy
// `UserIdentityStore` `@Observable` and expect to read/write its published
// properties as well as invoke its sign-in / sign-out / publish methods.
//
// This shim mirrors that surface area but has no real implementation: every
// method is a no-op or throws `NotImplemented`. It exists solely so the M1
// view layer compiles. Functional sign-in is wired in a follow-up via the
// Rust kernel snapshot.

import Foundation
import Observation
import SwiftUI

// MARK: - Signer marker protocol

/// Marker protocol — `NostrSigner` is referenced as a type by
/// `BlossomUploading.upload(data:contentType:signer:)` and as the type of
/// `UserIdentityStore.signer`. No methods are called on it in the migrated
/// views, so an empty marker protocol is sufficient.
///
/// Declared `Sendable` so `ChangePhotoSheet.handlePicked` can pass it across
/// a `Task` boundary into `BlossomUploading.upload`.
protocol NostrSigner: AnyObject, Sendable {}

// MARK: - Remote signer state

enum RemoteSignerState: Sendable, Equatable {
    case idle
    case connecting
    case reconnecting
    case awaitingAuthorization(URL)
    case connected(String)
    case failed(String)
}

// MARK: - User identity store (compat)

/// Compat shim for the legacy `UserIdentityStore`. All state defaults to
/// signed-out; all sign-in / publish methods are no-ops or throw.
///
/// Replaced when functional sign-in lands as part of the M1 exit deliverable.
@MainActor
@Observable
final class UserIdentityStore {

    enum Mode: String, Sendable, Codable {
        case none
        case localKey
        case remoteSigner
    }

    // Published-equivalent state — `@Observable` makes plain `var` observable.
    var publicKeyHex: String?
    var profileDisplayName: String?
    var profileName: String?
    var profileAbout: String?
    var profilePicture: String?
    var loginError: String?
    private(set) var mode: Mode = .none
    private(set) var signer: (any NostrSigner)?
    private(set) var remoteSignerState: RemoteSignerState = .idle

    var hasIdentity: Bool { publicKeyHex != nil }
    var isRemoteSigner: Bool { mode == .remoteSigner }

    var npub: String? {
        // Compat stub — real npub encoding lands with functional sign-in.
        publicKeyHex
    }

    var npubShort: String? {
        guard let full = npub, full.count > 16 else { return npub }
        return "\(full.prefix(10))…\(full.suffix(6))"
    }

    // MARK: - Lifecycle no-ops

    /// Starts the identity store. M1 compat: no-op. Functional sign-in will
    /// wire this to the Rust kernel snapshot.
    func start() {}

    /// Clears the active identity. M1 compat: resets local state only.
    func clearIdentity() {
        publicKeyHex = nil
        profileDisplayName = nil
        profileName = nil
        profileAbout = nil
        profilePicture = nil
        signer = nil
        mode = .none
        remoteSignerState = .idle
    }

    // MARK: - Sign-in (stubs)

    func importNsec(_ nsec: String) throws {
        throw CompatError.notImplemented("UserIdentityStore.importNsec")
    }

    func generateKey() throws {
        throw CompatError.notImplemented("UserIdentityStore.generateKey")
    }

    func connectRemoteSigner(uri: String) async {
        remoteSignerState = .failed("Sign-in not yet wired in M1.E compat shim.")
    }

    func disconnectRemoteSigner() async {
        remoteSignerState = .idle
    }

    /// NIP-46 nostrconnect:// pairing initiator. Compat shim: invokes the URI
    /// callback with an empty placeholder so the QR view renders, then
    /// short-circuits without completing pairing.
    func connectViaNostrConnect(onURI: @escaping (String) async -> Void) async {
        await onURI("nostrconnect://placeholder?relay=&secret=&perms=&name=Podcastr")
        remoteSignerState = .failed("nostrconnect pairing not yet wired in M1.E compat shim.")
    }

    // MARK: - Profile publish (stub)

    @discardableResult
    func publishProfile(
        name: String,
        displayName: String,
        about: String,
        picture: String
    ) async throws -> String {
        // M1.E compat: store fields locally so onChange observers re-hydrate
        // the form, then throw so the calling view surfaces a "couldn't reach
        // the relay" banner. Real publish flow lands when the Rust kernel
        // exposes a kind-0 publish dispatch namespace.
        profileName = name
        profileDisplayName = displayName
        profileAbout = about
        profilePicture = picture
        throw CompatError.notImplemented("UserIdentityStore.publishProfile")
    }
}

// MARK: - Compat error

enum CompatError: LocalizedError {
    case notImplemented(String)

    var errorDescription: String? {
        switch self {
        case .notImplemented(let symbol):
            return "\(symbol) is not yet implemented in the M1.E compat shim."
        }
    }
}
